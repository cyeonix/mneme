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

## 启动基础阶段 Web shell

```bash
pnpm --filter @mneme/desktop dev
```
