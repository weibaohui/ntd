# 094-WS广播预序列化与workspace过滤-实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (zhanlu) | 2026-08-10 | 初始版本 |
| AI (zhanlu) | 2026-08-10 | 按 CodeRabbit 评审：workspace_id() 升级为 EventScope 三态（未归属不再推给带参连接）、前端 onclose 竞态修复、补 vitest/handler 测试、变体数口径 14 |

> 对应设计：`docs/design/094-WS广播预序列化与workspace过滤-设计.md`。
> 093 专项性能类第 1 项：WS 广播逐客户端重复序列化 + 无 workspace 过滤。

## 1. 实现了什么

| 变化 | 文件 | 说明 |
|------|------|------|
| ExecEvent 查询方法 | `executor_service/events.rs` | `event_scope()`（14 变体全枚举，Global/Workspace/Unscoped 三态）+ `is_feishu_direct()` |
| WS 广播通路（新模块） | `handlers/ws_broadcast.rs` | `WsEnvelope`（Arc<str> 预序列化）、`envelope_matches` 三态判定、`spawn_ws_forwarder` 桥接任务 |
| AppState 接线 | `handlers/mod.rs` | `ws_tx` 字段 + forwarder 启动（容量复用 broadcast_channel_capacity 配置） |
| events_handler 改造 | `handlers/mod.rs` | `?workspace_id=` query 解析；Sync 握手按 record.workspace_id 过滤（无 record 保守保留）；主循环订阅 ws_tx 按匹配转发 |
| 前端订阅范围跟随 | `hooks/useExecutionEvents.ts` | 模块级 `sharedWorkspaceId` 记忆；连接 URL 带参；workspace 切换 teardown+重建（onclose 自动重连沿用同范围） |

## 2. 与设计的对应关系

| 设计项 | 落地 | 状态 |
|--------|------|------|
| C1 ExecEvent 查询方法 | 全枚举 match（新增变体漏处理编译期 non-exhaustive 报错） | ✅ |
| C2 WsEnvelope + forwarder | 飞书专用事件拦截、序列化失败跳事件不 panic、Lagged warn 继续 | ✅ |
| C3 handler 过滤 | 三态真值表（无参全推/全局事件全推/归属相等才推）；Sync 过滤后 record_ids 联动收敛（尾部日志/计数查询不为其他 workspace 白跑） | ✅ |
| C4 前端重连 | 首连同参、切换重建、卸载复位 | ✅ |
| 决策 1a/2a/3a | 面板按 workspace 隔离 / 异常任务保守保留 / 全局事件全推 | ✅ |

## 3. 测试与验证结果

- **后端单测**（ws_broadcast 模块 4 个）：
  - `envelope_matches` 三态真值表全枚举
  - `workspace_id()` 13 变体矩阵（Option 透传/i64 包 Some/全局组 None）
  - `is_feishu_direct` 判定
  - forwarder 端到端：DirectCard 拦截 + 正常事件携带正确 workspace_id 与预序列化 JSON
- **全量 lib 测试**：1643 通过 / 1 失败（git_sync 本机旧 git 环境问题，main 基线相同）
- **clippy**：改动文件零告警（全树存量告警为 rustc 1.95.0 新 lint，093 已记录不属本批）
- **前端 `npx tsc --noEmit`**：零错误
- **Playwright**（`frontend/tests/094-ws-workspace-filter.spec.ts`）✅：
  连接序列实测 `无参首连 → ?workspace_id=1（初始化落定重连）→ ?workspace_id=2（切换重连）`，
  console 零错误
- **dev 实例日志**：forwarder 启动正常，无序列化 error

## 4. 性能语义变化

| 维度 | 改前 | 改后 |
|------|------|------|
| 序列化次数 | 每事件 × N 客户端 | 每事件 × 1（forwarder 全局唯一） |
| 事件推送范围 | 全 workspace → 全客户端 | 匹配 workspace + 全局事件 → 声明该 workspace 的客户端 |
| 飞书专用事件 | 推给 WS 客户端（前端无消费） | 拦截在 WS channel 之外 |
| Sync 握手 | 全量任务 | 声明 workspace 的任务（无 record 异常任务保留） |
| 每客户端发送成本 | 序列化 + String 分配 | Arc<str> 共享 + 一次 memcpy |

## 5. 已知限制

- 发送侧仍有一次 memcpy（`Message::text(&*json)`）；彻底零拷贝（Bytes 直进 channel）留待实测有 CPU 压力再做；
- 无 workspace 参数的连接全推（兼容口，本服务单用户本地工具定位，不构成新暴露面）；
- 首个连接在初始 workspace 异步落定前为无参全推（一次性，目录加载完成后自动重连带参）。

## 6. 安全反思

- **收益**：消除跨 workspace 任务标题/日志流的面板可见性（现状轻度租户泄露）；
- forwarder 对序列化失败跳事件不 panic（WS 推送停摆风险归零）；
- 无 SQL/权限/文件路径新增面；事件过滤为纯内存判定。
