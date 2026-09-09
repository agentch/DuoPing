mod credential;
mod duolingo;
mod model;
mod scheduler;

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{Local, Utc};
use duolingo::{DuolingoProvider, HttpDuolingoProvider, ProviderError};
use model::{AppSettings, CheckResult, DailyStatus, Freshness};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, State, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_store::StoreExt;
use tokio::sync::Mutex;

const DUOLINGO_LEARN_URL: &str = "https://www.duolingo.com/learn";

pub struct AppState {
    settings: Mutex<AppSettings>,
    status: Mutex<DailyStatus>,
    executed: Mutex<HashSet<String>>,
    auth_paused: Mutex<bool>,
    tray_status: Mutex<Option<tauri::menu::MenuItem<tauri::Wry>>>,
    provider: HttpDuolingoProvider,
}

impl AppState {
    fn new(settings: AppSettings, status: DailyStatus) -> Self {
        Self {
            settings: Mutex::new(settings),
            status: Mutex::new(status),
            executed: Mutex::new(HashSet::new()),
            auth_paused: Mutex::new(false),
            tray_status: Mutex::new(None),
            provider: HttpDuolingoProvider::default(),
        }
    }
}

fn to_result(error: ProviderError) -> CheckResult {
    match error {
        ProviderError::NotAuthenticated => CheckResult::NotAuthenticated {
            message: error.to_string(),
        },
        ProviderError::RateLimited => CheckResult::RateLimited {
            message: error.to_string(),
        },
        ProviderError::Network => CheckResult::NetworkError {
            message: error.to_string(),
        },
        ProviderError::UpstreamChanged => CheckResult::UpstreamChanged {
            message: error.to_string(),
        },
    }
}

fn persist_status(app: &AppHandle, status: &DailyStatus) {
    if let Ok(store) = app.store("duoping.json") {
        if let Ok(value) = serde_json::to_value(status) {
            store.set("dailyStatus", value);
            if let Err(error) = store.save() {
                log::warn!("failed to persist daily status: {error}");
            }
        }
    }
}

#[cfg(windows)]
fn send_actionable_notification(
    app: &AppHandle,
    title: impl Into<String>,
    body: impl Into<String>,
) -> Result<(), String> {
    use notify_rust::{Notification, NotificationResponse};

    let mut notification = Notification::new();
    notification
        .summary(&title.into())
        .body(&body.into())
        .app_id(&app.config().identifier)
        .action("learn", "立即学习");
    let handle = notification
        .show()
        .map_err(|error| format!("无法发送 Windows 通知：{error}"))?;
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = handle.wait_for_response(move |response| match response {
            NotificationResponse::Default => show_window(&app),
            NotificationResponse::Action(action) if action == "learn" => {
                let _ = app.opener().open_url(DUOLINGO_LEARN_URL, None::<&str>);
            }
            _ => {}
        });
    });
    Ok(())
}

#[cfg(not(windows))]
fn send_actionable_notification(
    app: &AppHandle,
    title: impl Into<String>,
    body: impl Into<String>,
) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title.into())
        .body(body.into())
        .show()
        .map_err(|error| format!("无法发送系统通知：{error}"))
}

async fn perform_check(app: &AppHandle, state: &Arc<AppState>, notify: bool) -> CheckResult {
    let token = match credential::load() {
        Ok(Some(value)) => value,
        Ok(None) | Err(_) => {
            return CheckResult::NotAuthenticated {
                message: "请先导入 Duolingo 会话".into(),
            }
        }
    };
    let user = match state.provider.validate_session(&token).await {
        Ok(user) => user,
        Err(error) => {
            if error == ProviderError::NotAuthenticated {
                *state.auth_paused.lock().await = true;
                if notify {
                    let _ = send_actionable_notification(
                        app,
                        "DuoPing · 会话已失效",
                        "请打开 DuoPing，重新导入 Duolingo 会话。",
                    );
                }
            }
            return to_result(error);
        }
    };
    let today = Local::now().date_naive();
    let xp = match state.provider.daily_xp(&token, &user, today).await {
        Ok(xp) => xp,
        Err(error) => {
            if error == ProviderError::NotAuthenticated {
                *state.auth_paused.lock().await = true;
                if notify {
                    let _ = send_actionable_notification(
                        app,
                        "DuoPing · 会话已失效",
                        "请打开 DuoPing，重新导入 Duolingo 会话。",
                    );
                }
            }
            return to_result(error);
        }
    };
    let settings = state.settings.lock().await.clone();
    let status = DailyStatus {
        date: today,
        current_xp: xp,
        target_xp: settings.daily_xp_goal,
        completed: xp >= settings.daily_xp_goal,
        last_successful_check: Some(Utc::now()),
        freshness: Freshness::Fresh,
        username: Some(user.username),
    };
    *state.status.lock().await = status.clone();
    if let Some(item) = state.tray_status.lock().await.as_ref() {
        let _ = item.set_text(format!(
            "今日 XP：{} / {}",
            status.current_xp, status.target_xp
        ));
    }
    persist_status(app, &status);
    if notify && !status.completed {
        let remaining = status.target_xp.saturating_sub(status.current_xp);
        if let Err(error) = send_actionable_notification(
            app,
            "DuoPing · 今日目标还差一点",
            format!(
                "今天已完成 {} XP，还差 {} XP。",
                status.current_xp, remaining
            ),
        ) {
            log::warn!("failed to show notification: {error}");
        }
    }
    CheckResult::Success { status }
}

#[tauri::command]
async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<AppSettings, String> {
    Ok(state.settings.lock().await.clone())
}

#[tauri::command]
async fn update_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let settings = settings.normalize()?;
    let autostart = app.autolaunch();
    let autostart_enabled = autostart
        .is_enabled()
        .map_err(|_| "无法读取开机启动状态".to_string())?;
    if settings.autostart != autostart_enabled {
        if settings.autostart {
            autostart.enable()
        } else {
            autostart.disable()
        }
        .map_err(|_| "无法更新开机启动设置".to_string())?;
    }
    let store = app
        .store("duoping.json")
        .map_err(|_| "无法打开设置存储".to_string())?;
    store.set(
        "settings",
        serde_json::to_value(&settings).map_err(|_| "无法序列化设置".to_string())?,
    );
    store.save().map_err(|_| "无法保存设置".to_string())?;
    {
        let mut status = state.status.lock().await;
        status.target_xp = settings.daily_xp_goal;
        status.completed = status.current_xp >= settings.daily_xp_goal;
    }
    *state.settings.lock().await = settings.clone();
    Ok(settings)
}

#[tauri::command]
async fn get_status(state: State<'_, Arc<AppState>>) -> Result<DailyStatus, String> {
    let mut status = state.status.lock().await;
    status.roll_to_today();
    if status
        .last_successful_check
        .is_some_and(|value| Utc::now().signed_duration_since(value).num_minutes() > 30)
    {
        status.freshness = Freshness::Stale;
    }
    Ok(status.clone())
}

#[tauri::command]
async fn check_now(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<CheckResult, String> {
    Ok(perform_check(&app, state.inner(), false).await)
}

#[tauri::command]
async fn import_session(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    token: String,
) -> Result<CheckResult, String> {
    let token = token.trim();
    if token.len() > 8192 {
        return Err("会话令牌长度异常".into());
    }
    let user = match state.provider.validate_session(token).await {
        Ok(user) => user,
        Err(error) => return Ok(to_result(error)),
    };
    credential::save(token)?;
    *state.auth_paused.lock().await = false;
    let today = Local::now().date_naive();
    match state.provider.daily_xp(token, &user, today).await {
        Ok(xp) => {
            let settings = state.settings.lock().await.clone();
            let status = DailyStatus {
                date: today,
                current_xp: xp,
                target_xp: settings.daily_xp_goal,
                completed: xp >= settings.daily_xp_goal,
                last_successful_check: Some(Utc::now()),
                freshness: Freshness::Fresh,
                username: Some(user.username),
            };
            *state.status.lock().await = status.clone();
            persist_status(&app, &status);
            Ok(CheckResult::Success { status })
        }
        Err(error) => Ok(to_result(error)),
    }
}

#[tauri::command]
async fn clear_session(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    credential::clear()?;
    let target = state.settings.lock().await.daily_xp_goal;
    let status = DailyStatus::empty(target);
    *state.status.lock().await = status.clone();
    persist_status(&app, &status);
    *state.auth_paused.lock().await = true;
    Ok(())
}

#[tauri::command]
fn test_notification(app: AppHandle) -> Result<(), String> {
    send_actionable_notification(&app, "DuoPing", "提醒已准备好。点击通知可以打开 DuoPing。")
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let current = MenuItem::with_id(app, "status", "今日 XP：—", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "打开 DuoPing", true, None::<&str>)?;
    let check = MenuItem::with_id(app, "check", "立即检查", true, None::<&str>)?;
    let duolingo = MenuItem::with_id(app, "duolingo", "打开 Duolingo", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&current, &open, &check, &duolingo, &quit])?;
    let state = app.state::<Arc<AppState>>().inner().clone();
    tauri::async_runtime::block_on(async {
        *state.tray_status.lock().await = Some(current);
    });
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".into()))?;
    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("DuoPing")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_window(app),
            "duolingo" => {
                let _ = app.opener().open_url(DUOLINGO_LEARN_URL, None::<&str>);
            }
            "check" => {
                let app = app.clone();
                let state = app.state::<Arc<AppState>>().inner().clone();
                tauri::async_runtime::spawn(async move {
                    let _ = perform_check(&app, &state, false).await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_window(app)
        }))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .setup(|app| {
            let store = app.store("duoping.json")?;
            let settings = store
                .get("settings")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .and_then(|value: AppSettings| value.normalize().ok())
                .unwrap_or_default();
            let mut status = store
                .get("dailyStatus")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_else(|| DailyStatus::empty(settings.daily_xp_goal));
            status.roll_to_today();
            let state = Arc::new(AppState::new(settings, status));
            app.manage(state.clone());
            setup_tray(app.handle())?;
            scheduler::start(app.handle().clone(), state);
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Resized(_) if window.is_minimized().unwrap_or(false) => {
                let _ = window.hide();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            update_settings,
            get_status,
            check_now,
            import_session,
            clear_session,
            test_notification
        ])
        .run(tauri::generate_context!())
        .expect("error while running DuoPing");
}
