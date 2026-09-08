# 开发进度

更新日期：2026-09-08

| 里程碑 | 状态 | 说明 |
| --- | --- | --- |
| M0 立项与验证 | 进行中 | 工程、文档、CI、RsProxy 与脱敏 API 契约测试已建立；前后端构建通过，等待有效个人会话实测 |
| M1 核心数据与设置 | 已实现 | Provider、凭据库、设置、手动检查、错误分类及状态 UI 已落地 |
| M2 后台提醒 | 已实现，待 Windows 验收 | 固定时间、跨日、休眠补检、重试、托盘、通知、单实例及开机启动已落地 |
| M3 发布候选 | 进行中 | 图标、隐私、故障排查与 NSIS 配置已完成；Windows 真机与签名发布待验收 |

## 验证记录

- `npm test`：3 个前端测试通过。
- `npm run build`：TypeScript 与 Vite 生产构建通过。
- `cargo fmt --check`：通过。
- `cargo clippy --all-targets -- -D warnings`：通过。
- `cargo test`：6 个 Rust 测试通过。
- `npm run tauri build -- --no-bundle`：WSL release 应用构建通过。

## 当前限制

- 当前开发环境为 WSL；Windows 原生通知、Credential Manager、WebView2、托盘和 NSIS 安装流程必须在 Windows 环境验收。
- 尚未使用真实个人 JWT 调用接口；响应兼容性目前由社区资料和脱敏夹具验证。
- Duolingo 非官方接口没有稳定性承诺，Provider 需要随上游变化维护。
