# Duolingo API 研究记录

## 结论

Duolingo 未提供供普通第三方应用使用的稳定公开 API。DuoPing 使用只读内部接口，所有路径与字段都视为不稳定，并集中封装在 `DuolingoProvider`。

## v0.1 调用

1. JWT payload 中的 `sub` 作为用户 ID；只解析 payload 以确定请求路径，服务端请求仍负责验证令牌。
2. `GET /2017-06-30/users/{userId}?fields=id,username` 验证会话并取得用户名。
3. `GET /2017-06-30/users/{userId}/xp_summaries?startDate=YYYY-MM-DD&endDate=YYYY-MM-DD` 取得账户级每日 XP。
4. 认证统一使用 `Authorization: Bearer <jwt_token>`。

响应兼容 `summaries`/`xpSummaries`、时间戳/日期字符串以及 `gainedXp`/`xp`。未知结构返回“上游接口变化”，绝不默认为零；只有明确的空摘要才表示 0 XP。

## 风险控制

- 只读请求、15 秒超时、有限重试，不实现课程提交或 XP 修改。
- 401/403 暂停计划检查；429 单独呈现；5xx/连接失败归为网络错误。
- JWT 不记录、不写入普通 Store、不返回前端；Windows Credential Manager 是唯一持久化位置。
- 应用内登录从隔离 WebView 的 Duolingo Cookie 存储读取 `jwt_token`；读取发生在 Rust 异步命令中，令牌不经过 React 状态。
- 发布前用用户本人令牌验证，测试与问题报告中只能保留脱敏响应。

参考：[社区 API 说明](https://github.com/api-evangelist/duolingo/blob/main/graphql/duolingo-graphql.md)、[近期 XP endpoint 调研](https://github.com/csutaria/duolingo-dash/blob/main/docs/api-map.md)。
