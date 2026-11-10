# Phase 1 Foundation Architecture

Phase 1 只验证 Mneme 的第一条纵向切片，不包含持久化存储和真实模型调用。

## 边界

- `mneme-kernel` 拥有领域类型和 Operation Ledger 接口。
- `mneme-api` 暴露 HTTP DTO 与 routes。
- `mneme-daemon` 负责进程启动和本地监听地址。
- `apps/desktop` 通过 typed API client 消费 Kernel API，不直接写 Kernel 状态。

## 数据流

```text
React Workbench
  → MnemeApiClient
  → HTTP /api/v1/*
  → mneme-api routes
  → mneme-kernel ledger
```

## 存储演进

Phase 1 初始切片使用 in-memory ledger，以便先确认事件 schema 与 API 边界。Milestone 1/2 之后，daemon 默认使用 SQLite ledger、runs 投影和 Kernel service data plane；内存实现仅作为早期上下文和测试替身。

## Phase 1 之后的数据面演进

Milestone 1 在不改变外部 API 基本形状的前提下，把内存 ledger 替换为 SQLite ledger，并加入 runs 投影。Phase 1 的 API route、daemon、TypeScript client 和 Workbench 会继续存在，只是数据来源从内存对象变成可持久化 Kernel 数据面。

Milestone 2 进一步把创建 run 的业务流程收敛进 Kernel service，避免 HTTP route 直接组合多个存储写入。

## 验证

```bash
pnpm check
```
