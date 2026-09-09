# DuoPing 开发计划

## 产品边界

v0.1 面向单台 Windows 电脑上的单个 Duolingo 账号，以用户设置的每日 XP 为完成标准。应用只读取学习数据，不提交课程、练习或社交操作。历史趋势、具体每日任务、排行榜、云同步、自动更新和其他桌面平台不在首版范围内。

## 架构

- React/TypeScript：状态与设置界面，只通过 Tauri Commands 访问能力。
- Rust/Tauri：凭据、HTTP 适配器、状态、调度、通知和系统集成。
- `DuolingoProvider`：隔离非官方 API；原始 JSON 不进入前端。
- Tauri Store：保存非敏感设置与当天状态；Windows Credential Manager：保存 JWT。

## 里程碑

| 里程碑 | 内容 | 完成条件 |
| --- | --- | --- |
| M0 | 工程、文档、API 样本、CI | 工程可构建，契约测试覆盖 XP 响应 |
| M1 | Provider、凭据、设置、状态页 | 会话可验证并展示当天 XP |
| M2 | 调度、托盘、通知、开机启动 | 未达标按时间通知且不重复 |
| M3 | 安装包、隐私、真机验收 | Windows NSIS 完成验收并发布 v0.1.0 |

## 应用内自动登录

- 在 DuoPing 中打开隔离的 Tauri WebView，加载 Duolingo 官方登录页；用户密码只提交给 Duolingo，客户端不读取登录表单。
- 登录成功后轮询读取 WebView 的 `jwt_token` Cookie，验证账号并保存到 Windows Credential Manager，然后关闭窗口并清理临时浏览数据。
- 登录窗口只允许 Duolingo 必需域名，拦截未知跳转；保留手动导入 JWT 作为兼容和故障回退入口。
- 登录 WebView 使用隐私模式，与外部 Chrome 及主窗口存储隔离；Duolingo 没有公开 OAuth 回调，因此不使用外部浏览器自动回传客户端。
- Windows 安装版必须验证账号密码登录以及 Google/Apple 等第三方登录是否允许嵌入式 WebView；会话检测通过异步命令轮询，避免在 WebView2 主线程同步读取 Cookie。

## 发布门槛

TypeScript 类型检查、Vitest、Rust fmt、Clippy、Rust tests 和 Windows release build 全部通过；安装版完成托盘、通知、开机启动、休眠恢复、跨日和卸载测试。
