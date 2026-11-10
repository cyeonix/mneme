# Milestone 1：持久化 Operation Ledger 与 runs 投影

## 边界

Milestone 1 把 Phase 1 的内存账本升级为 SQLite 本地数据面，让 Mneme 这个 Personal AI Kernel 拥有可审计、可重放的本地事实源。它只覆盖 Operation Ledger 的持久化、runs 读模型投影、API/Workbench 对该数据面的读写路径，以及 daemon 使用本地数据库启动的运行方式。

Milestone 1 不执行模型、不调用工具、不实现人工审批，也不引入 Tauri、自托管服务器或完整 agent run loop。这些能力会在后续里程碑基于同一 ledger 数据面继续演进。

## 数据流

```text
React Workbench
  → TypeScript API Client
  → HTTP /api/v1/*
  → mneme-api
  → mneme-kernel
  → SQLite operation_events / runs
```

Kernel 内部的数据流：

1. `OperationEvent` 追加写入 `operation_events`。
2. `RunProjectionStore` 根据同一事件更新 `runs`。
3. `RunProjector::replay()` 可以从事件流重建 `runs`，用于迁移、修复和测试。

Daemon 启动时通过 `MNEME_DB_URL` 选择 SQLite 数据库；未设置时使用本地默认库。API route、TypeScript client 与 Workbench 不直接关心数据库位置，只通过 `/api/v1/*` 与 Kernel 数据面交互。

## 表职责

- `operation_events`：事实源，append-only。所有运行生命周期变化先以事件形式持久化，后续投影、调试和审计都以它为准。
- `runs`：读模型，可删除后由 replay 重建。它服务列表页、Workbench 展示和常用查询，不作为不可替代事实源。

这种分工让写入路径保持可追溯，同时允许读模型随 UI 和查询需求调整；只要 `operation_events` 保留完整，`runs` 就可以通过 `RunProjector::replay()` 确定性恢复。

## Milestone 2 之后的服务边界

Milestone 1 建立了 SQLite 数据面；Milestone 2 在其上新增 `RunService`，让 API/daemon/未来 typed RPC 共享同一业务入口。`operation_events` 与 `runs` 的职责不变，但 run 创建不再由 API route 直接编排。

## 验证

常用验证：

```bash
pnpm check
git diff --check
```

最终收口验证：

```bash
. "$HOME/.cargo/env" && pnpm check && git diff --check && git status --short --branch
```

文档层面的自查重点：

- 是否明确 Milestone 1 的边界，避免把模型执行、工具审批和完整 agent loop 纳入本阶段。
- 是否说明 `operation_events` 与 `runs` 的职责差异。
- 是否说明 projection replay 可用于迁移、修复和测试。
- 是否保留 daemon 使用 `MNEME_DB_URL` 切换本地 SQLite 数据库的运行方式。
