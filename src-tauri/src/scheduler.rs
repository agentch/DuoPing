use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use crate::{perform_check, AppState};
use chrono::{Local, Timelike};
use tauri::AppHandle;

pub fn start(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        // A silent startup refresh makes the dashboard useful without creating a notification.
        if crate::credential::load().ok().flatten().is_some() {
            let _ = perform_check(&app, &state, false).await;
        }
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            if *state.auth_paused.lock().await {
                continue;
            }
            let settings = state.settings.lock().await.clone();
            let today = Local::now().date_naive();
            {
                let mut status = state.status.lock().await;
                status.roll_to_today();
            }
            let now = format!("{:02}:{:02}", Local::now().hour(), Local::now().minute());
            let mut executed = state.executed.lock().await;
            executed.retain(|key| key.starts_with(&today.to_string()));
            let pending: Vec<_> = settings
                .check_times
                .iter()
                .filter(|time| time.as_str() <= now.as_str())
                .filter(|time| !executed.contains(&format!("{today}:{time}")))
                .cloned()
                .collect();
            if pending.is_empty() {
                continue;
            }
            let latest = latest_due(&settings.check_times, &now, &executed, &today.to_string())
                .expect("pending slots imply a latest due slot");
            // Mark all elapsed slots so a wake from sleep only catches up the latest one.
            for time in &pending {
                executed.insert(format!("{today}:{time}"));
            }
            drop(executed);
            let completed = state.status.lock().await.completed;
            if settings.skip_if_completed && completed {
                continue;
            }

            let app_for_check = app.clone();
            let state_for_check = state.clone();
            tauri::async_runtime::spawn(async move {
                for (attempt, delay) in [0_u64, 30, 120].into_iter().enumerate() {
                    if delay > 0 {
                        tokio::time::sleep(Duration::from_secs(delay)).await;
                    }
                    let result = perform_check(&app_for_check, &state_for_check, true).await;
                    if matches!(
                        result,
                        crate::model::CheckResult::Success { .. }
                            | crate::model::CheckResult::NotAuthenticated { .. }
                    ) {
                        break;
                    }
                    log::warn!(
                        "scheduled check retry {} failed for slot {}",
                        attempt + 1,
                        latest
                    );
                }
            });
        }
    });
}

pub fn latest_due(
    times: &[String],
    now: &str,
    executed: &HashSet<String>,
    date: &str,
) -> Option<String> {
    times
        .iter()
        .rfind(|time| time.as_str() <= now && !executed.contains(&format!("{date}:{time}")))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wakeup_chooses_only_latest_missed_slot() {
        let times = vec!["12:00".into(), "18:00".into(), "22:00".into()];
        assert_eq!(
            latest_due(&times, "20:00", &HashSet::new(), "2026-09-08"),
            Some("18:00".into())
        );
    }

    #[test]
    fn executed_slot_is_not_due_again() {
        let executed = HashSet::from(["2026-09-08:18:00".to_string()]);
        let times = vec!["18:00".into()];
        assert_eq!(latest_due(&times, "18:30", &executed, "2026-09-08"), None);
    }
}
