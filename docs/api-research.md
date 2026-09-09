# Duolingo API 研究记录

## 结论

Duolingo 未提供供普通第三方应用使用的稳定公开 API。DuoPing 使用只读内部接口，所有路径与字段都视为不稳定，并集中封装在 `DuolingoProvider`。

## XP 调用

1. JWT payload 中的 `sub` 作为用户 ID；只解析 payload 以确定请求路径，服务端请求仍负责验证令牌。
2. `GET /2017-06-30/users/{userId}?fields=id,username` 验证会话并取得用户名。
3. `GET /2017-06-30/users/{userId}/xp_summaries?startDate=YYYY-MM-DD&endDate=YYYY-MM-DD` 取得账户级每日 XP。
4. 认证统一使用 `Authorization: Bearer <jwt_token>`。

响应兼容 `summaries`/`xpSummaries`、时间戳/日期字符串以及 `gainedXp`/`xp`。未知结构返回“上游接口变化”，绝不默认为零；只有明确的空摘要才表示 0 XP。

## 每日任务进度

每日任务使用独立的非官方 Goals 服务，只读请求如下：

1. `GET https://goals-api.duolingo.com/schema?ui_language=en`：读取目标定义。
2. `GET https://goals-api.duolingo.com/users/{userId}/progress?timezone={IANA 时区}&ui_language=en`：读取当日目标进度和已获徽章。

请求同时携带 Bearer JWT、`x-requested-with: XMLHttpRequest` 与 JSON Accept header，并使用 `ui_language=zh` 请求可见的中文任务名称。Schema 是包含历史徽章的完整目录，因此适配器只接受 `goalId` 存在于本次 `goals.progress` 中的当前激活目标，并按分类与 ID 共同区分：`FRIEND` 为好友任务；所有形如 `YYYY_MM_...` 的 ID 无论分类为何都作为月度任务，且只保留与本机当前月份一致的特别任务；其余包含 `DAILY` 的目标才作为今日任务。以 `goalId`/`badgeId` 是否在 `badges.earned` 判断完成状态，以 `goals.progress` 的数值或 `progress` 字段读取单项进度，`threshold` 作为目标值，`title.uiString` 作为标题。任何必需顶层结构缺失都识别为上游格式变化，不向界面传递原始响应。

## 风险控制

- 只读请求、15 秒超时、有限重试，不实现课程提交或 XP 修改。
- 401/403 暂停计划检查；429 单独呈现；5xx/连接失败归为网络错误。
- JWT 不记录、不写入普通 Store、不返回前端；Windows Credential Manager 是唯一持久化位置。
- 应用内登录从隔离 WebView 的 Duolingo Cookie 存储读取 `jwt_token`；读取发生在 Rust 异步命令中，令牌不经过 React 状态。
- 发布前用用户本人令牌验证，测试与问题报告中只能保留脱敏响应。

参考：[社区 API 说明](https://github.com/api-evangelist/duolingo/blob/main/graphql/duolingo-graphql.md)、[近期 XP endpoint 调研](https://github.com/csutaria/duolingo-dash/blob/main/docs/api-map.md)。
