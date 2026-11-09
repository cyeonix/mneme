# Mneme

Mneme 是一个 Personal AI Kernel（个人 AI 内核）：面向本地优先、可审计、可扩展的 Agent 工作台，用来管理 runs、context、tools、memory、artifacts 和 operation history。

## 当前阶段

Phase 1 构建基础切片：

- Rust Kernel crate
- 内存版追加式 Operation Ledger
- 本地 Axum API
- daemon binary
- TypeScript API client
- 最小 React Workbench

## 环境要求

- Rust stable via rustup
- Node.js 24+
- pnpm 11+

## 验证

```bash
pnpm check
```

## 启动本地 daemon

```bash
cargo run -p mneme-daemon
```

健康检查：

```bash
curl http://127.0.0.1:3766/api/v1/health
```

## 启动基础阶段 Web shell

```bash
pnpm --filter @mneme/desktop dev
```
