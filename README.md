# DuoPing

DuoPing 是一款轻量的 Windows Duolingo 每日 XP 提醒工具。它在本机按自定义时间检查当天 XP，未完成目标时发送 Windows 通知，完成后停止当天提醒。

> DuoPing 是非官方个人工具，与 Duolingo, Inc. 无隶属或认可关系。Duolingo 没有稳定的公开开发者 API，上游接口变化可能导致查询暂时不可用。

## v0.1 功能

- 每日 XP 目标与多个自定义检查时间
- Windows 托盘、原生通知、开机启动、单实例
- 启动静默刷新、休眠恢复后补检、失败有限重试
- 应用内 Duolingo 登录及 JWT 手动回退；令牌只进入 Windows Credential Manager
- 中文状态页、明确的认证/限流/网络/接口变化错误

## 开发环境

需要 Node.js LTS、npm、Rust stable、Windows WebView2，以及 [Tauri 的 Windows 前置依赖](https://v2.tauri.app/start/prerequisites/#windows)。

```bash
npm install
npm run test
npm run tauri dev
```

构建当前用户级 NSIS 安装包：

```bash
npm run tauri build
```

也可以在 GitHub 仓库的 **Actions → CI → Run workflow** 手动构建。任务完成后，在该次运行页面底部的 Artifacts 区域下载 `DuoPing-Windows-NSIS-*`；构建产物保留 30 天。

## 登录

在设置页点击“登录 Duolingo”，并在独立隐私窗口中完成登录。DuoPing 检测到有效会话后会自动关闭登录窗口并更新今日状态；账号密码不会进入 DuoPing 前端或日志。

如果嵌入式登录受到 Duolingo 或第三方登录限制，可以手动导入会话：

1. 在浏览器登录 `duolingo.com`。
2. 打开开发者工具 → Application/应用 → Cookies → `https://www.duolingo.com`。
3. 复制 `jwt_token` 的值，在 DuoPing 设置页选择“导入并验证”。

不要分享该令牌。DuoPing 不接收账号密码，也不会自动读取浏览器 Cookie。

## 项目状态

参见 [开发计划](docs/development-plan.md)、[进度记录](docs/progress.md)、[接口研究](docs/api-research.md) 和 [隐私说明](docs/privacy.md)。

## License

[MIT](LICENSE)
