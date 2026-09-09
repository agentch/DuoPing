# 隐私说明

DuoPing 是本地运行的个人工具，不设服务器、不收集遥测。普通设置与当天 XP 状态保存在 Tauri 应用数据目录；Duolingo JWT 仅保存在 Windows Credential Manager。

应用内登录使用独立数据目录的 WebView 直接加载 Duolingo 登录页。账号密码提交给 Duolingo，DuoPing 不读取登录表单；登录完成或取消后会清理登录窗口的浏览数据。登录完成时仅提取 `jwt_token`，验证并保存会话。登录窗口限制顶层导航到 Duolingo 及已知身份提供商域名。

应用会向 `www.duolingo.com` 发起只读 HTTPS 请求，并把 JWT 放在 Authorization 请求头中。令牌不会显示回界面、写入日志或放入普通配置。清除会话会删除 Credential Manager 中的对应项。

卸载程序是否清除 Windows Credential Manager 项取决于安装器行为；卸载前应先在设置中点击“清除本机会话”。
