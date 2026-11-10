# Milestone 4：Effect-Typed Tools 与审批记录

Milestone 4 的目标是记录工具意图和审批状态，而不是执行真实工具。Kernel 通过 Operation Ledger 追加 tool request / approval / denial / result / failure 事件，让后续 Milestone 7 的 worker runtime 可以复用同一条服务边界。

## 范围

包含：
- `ToolEffect` 分类：`read_only`、`local_write`、`network`、`shell`
- `ToolApprovalPolicy`：`read_only` 自动批准，其余 effect 需要审批
- `RunService` tool 方法负责 run 状态校验、ledger append、projection activity 更新
- HTTP API 暴露 tool request、approve、deny、result、fail endpoints
- Workbench 只读展示 tool activity

不包含：
- 不执行真实工具
- 不接 MCP / OpenAPI / CLI / local script
- 不做 OS 权限弹窗
- 不做 Tauri 打包
- 不做云同步

## Event Flow

`RunStarted → ToolCallRequested → ToolCallApproved/ToolCallDenied → ToolCallCompleted/ToolCallFailed`

`ToolCallRequested` 总是记录工具名、effect 和 input JSON。`read_only` 请求会在同一服务调用中追加 `ToolCallApproved { approved_by: "mneme-policy" }`。有副作用的 effect 只记录 request，后续由 approve/deny endpoint 追加审批结果。

## Approval Policy

- `read_only` → `auto_approved`
- `local_write` → `requires_approval`
- `network` → `requires_approval`
- `shell` → `requires_approval`

## Service Boundary

API 和 future worker runtime 必须调用 `RunService`，不能直接写 SQLite。`RunService` 会拒绝 unknown run、blank tool text、以及 completed/failed run 的 tool 事件。

## Verification

```bash
. "$HOME/.cargo/env"
pnpm check
git diff --check
```
