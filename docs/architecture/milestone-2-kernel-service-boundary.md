# Milestone 2：Kernel RunService 边界

## 边界

Milestone 2 把 run 写入语义从 HTTP route 和 daemon wiring 中收敛到 Kernel service。外部 adapter 只负责协议转换：HTTP API 把 JSON request 转成 service call，daemon 初始化 SQLite store 并注入 service，未来 typed RPC/event bus 也应调用同一 service。

Milestone 2 不实现模型执行、工具审批、Tauri、自托管服务器或完整 agent run loop。

## 写入路径

```text
POST /api/v1/runs
  → mneme-api route
  → SharedRunService::create_run
  → OperationEvent::RunStarted
  → operation_events append
  → runs projection upsert
```

在 daemon 默认路径中，`SqliteRunService` 使用同一个 SQL transaction 写入 `operation_events` 与 `runs`，避免 append 成功但 projection 更新失败导致 HTTP 创建 run 后立即不可见。

## Repair 路径

`RunService::rebuild_run_projections()` 从 `operation_events` 读取事实源，并用 `RunProjector::replay()` 重建 `runs`。daemon 默认不自动 rebuild；需要修复时显式设置：

```bash
MNEME_REBUILD_RUNS_ON_START=1 cargo run -p mneme-daemon
```

## Adapter 规则

- API route 不直接调用 ledger/store 的写入组合。
- daemon 不直接拼装 ledger + projection 写入流程。
- SQLite 事务细节留在 Kernel storage/service 内部。
- Workbench 与 TypeScript client 继续只通过 HTTP API 访问。

## 验证

```bash
. "$HOME/.cargo/env" && pnpm check
git diff --check
```
