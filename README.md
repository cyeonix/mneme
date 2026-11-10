# Mneme

Mneme 是一个 Personal AI Kernel（个人 AI 内核）：面向本地优先、可审计、可扩展的 Agent 工作台，用来管理 runs、context、tools、memory、artifacts 和 operation history。

## 当前阶段

Phase 1 构建基础切片：

- Rust Kernel crate
- SQLite 持久化追加式 Operation Ledger
- 本地 Axum API
- daemon binary
- TypeScript API client
- 最小 React Workbench

## 环境要求

- Rust stable via rustup
- Node.js 24+
- pnpm 11+

## 常用验证

```bash
pnpm check
git diff --check
```

## 启动本地 daemon

```bash
cargo run -p mneme-daemon
```

### 本地 SQLite 数据库

默认 daemon 使用 `sqlite://.mneme/mneme.db?mode=rwc`。如需隔离测试数据：

```bash
MNEME_DB_URL='sqlite://.mneme-dev/mneme.db?mode=rwc' cargo run -p mneme-daemon
```

Operation Ledger 的 `operation_events` 表是事实源；`runs` 表是可重建投影。

## Kernel service boundary

HTTP API 不直接拼接 ledger 与 projection 写入流程。创建 run 会进入 Kernel `RunService`；daemon 默认使用 SQLite service，在同一事务内写入 `operation_events` 与 `runs`。

如需在启动时从 `operation_events` 修复 `runs` 读模型，可以显式开启：

```bash
MNEME_REBUILD_RUNS_ON_START=1 cargo run -p mneme-daemon
```

默认不自动 rebuild，避免每次启动都重写读模型；当迁移、调试或怀疑 projection 失配时再开启。

健康检查：

```bash
curl http://127.0.0.1:3766/api/v1/health
```

## 创建一条 run

```bash
curl -s -X POST http://127.0.0.1:3766/api/v1/runs \
  -H 'content-type: application/json' \
  -d '{"instruction":"整理今天的工作"}'

curl -s http://127.0.0.1:3766/api/v1/runs
```

### 手动推进 run lifecycle

Milestone 3 提供本地手动验证路径，用来确认 lifecycle event append、projection 和 Workbench 展示；这不代表已接入真实模型调用。

```bash
RUN_ID=$(curl -s http://127.0.0.1:3766/api/v1/runs \
  -H 'content-type: application/json' \
  -d '{"instruction":"整理今天的工作"}' | jq -r '.run_id')

curl -s "http://127.0.0.1:3766/api/v1/runs/$RUN_ID/model-requests" \
  -H 'content-type: application/json' \
  -d '{"text":"请总结今天的工作"}'

curl -s "http://127.0.0.1:3766/api/v1/runs/$RUN_ID/assistant-messages" \
  -H 'content-type: application/json' \
  -d '{"text":"今天完成了 Kernel service boundary。"}'

curl -s "http://127.0.0.1:3766/api/v1/runs/$RUN_ID/complete" \
  -H 'content-type: application/json' \
  -d '{"text":"总结完成"}'

curl -s http://127.0.0.1:3766/api/v1/runs
```

## 启动基础阶段 Web shell

```bash
pnpm --filter @mneme/desktop dev
```
