# 隐私说明

DuoPing 是本地运行的个人工具，不设服务器、不收集遥测。普通设置与当天 XP 状态保存在 Tauri 应用数据目录；Duolingo JWT 仅保存在 Windows Credential Manager。

应用会向 `www.duolingo.com` 发起只读 HTTPS 请求，并把 JWT 放在 Authorization 请求头中。令牌不会显示回界面、写入日志或放入普通配置。清除会话会删除 Credential Manager 中的对应项。

卸载程序是否清除 Windows Credential Manager 项取决于安装器行为；卸载前应先在设置中点击“清除本机会话”。

