use tauri::{
    webview::{NewWindowResponse, WebviewWindowBuilder},
    AppHandle, Manager, Url, WebviewUrl,
};

pub const LOGIN_WINDOW_LABEL: &str = "duolingo-login";
const LOGIN_URL: &str = "https://www.duolingo.com/log-in";
const COOKIE_URL: &str = "https://www.duolingo.com";

pub fn open(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LOGIN_WINDOW_LABEL) {
        window.show().map_err(|_| "无法显示登录窗口".to_string())?;
        window
            .set_focus()
            .map_err(|_| "无法聚焦登录窗口".to_string())?;
        return Ok(());
    }

    let url = LOGIN_URL
        .parse::<Url>()
        .map_err(|_| "Duolingo 登录地址无效".to_string())?;
    let data_directory = app
        .path()
        .app_cache_dir()
        .map_err(|_| "无法创建隔离的登录数据目录".to_string())?
        .join("login-webview");
    WebviewWindowBuilder::new(app, LOGIN_WINDOW_LABEL, WebviewUrl::External(url))
        .title("登录 Duolingo · DuoPing")
        .inner_size(520.0, 720.0)
        .min_inner_size(420.0, 560.0)
        .center()
        .data_directory(data_directory)
        .on_navigation(is_allowed_navigation)
        .on_new_window(|url, _features| {
            if is_allowed_navigation(&url) {
                NewWindowResponse::Allow
            } else {
                NewWindowResponse::Deny
            }
        })
        .build()
        .map_err(|error| format!("无法打开 Duolingo 登录窗口：{error}"))?;
    Ok(())
}

pub fn token(app: &AppHandle) -> Result<Option<String>, String> {
    let window = app
        .get_webview_window(LOGIN_WINDOW_LABEL)
        .ok_or_else(|| "登录窗口已关闭".to_string())?;
    let url = COOKIE_URL
        .parse::<Url>()
        .map_err(|_| "Duolingo Cookie 地址无效".to_string())?;
    let cookies = window
        .cookies_for_url(url)
        .map_err(|_| "暂时无法读取 Duolingo 登录状态".to_string())?;
    Ok(cookies
        .into_iter()
        .find(|cookie| cookie.name() == "jwt_token")
        .map(|cookie| cookie.value().to_string())
        .filter(|value| !value.is_empty()))
}

pub fn close_and_clear(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LOGIN_WINDOW_LABEL) {
        if let Err(error) = window.clear_all_browsing_data() {
            log::warn!("failed to clear login webview data: {error}");
        }
        let _ = window.destroy();
    }
}

fn is_allowed_navigation(url: &Url) -> bool {
    if url.as_str() == "about:blank" {
        return true;
    }
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    is_host_or_subdomain(host, "duolingo.com")
        || is_host_or_subdomain(host, "google.com")
        || is_host_or_subdomain(host, "googleusercontent.com")
        || is_host_or_subdomain(host, "apple.com")
        || is_host_or_subdomain(host, "facebook.com")
}

fn is_host_or_subdomain(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

#[cfg(test)]
mod tests {
    use super::is_allowed_navigation;

    #[test]
    fn allows_duolingo_and_known_identity_providers() {
        assert!(is_allowed_navigation(
            &"https://www.duolingo.com/log-in".parse().unwrap()
        ));
        assert!(is_allowed_navigation(
            &"https://accounts.google.com/o/oauth2/auth".parse().unwrap()
        ));
    }

    #[test]
    fn blocks_lookalike_hosts_and_insecure_navigation() {
        assert!(!is_allowed_navigation(
            &"https://duolingo.com.attacker.example/".parse().unwrap()
        ));
        assert!(!is_allowed_navigation(
            &"http://www.duolingo.com/log-in".parse().unwrap()
        ));
    }

    #[test]
    fn allows_only_the_safe_initial_blank_page() {
        assert!(is_allowed_navigation(&"about:blank".parse().unwrap()));
        assert!(!is_allowed_navigation(
            &"data:text/html,hello".parse().unwrap()
        ));
    }
}
