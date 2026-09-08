#[cfg(windows)]
const SERVICE: &str = "com.duoping.desktop";
#[cfg(windows)]
const ACCOUNT: &str = "duolingo-jwt";

#[cfg(windows)]
pub fn save(token: &str) -> Result<(), String> {
    keyring::Entry::new(SERVICE, ACCOUNT)
        .map_err(|_| "无法访问 Windows 凭据库".to_string())?
        .set_password(token)
        .map_err(|_| "无法保存会话到 Windows 凭据库".to_string())
}

#[cfg(windows)]
pub fn load() -> Result<Option<String>, String> {
    let entry =
        keyring::Entry::new(SERVICE, ACCOUNT).map_err(|_| "无法访问 Windows 凭据库".to_string())?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("无法读取 Windows 凭据库".into()),
    }
}

#[cfg(windows)]
pub fn clear() -> Result<(), String> {
    let entry =
        keyring::Entry::new(SERVICE, ACCOUNT).map_err(|_| "无法访问 Windows 凭据库".to_string())?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("无法从 Windows 凭据库清除会话".into()),
    }
}

// v0.1 is released for Windows only. Failing closed on other systems prevents
// credentials from silently falling back to an unencrypted file.
#[cfg(not(windows))]
pub fn save(_token: &str) -> Result<(), String> {
    Err("当前版本仅在 Windows 上支持安全会话存储".into())
}
#[cfg(not(windows))]
pub fn load() -> Result<Option<String>, String> {
    Ok(None)
}
#[cfg(not(windows))]
pub fn clear() -> Result<(), String> {
    Ok(())
}
