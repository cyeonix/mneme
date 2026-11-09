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

## 当前存储

本阶段 Ledger 是内存实现。下一阶段再加入 SQLite 持久化，这样可以先确认事件 schema 与 API 边界足够稳定。

## 验证

```bash
pnpm check
```
