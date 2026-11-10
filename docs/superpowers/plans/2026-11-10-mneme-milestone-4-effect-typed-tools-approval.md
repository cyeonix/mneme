# Mneme Milestone 4 Effect-Typed Tools and Approval Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不执行真实工具的前提下，为 Mneme Kernel 增加 effect-typed tool request / approval / result 事件链路，并通过服务边界、HTTP API、TypeScript client 与 Workbench 展示形成可验证闭环。

**Architecture:** Milestone 4 继续坚持 Operation Ledger 是事实源：所有 tool request、approval、denial、result、failure 都追加为 `OperationEvent`。`RunService` 仍是唯一写入业务边界，负责校验 run 是否存在且仍为 running，并把 tool 事件写入 ledger，再让 runs projection 仅更新 `updated_at`。本里程碑只记录工具意图、审批和结果，不做真实工具执行、MCP、CLI、沙箱或权限弹窗。

**Tech Stack:** Rust `mneme-kernel` / Axum `mneme-api` / SQLite SQLx data plane / TypeScript API client / React Workbench / pnpm check。

## Global Constraints

- Mneme 是 Personal AI Kernel（个人 AI 内核），不是通用云端 Agent 平台。
- 当前工作区是 `/Users/baiye/code/ai/mneme/.worktrees/phase-1-foundation`，分支是 `phase-1-foundation`。
- 必须继续使用 Rust-first Kernel + TypeScript UI；Kernel 是 authoritative control plane。
- Operation Ledger 是 append-only fact source；SQLite `operation_events` 保存完整事实，`runs` 是可重建读模型。
- 所有新增写入必须通过 `RunService` 或它明确拥有的服务边界；API、daemon、UI 不直接组合 ledger + projection 写入。
- HTTP API 必须保持 additive：不能破坏既有 `/api/v1/health`、`/api/v1/events`、`/api/v1/runs` 和 lifecycle endpoints。
- Milestone 4 只做 effect-typed tool event + approval record MVP；不执行真实工具、不接 MCP/OpenAPI/CLI、不做 Tauri、不做云同步。
- 终态 run（completed/failed）不能再被 tool 事件恢复为 running；service 层必须拒绝 post-terminal tool 事件。
- 中文优先：规划、文档、注释尽量中文；代码公共 API 名称保持英文、类型清晰。
- 每个任务必须先写失败测试，再实现最小代码，再运行验证并提交。

---

## Current Baseline

- `OperationEventKind` 目前包含 `RunStarted`、`RunModelRequested`、`RunAssistantMessage`、`RunCompleted`、`RunFailed`。
- `RunService` 目前包含 `create_run`、`request_model`、`record_assistant_message`、`complete_run`、`fail_run`、`list_events`、`list_runs`、`rebuild_run_projections`。
- `ComposedRunService::append_and_apply_lifecycle_event()` 会先检查 run 存在并且 `RunStatus::Running`，否则返回 `InvalidRunId` 或 `RunAlreadyTerminal`。
- `SqliteRunService` 在 SQLite transaction 内检查 `runs.status`，再插入 lifecycle event 并更新 `runs`。
- `RunProjector`、`InMemoryRunProjectionStore` 和 SQLite projection 对中间 lifecycle events 只更新 running run 的 `updated_at`。
- API 已有 lifecycle endpoints，并把 `RunAlreadyTerminal` 映射为 `400 BAD_REQUEST`。
- Workbench 现在展示 runs、summary 和 error，但尚未展示 tool activity。

## File Structure

- `crates/mneme-kernel/src/ids.rs`：新增 `ToolCallId` newtype，和 `EventId`/`RunId` 一样负责强类型 ID。
- `crates/mneme-kernel/src/tools.rs`：新增 tool domain types：`ToolEffect`、`ToolApprovalDecision`、`ToolApprovalPolicy`、`ToolCallStatus`、`ToolCallProjection`、`ToolProjector`。
- `crates/mneme-kernel/src/events.rs`：新增 tool 事件变体与构造函数。
- `crates/mneme-kernel/src/projections.rs`、`crates/mneme-kernel/src/runs.rs`、`crates/mneme-kernel/src/storage/sqlite.rs`：让 tool events 作为 running run 的 activity 更新 `updated_at`，并在 SQLite ledger roundtrip 中支持新 payload。
- `crates/mneme-kernel/src/service.rs`：扩展 `RunService` 的 tool 方法和错误类型；Composed service 负责输入校验、policy 判断、append/apply。
- `crates/mneme-api/src/types.rs`、`crates/mneme-api/src/routes.rs`：新增 tool HTTP DTO 和 endpoints。
- `apps/desktop/src/api/client.ts`、`apps/desktop/src/api/client.test.ts`：新增 TypeScript tool API client 类型和方法。
- `apps/desktop/src/app/App.tsx`、`apps/desktop/src/app/App.test.tsx`、`apps/desktop/src/styles.css`：展示 tool activity，不提供复杂交互。
- `README.md`、`docs/architecture/milestone-4-effect-typed-tools-approval.md`、相关架构文档：记录边界、事件流、审批策略和验证命令。

---

### Task 1: 增加 ToolCallId、effect 类型、approval policy 与 tool 事件模型

**Files:**
- Modify: `crates/mneme-kernel/src/ids.rs`
- Create: `crates/mneme-kernel/src/tools.rs`
- Modify: `crates/mneme-kernel/src/events.rs`
- Modify: `crates/mneme-kernel/src/projections.rs`
- Modify: `crates/mneme-kernel/src/runs.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: `RunId`, `OperationEvent`, `OperationEventKind`, `RunProjector::replay(events: &[OperationEvent]) -> Vec<RunProjection>`。
- Produces:
  - `ToolCallId::new() -> ToolCallId`
  - `ToolEffect::{ReadOnly, LocalWrite, Network, Shell}` serialized as `read_only`, `local_write`, `network`, `shell`
  - `ToolApprovalDecision::{AutoApproved, RequiresApproval}` serialized as `auto_approved`, `requires_approval`
  - `ToolApprovalPolicy::decision_for(effect: &ToolEffect) -> ToolApprovalDecision`
  - `OperationEventKind::{ToolCallRequested, ToolCallApproved, ToolCallDenied, ToolCallCompleted, ToolCallFailed}`
  - Constructors on `OperationEvent` for all five tool variants.
  - Tool events update run `updated_at` only while the run is `running`.

- [ ] **Step 1: 写失败测试：ToolCallId 与 effect policy**

In `crates/mneme-kernel/src/ids.rs` tests add:

```rust
#[test]
fn generated_tool_call_ids_are_displayable() {
    let tool_call_id = ToolCallId::new().to_string();

    assert_eq!(tool_call_id.len(), 36);
    assert!(tool_call_id.contains('-'));
}
```

Create `crates/mneme-kernel/src/tools.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_policy_auto_approves_read_only_tools() {
        assert_eq!(
            ToolApprovalPolicy::default().decision_for(&ToolEffect::ReadOnly),
            ToolApprovalDecision::AutoApproved
        );
    }

    #[test]
    fn approval_policy_requires_approval_for_side_effects() {
        let policy = ToolApprovalPolicy::default();

        assert_eq!(
            policy.decision_for(&ToolEffect::LocalWrite),
            ToolApprovalDecision::RequiresApproval
        );
        assert_eq!(
            policy.decision_for(&ToolEffect::Network),
            ToolApprovalDecision::RequiresApproval
        );
        assert_eq!(
            policy.decision_for(&ToolEffect::Shell),
            ToolApprovalDecision::RequiresApproval
        );
    }
}
```

- [ ] **Step 2: 运行失败测试**

Run:

```bash
. "$HOME/.cargo/env"
cargo test -p mneme-kernel generated_tool_call_ids_are_displayable
cargo test -p mneme-kernel approval_policy_auto_approves_read_only_tools
```

Expected: FAIL，包含 `cannot find type ToolCallId` 或 `cannot find type ToolEffect`。

- [ ] **Step 3: 实现 ToolCallId**

In `crates/mneme-kernel/src/ids.rs` add:

```rust
/// 一次工具调用请求的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolCallId(Uuid);

impl ToolCallId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ToolCallId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ToolCallId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ToolCallId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}
```

Update `crates/mneme-kernel/src/lib.rs`:

```rust
pub use ids::{EventId, RunId, ToolCallId};
```

- [ ] **Step 4: 实现 tools domain types**

In `crates/mneme-kernel/src/tools.rs`:

```rust
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{OperationEvent, OperationEventKind, RunId, ToolCallId};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffect {
    ReadOnly,
    LocalWrite,
    Network,
    Shell,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolApprovalDecision {
    AutoApproved,
    RequiresApproval,
}

#[derive(Clone, Debug, Default)]
pub struct ToolApprovalPolicy;

impl ToolApprovalPolicy {
    pub fn decision_for(&self, effect: &ToolEffect) -> ToolApprovalDecision {
        match effect {
            ToolEffect::ReadOnly => ToolApprovalDecision::AutoApproved,
            ToolEffect::LocalWrite | ToolEffect::Network | ToolEffect::Shell => {
                ToolApprovalDecision::RequiresApproval
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Requested,
    Approved,
    Denied,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolCallProjection {
    pub run_id: RunId,
    pub tool_call_id: ToolCallId,
    pub tool_name: String,
    pub effect: ToolEffect,
    pub status: ToolCallStatus,
    pub requested_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub approved_by: Option<String>,
    pub denial_reason: Option<String>,
    pub failure_reason: Option<String>,
}

pub struct ToolProjector;

impl ToolProjector {
    pub fn replay(events: &[OperationEvent]) -> Vec<ToolCallProjection> {
        let mut tools = Vec::<ToolCallProjection>::new();

        for event in events {
            match &event.kind {
                OperationEventKind::ToolCallRequested {
                    tool_call_id,
                    tool_name,
                    effect,
                    ..
                } => {
                    if let Some(tool) = tools.iter_mut().find(|tool| tool.tool_call_id == *tool_call_id) {
                        tool.status = ToolCallStatus::Requested;
                        tool.tool_name = tool_name.clone();
                        tool.effect = effect.clone();
                        tool.requested_at = event.occurred_at;
                        tool.updated_at = event.occurred_at;
                        tool.approved_by = None;
                        tool.denial_reason = None;
                        tool.failure_reason = None;
                    } else {
                        tools.push(ToolCallProjection {
                            run_id: event.run_id,
                            tool_call_id: *tool_call_id,
                            tool_name: tool_name.clone(),
                            effect: effect.clone(),
                            status: ToolCallStatus::Requested,
                            requested_at: event.occurred_at,
                            updated_at: event.occurred_at,
                            approved_by: None,
                            denial_reason: None,
                            failure_reason: None,
                        });
                    }
                }
                OperationEventKind::ToolCallApproved { tool_call_id, approved_by } => {
                    if let Some(tool) = tools.iter_mut().find(|tool| tool.tool_call_id == *tool_call_id) {
                        tool.status = ToolCallStatus::Approved;
                        tool.approved_by = Some(approved_by.clone());
                        tool.updated_at = event.occurred_at;
                    }
                }
                OperationEventKind::ToolCallDenied { tool_call_id, reason } => {
                    if let Some(tool) = tools.iter_mut().find(|tool| tool.tool_call_id == *tool_call_id) {
                        tool.status = ToolCallStatus::Denied;
                        tool.denial_reason = Some(reason.clone());
                        tool.updated_at = event.occurred_at;
                    }
                }
                OperationEventKind::ToolCallCompleted { tool_call_id, .. } => {
                    if let Some(tool) = tools.iter_mut().find(|tool| tool.tool_call_id == *tool_call_id) {
                        tool.status = ToolCallStatus::Completed;
                        tool.updated_at = event.occurred_at;
                    }
                }
                OperationEventKind::ToolCallFailed { tool_call_id, reason } => {
                    if let Some(tool) = tools.iter_mut().find(|tool| tool.tool_call_id == *tool_call_id) {
                        tool.status = ToolCallStatus::Failed;
                        tool.failure_reason = Some(reason.clone());
                        tool.updated_at = event.occurred_at;
                    }
                }
                _ => {}
            }
        }

        tools.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        tools
    }
}
```

Update `crates/mneme-kernel/src/lib.rs`:

```rust
pub mod tools;
pub use tools::{
    ToolApprovalDecision, ToolApprovalPolicy, ToolCallProjection, ToolCallStatus, ToolEffect,
    ToolProjector,
};
```

- [ ] **Step 5: 写失败测试：OperationEvent tool variants**

In `crates/mneme-kernel/src/events.rs` tests add:

```rust
#[test]
fn tool_call_requested_event_captures_effect_and_input() {
    let run_id = RunId::new();
    let tool_call_id = ToolCallId::new();
    let input_json = serde_json::json!({ "path": "README.md" });
    let event = OperationEvent::new_tool_call_requested(
        run_id,
        tool_call_id,
        "read_file",
        ToolEffect::ReadOnly,
        input_json.clone(),
    );

    assert_eq!(event.run_id, run_id);
    assert!(matches!(
        event.kind,
        OperationEventKind::ToolCallRequested {
            tool_call_id: actual_tool_call_id,
            ref tool_name,
            ref effect,
            ref input_json: actual_input_json,
        } if actual_tool_call_id == tool_call_id
            && tool_name == "read_file"
            && effect == &ToolEffect::ReadOnly
            && actual_input_json == &input_json
    ));
}

#[test]
fn tool_call_result_events_capture_tool_call_id() {
    let run_id = RunId::new();
    let tool_call_id = ToolCallId::new();

    let approved = OperationEvent::new_tool_call_approved(run_id, tool_call_id, "mneme-policy");
    let denied = OperationEvent::new_tool_call_denied(run_id, tool_call_id, "需要人工确认");
    let completed = OperationEvent::new_tool_call_completed(
        run_id,
        tool_call_id,
        serde_json::json!({ "ok": true }),
    );
    let failed = OperationEvent::new_tool_call_failed(run_id, tool_call_id, "工具执行失败");

    assert!(matches!(approved.kind, OperationEventKind::ToolCallApproved { tool_call_id: id, .. } if id == tool_call_id));
    assert!(matches!(denied.kind, OperationEventKind::ToolCallDenied { tool_call_id: id, .. } if id == tool_call_id));
    assert!(matches!(completed.kind, OperationEventKind::ToolCallCompleted { tool_call_id: id, .. } if id == tool_call_id));
    assert!(matches!(failed.kind, OperationEventKind::ToolCallFailed { tool_call_id: id, .. } if id == tool_call_id));
}
```

- [ ] **Step 6: 运行失败测试**

Run:

```bash
. "$HOME/.cargo/env"
cargo test -p mneme-kernel tool_call_requested_event_captures_effect_and_input
cargo test -p mneme-kernel tool_call_result_events_capture_tool_call_id
```

Expected: FAIL，包含 `no variant named 'ToolCallRequested'` 或 `no associated function named 'new_tool_call_requested'`。

- [ ] **Step 7: 实现 OperationEvent tool variants**

In `crates/mneme-kernel/src/events.rs`, add imports:

```rust
use serde_json::Value;
use crate::{ToolCallId, ToolEffect};
```

Add variants to `OperationEventKind`:

```rust
ToolCallRequested {
    tool_call_id: ToolCallId,
    tool_name: String,
    effect: ToolEffect,
    input_json: Value,
},
ToolCallApproved {
    tool_call_id: ToolCallId,
    approved_by: String,
},
ToolCallDenied {
    tool_call_id: ToolCallId,
    reason: String,
},
ToolCallCompleted {
    tool_call_id: ToolCallId,
    output_json: Value,
},
ToolCallFailed {
    tool_call_id: ToolCallId,
    reason: String,
},
```

Add constructors:

```rust
pub fn new_tool_call_requested(
    run_id: RunId,
    tool_call_id: ToolCallId,
    tool_name: impl Into<String>,
    effect: ToolEffect,
    input_json: Value,
) -> Self {
    Self {
        id: EventId::new(),
        run_id,
        occurred_at: OffsetDateTime::now_utc(),
        kind: OperationEventKind::ToolCallRequested {
            tool_call_id,
            tool_name: tool_name.into(),
            effect,
            input_json,
        },
    }
}

pub fn new_tool_call_approved(
    run_id: RunId,
    tool_call_id: ToolCallId,
    approved_by: impl Into<String>,
) -> Self {
    Self {
        id: EventId::new(),
        run_id,
        occurred_at: OffsetDateTime::now_utc(),
        kind: OperationEventKind::ToolCallApproved {
            tool_call_id,
            approved_by: approved_by.into(),
        },
    }
}

pub fn new_tool_call_denied(
    run_id: RunId,
    tool_call_id: ToolCallId,
    reason: impl Into<String>,
) -> Self {
    Self {
        id: EventId::new(),
        run_id,
        occurred_at: OffsetDateTime::now_utc(),
        kind: OperationEventKind::ToolCallDenied {
            tool_call_id,
            reason: reason.into(),
        },
    }
}

pub fn new_tool_call_completed(
    run_id: RunId,
    tool_call_id: ToolCallId,
    output_json: Value,
) -> Self {
    Self {
        id: EventId::new(),
        run_id,
        occurred_at: OffsetDateTime::now_utc(),
        kind: OperationEventKind::ToolCallCompleted {
            tool_call_id,
            output_json,
        },
    }
}

pub fn new_tool_call_failed(
    run_id: RunId,
    tool_call_id: ToolCallId,
    reason: impl Into<String>,
) -> Self {
    Self {
        id: EventId::new(),
        run_id,
        occurred_at: OffsetDateTime::now_utc(),
        kind: OperationEventKind::ToolCallFailed {
            tool_call_id,
            reason: reason.into(),
        },
    }
}
```

- [ ] **Step 8: 更新 projection activity handling**

In `RunProjector::replay()` and `InMemoryRunProjectionStore::apply()`, treat all tool events like model/assistant intermediate events:

```rust
OperationEventKind::RunModelRequested { .. }
| OperationEventKind::RunAssistantMessage { .. }
| OperationEventKind::ToolCallRequested { .. }
| OperationEventKind::ToolCallApproved { .. }
| OperationEventKind::ToolCallDenied { .. }
| OperationEventKind::ToolCallCompleted { .. }
| OperationEventKind::ToolCallFailed { .. } => {
    if let Some(run) = runs
        .iter_mut()
        .find(|run| run.run_id == event.run_id && run.status == RunStatus::Running)
    {
        run.updated_at = event.occurred_at;
    }
}
```

In `SqliteRunProjectionStore::apply()` in `crates/mneme-kernel/src/storage/sqlite.rs`, add the same tool variants to the branch that runs:

```rust
"update runs set updated_at = ?2 where run_id = ?1 and status = 'running'"
```

Update `kind_type_for_event()` in `crates/mneme-kernel/src/storage/sqlite.rs`:

```rust
OperationEventKind::ToolCallRequested { .. } => "tool_call_requested",
OperationEventKind::ToolCallApproved { .. } => "tool_call_approved",
OperationEventKind::ToolCallDenied { .. } => "tool_call_denied",
OperationEventKind::ToolCallCompleted { .. } => "tool_call_completed",
OperationEventKind::ToolCallFailed { .. } => "tool_call_failed",
```

- [ ] **Step 9: 写 SQLite roundtrip 和 projector 测试**

In `crates/mneme-kernel/src/tools.rs` tests add:

```rust
#[test]
fn tool_projector_replays_request_approval_and_completion() {
    let run_id = RunId::new();
    let tool_call_id = ToolCallId::new();
    let events = vec![
        OperationEvent::new_tool_call_requested(
            run_id,
            tool_call_id,
            "read_file",
            ToolEffect::ReadOnly,
            serde_json::json!({ "path": "README.md" }),
        ),
        OperationEvent::new_tool_call_approved(run_id, tool_call_id, "mneme-policy"),
        OperationEvent::new_tool_call_completed(
            run_id,
            tool_call_id,
            serde_json::json!({ "content": "hello" }),
        ),
    ];

    let tools = ToolProjector::replay(&events);

    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].tool_call_id, tool_call_id);
    assert_eq!(tools[0].status, ToolCallStatus::Completed);
    assert_eq!(tools[0].approved_by.as_deref(), Some("mneme-policy"));
}
```

In `crates/mneme-kernel/src/storage/sqlite.rs` tests add a test named `sqlite_ledger_roundtrips_tool_events` that appends `ToolCallRequested`, `ToolCallApproved`, and `ToolCallCompleted`, lists the ledger, and asserts the exact `tool_call_id`, `tool_name`, `effect`, and output JSON survive roundtrip.

- [ ] **Step 10: 运行验证**

Run:

```bash
. "$HOME/.cargo/env"
cargo fmt --all --check
cargo test -p mneme-kernel tool_call
cargo test -p mneme-kernel tool_projector
cargo test -p mneme-kernel sqlite_ledger_roundtrips_tool_events
cargo test -p mneme-kernel replay_
git diff --check
```

Expected: all PASS。

- [ ] **Step 11: Commit**

```bash
git add crates/mneme-kernel/src/ids.rs crates/mneme-kernel/src/tools.rs crates/mneme-kernel/src/events.rs crates/mneme-kernel/src/projections.rs crates/mneme-kernel/src/runs.rs crates/mneme-kernel/src/storage/sqlite.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add effect typed tool events"
```

---

### Task 2: 扩展 RunService tool request / approval / result 方法

**Files:**
- Modify: `crates/mneme-kernel/src/service.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 `ToolCallId`、`ToolEffect`、`ToolApprovalDecision`、tool event constructors。
- Produces:
  - `ToolCallOutcome { pub run_id: RunId, pub tool_call_id: ToolCallId, pub approval: ToolApprovalDecision }`
  - `ToolLifecycleOutcome { pub run_id: RunId, pub tool_call_id: ToolCallId }`
  - `RunServiceError::InvalidToolInput`
  - `RunService::request_tool_call(&self, run_id: RunId, tool_name: String, effect: ToolEffect, input_json: serde_json::Value) -> Result<ToolCallOutcome, RunServiceError>`
  - `RunService::approve_tool_call(&self, run_id: RunId, tool_call_id: ToolCallId, approved_by: String) -> Result<ToolLifecycleOutcome, RunServiceError>`
  - `RunService::deny_tool_call(&self, run_id: RunId, tool_call_id: ToolCallId, reason: String) -> Result<ToolLifecycleOutcome, RunServiceError>`
  - `RunService::record_tool_result(&self, run_id: RunId, tool_call_id: ToolCallId, output_json: serde_json::Value) -> Result<ToolLifecycleOutcome, RunServiceError>`
  - `RunService::record_tool_failure(&self, run_id: RunId, tool_call_id: ToolCallId, reason: String) -> Result<ToolLifecycleOutcome, RunServiceError>`

- [ ] **Step 1: 写失败测试：ComposedRunService tool lifecycle**

In `crates/mneme-kernel/src/service.rs` tests add:

```rust
#[tokio::test]
async fn composed_service_records_read_only_tool_request_as_auto_approved() {
    let ledger = InMemoryOperationLedger::new();
    let run_store = InMemoryRunProjectionStore::new();
    let service = ComposedRunService::new(ledger.clone().shared(), run_store.clone().shared());
    let created = service.create_run("读取文件".to_string()).await.unwrap();

    let outcome = service
        .request_tool_call(
            created.run_id,
            "read_file".to_string(),
            ToolEffect::ReadOnly,
            serde_json::json!({ "path": "README.md" }),
        )
        .await
        .unwrap();

    let events = ledger.list().await.unwrap();
    assert_eq!(outcome.run_id, created.run_id);
    assert_eq!(outcome.approval, ToolApprovalDecision::AutoApproved);
    assert_eq!(events.len(), 3);
    assert!(matches!(events[1].kind, OperationEventKind::ToolCallRequested { .. }));
    assert!(matches!(events[2].kind, OperationEventKind::ToolCallApproved { .. }));
}

#[tokio::test]
async fn composed_service_records_side_effect_tool_request_as_requires_approval() {
    let ledger = InMemoryOperationLedger::new();
    let run_store = InMemoryRunProjectionStore::new();
    let service = ComposedRunService::new(ledger.clone().shared(), run_store.clone().shared());
    let created = service.create_run("写文件".to_string()).await.unwrap();

    let outcome = service
        .request_tool_call(
            created.run_id,
            "write_file".to_string(),
            ToolEffect::LocalWrite,
            serde_json::json!({ "path": "README.md", "content": "hello" }),
        )
        .await
        .unwrap();

    assert_eq!(outcome.approval, ToolApprovalDecision::RequiresApproval);
    assert_eq!(ledger.list().await.unwrap().len(), 2);
}

#[tokio::test]
async fn composed_service_rejects_tool_request_after_run_completed() {
    let service = ComposedRunService::new(
        InMemoryOperationLedger::new().shared(),
        InMemoryRunProjectionStore::new().shared(),
    );
    let created = service.create_run("完成后不能请求工具".to_string()).await.unwrap();
    service.complete_run(created.run_id, "完成".to_string()).await.unwrap();

    let error = service
        .request_tool_call(
            created.run_id,
            "read_file".to_string(),
            ToolEffect::ReadOnly,
            serde_json::json!({ "path": "README.md" }),
        )
        .await
        .unwrap_err();

    assert_eq!(error, RunServiceError::RunAlreadyTerminal);
}
```

- [ ] **Step 2: 运行失败测试**

Run:

```bash
. "$HOME/.cargo/env"
cargo test -p mneme-kernel composed_service_records_read_only_tool_request_as_auto_approved
cargo test -p mneme-kernel composed_service_records_side_effect_tool_request_as_requires_approval
cargo test -p mneme-kernel composed_service_rejects_tool_request_after_run_completed
```

Expected: FAIL，包含 `no method named 'request_tool_call'` 或 `cannot find type ToolCallOutcome`。

- [ ] **Step 3: 实现 RunService trait 与 ComposedRunService**

In `crates/mneme-kernel/src/service.rs` add imports:

```rust
use serde_json::Value;
use crate::{ToolApprovalDecision, ToolApprovalPolicy, ToolCallId, ToolEffect};
```

Add outcome structs:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallOutcome {
    pub run_id: RunId,
    pub tool_call_id: ToolCallId,
    pub approval: ToolApprovalDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolLifecycleOutcome {
    pub run_id: RunId,
    pub tool_call_id: ToolCallId,
}
```

Add error variant:

```rust
#[error("tool input must not be blank")]
InvalidToolInput,
```

Add trait methods exactly as listed in **Interfaces**。

Add helpers in `ComposedRunService`:

```rust
fn non_blank_tool_text(value: String) -> Result<String, RunServiceError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(RunServiceError::InvalidToolInput);
    }
    Ok(value)
}

async fn ensure_running_run(&self, run_id: RunId) -> Result<(), RunServiceError> {
    let runs = self
        .run_store
        .list_runs()
        .await
        .map_err(|error| RunServiceError::Projection(error.to_string()))?;
    let run = runs
        .iter()
        .find(|run| run.run_id == run_id)
        .ok_or(RunServiceError::InvalidRunId)?;
    if run.status != crate::RunStatus::Running {
        return Err(RunServiceError::RunAlreadyTerminal);
    }
    Ok(())
}

async fn append_and_apply_tool_event(
    &self,
    event: OperationEvent,
) -> Result<ToolLifecycleOutcome, RunServiceError> {
    let run_id = event.run_id;
    let tool_call_id = tool_call_id_for_event(&event).ok_or(RunServiceError::InvalidToolInput)?;
    self.ensure_running_run(run_id).await?;
    self.ledger
        .append(event.clone())
        .await
        .map_err(|error| RunServiceError::Ledger(error.to_string()))?;
    self.run_store
        .apply(&event)
        .await
        .map_err(|error| RunServiceError::Projection(error.to_string()))?;
    Ok(ToolLifecycleOutcome { run_id, tool_call_id })
}
```

Add private function near tests or helpers:

```rust
fn tool_call_id_for_event(event: &OperationEvent) -> Option<ToolCallId> {
    match &event.kind {
        OperationEventKind::ToolCallRequested { tool_call_id, .. }
        | OperationEventKind::ToolCallApproved { tool_call_id, .. }
        | OperationEventKind::ToolCallDenied { tool_call_id, .. }
        | OperationEventKind::ToolCallCompleted { tool_call_id, .. }
        | OperationEventKind::ToolCallFailed { tool_call_id, .. } => Some(*tool_call_id),
        _ => None,
    }
}
```

Implement `request_tool_call`: trim `tool_name`, reject blank as `InvalidToolInput`, call `ensure_running_run`, create `ToolCallId::new()`, append `ToolCallRequested`, apply projection, compute `ToolApprovalPolicy::default().decision_for(&effect)`, and for `AutoApproved` also append/apply `ToolCallApproved { approved_by: "mneme-policy" }`。

Implement approval/result methods by trimming text where applicable and calling `append_and_apply_tool_event()` with the matching constructor.

Update `crates/mneme-kernel/src/lib.rs` export:

```rust
pub use service::{
    ComposedRunService, CreateRunOutcome, RunLifecycleOutcome, RunService, RunServiceError,
    SharedRunService, ToolCallOutcome, ToolLifecycleOutcome,
};
```

- [ ] **Step 4: 写失败测试：SqliteRunService tool lifecycle**

In `crates/mneme-kernel/src/storage/sqlite.rs` tests add:

```rust
#[tokio::test]
async fn sqlite_run_service_records_tool_request_and_result() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let service = SqliteRunService::new(store.clone());
    let created = service.create_run("读取文件".to_string()).await.unwrap();

    let requested = service
        .request_tool_call(
            created.run_id,
            "read_file".to_string(),
            ToolEffect::ReadOnly,
            serde_json::json!({ "path": "README.md" }),
        )
        .await
        .unwrap();
    service
        .record_tool_result(
            created.run_id,
            requested.tool_call_id,
            serde_json::json!({ "content": "hello" }),
        )
        .await
        .unwrap();

    let events = service.list_events().await.unwrap();
    let tools = ToolProjector::replay(&events);
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].tool_call_id, requested.tool_call_id);
    assert_eq!(tools[0].status, ToolCallStatus::Completed);
}

#[tokio::test]
async fn sqlite_run_service_rejects_tool_request_after_failed_run_without_side_effects() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let service = SqliteRunService::new(store.clone());
    let created = service.create_run("失败后不能请求工具".to_string()).await.unwrap();
    service.fail_run(created.run_id, "模型失败".to_string()).await.unwrap();
    let event_count_before = service.list_events().await.unwrap().len();

    let error = service
        .request_tool_call(
            created.run_id,
            "read_file".to_string(),
            ToolEffect::ReadOnly,
            serde_json::json!({ "path": "README.md" }),
        )
        .await
        .unwrap_err();

    assert_eq!(error, RunServiceError::RunAlreadyTerminal);
    assert_eq!(service.list_events().await.unwrap().len(), event_count_before);
}
```

- [ ] **Step 5: 实现 SqliteRunService tool methods**

In `SqliteRunService`, add transaction helper that:

1. Begins transaction.
2. Selects `status from runs where run_id = ?1`.
3. Returns `InvalidRunId` for missing row.
4. Returns `RunAlreadyTerminal` for non-`running`.
5. Inserts one or more tool events into `operation_events` with `kind_type_for_event()`.
6. Updates `runs.updated_at` with `where run_id = ?1 and status = 'running'`.
7. Commits.

`request_tool_call()` should insert `ToolCallRequested`, and if policy returns `AutoApproved`, insert `ToolCallApproved` in the same transaction. Side-effect effects only insert request event.

`approve_tool_call()`、`deny_tool_call()`、`record_tool_result()`、`record_tool_failure()` each insert one matching event in a transaction and update run activity.

- [ ] **Step 6: 运行验证**

Run:

```bash
. "$HOME/.cargo/env"
cargo fmt --all --check
cargo test -p mneme-kernel composed_service_records_read_only_tool_request_as_auto_approved
cargo test -p mneme-kernel composed_service_records_side_effect_tool_request_as_requires_approval
cargo test -p mneme-kernel composed_service_rejects_tool_request_after_run_completed
cargo test -p mneme-kernel sqlite_run_service_records_tool_request_and_result
cargo test -p mneme-kernel sqlite_run_service_rejects_tool_request_after_failed_run_without_side_effects
cargo test -p mneme-kernel service
cargo test -p mneme-kernel sqlite_run_service
git diff --check
```

Expected: all PASS。

- [ ] **Step 7: Commit**

```bash
git add crates/mneme-kernel/src/service.rs crates/mneme-kernel/src/storage/sqlite.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add tool lifecycle service methods"
```

---

### Task 3: 暴露 tool HTTP API

**Files:**
- Modify: `crates/mneme-api/src/types.rs`
- Modify: `crates/mneme-api/src/routes.rs`

**Interfaces:**
- Consumes: Task 2 `RunService` tool methods。
- Produces:
  - DTO `ToolCallRequest { pub tool_name: String, pub effect: ToolEffect, pub input_json: serde_json::Value }`
  - DTO `ToolCallResponse { pub run_id: String, pub tool_call_id: String, pub approval: ToolApprovalDecision }`
  - DTO `ToolLifecycleRequest { pub text: String }`
  - DTO `ToolResultRequest { pub output_json: serde_json::Value }`
  - DTO `ToolLifecycleResponse { pub run_id: String, pub tool_call_id: String }`
  - `POST /api/v1/runs/:run_id/tool-calls`
  - `POST /api/v1/runs/:run_id/tool-calls/:tool_call_id/approve`
  - `POST /api/v1/runs/:run_id/tool-calls/:tool_call_id/deny`
  - `POST /api/v1/runs/:run_id/tool-calls/:tool_call_id/result`
  - `POST /api/v1/runs/:run_id/tool-calls/:tool_call_id/fail`

- [ ] **Step 1: 写失败测试：tool request API**

In `crates/mneme-api/src/routes.rs` tests add:

```rust
#[tokio::test]
async fn api_records_read_only_tool_call_as_auto_approved() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let service = SqliteRunService::new(store).shared();
    let app = build_router_with_service(service);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"instruction":"读取文件"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let create_body = create_response.into_body().collect().await.unwrap().to_bytes();
    let created: CreateRunResponse = serde_json::from_slice(&create_body).unwrap();

    let tool_response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/runs/{}/tool-calls", created.run_id))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"tool_name":"read_file","effect":"read_only","input_json":{"path":"README.md"}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(tool_response.status(), StatusCode::OK);
    let body = tool_response.into_body().collect().await.unwrap().to_bytes();
    let tool: ToolCallResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(tool.run_id, created.run_id);
    assert_eq!(tool.approval, ToolApprovalDecision::AutoApproved);
    assert_eq!(tool.tool_call_id.len(), 36);
}
```

- [ ] **Step 2: 运行失败测试**

Run:

```bash
. "$HOME/.cargo/env"
cargo test -p mneme-api api_records_read_only_tool_call_as_auto_approved
```

Expected: FAIL，包含 `cannot find type ToolCallResponse` 或 HTTP 404。

- [ ] **Step 3: 实现 DTO**

In `crates/mneme-api/src/types.rs` add imports:

```rust
use mneme_kernel::{ToolApprovalDecision, ToolEffect};
use serde_json::Value;
```

Add DTOs:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub tool_name: String,
    pub effect: ToolEffect,
    pub input_json: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallResponse {
    pub run_id: String,
    pub tool_call_id: String,
    pub approval: ToolApprovalDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolLifecycleRequest {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResultRequest {
    pub output_json: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolLifecycleResponse {
    pub run_id: String,
    pub tool_call_id: String,
}
```

- [ ] **Step 4: 实现 API routes**

In `crates/mneme-api/src/routes.rs`, import `ToolCallId` and DTOs. Add routes under `/api/v1/runs/:run_id/tool-calls` and parse IDs with `RunId::from_str()` / `ToolCallId::from_str()`。

Handlers:

```rust
async fn request_tool_call(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<ToolCallRequest>,
) -> Result<Json<ToolCallResponse>, StatusCode> {
    let run_id = RunId::from_str(&run_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let outcome = state
        .service
        .request_tool_call(run_id, request.tool_name, request.effect, request.input_json)
        .await
        .map_err(service_error_to_status)?;
    Ok(Json(ToolCallResponse {
        run_id: outcome.run_id.to_string(),
        tool_call_id: outcome.tool_call_id.to_string(),
        approval: outcome.approval,
    }))
}
```

Use the same pattern for approve/deny/result/fail. Map invalid UUIDs to `400 BAD_REQUEST`.

- [ ] **Step 5: 写失败测试：approval/result endpoints**

Add tests:

```rust
#[tokio::test]
async fn api_records_tool_result_after_auto_approved_request() {
    // Create run, POST read_only tool request, then POST result.
    // Assert result endpoint returns 200 and same tool_call_id.
}

#[tokio::test]
async fn api_rejects_tool_request_after_completed_run() {
    // Create run, complete it, then POST tool request.
    // Assert StatusCode::BAD_REQUEST.
}
```

Use full request-building code as in Step 1; do not introduce helper functions unless the test file already has equivalent helper style.

- [ ] **Step 6: 运行验证**

Run:

```bash
. "$HOME/.cargo/env"
cargo fmt --all --check
cargo test -p mneme-api tool_call
cargo test -p mneme-api lifecycle
cargo test -p mneme-api
cargo test --workspace
git diff --check
```

Expected: all PASS。

- [ ] **Step 7: Commit**

```bash
git add crates/mneme-api/src/types.rs crates/mneme-api/src/routes.rs
git commit -m "feat: expose tool approval API"
```

---

### Task 4: 增加 TypeScript tool API client

**Files:**
- Modify: `apps/desktop/src/api/client.ts`
- Modify: `apps/desktop/src/api/client.test.ts`
- Modify: `apps/desktop/src/app/App.test.tsx`

**Interfaces:**
- Consumes: Task 3 HTTP endpoints。
- Produces:
  - `ToolEffect = 'read_only' | 'local_write' | 'network' | 'shell'`
  - `ToolApprovalDecision = 'auto_approved' | 'requires_approval'`
  - `ToolCallRequest`、`ToolCallResponse`、`ToolLifecycleRequest`、`ToolResultRequest`、`ToolLifecycleResponse`
  - `MnemeApiClient.requestToolCall(runId: string, request: ToolCallRequest): Promise<ToolCallResponse>`
  - `approveToolCall(runId: string, toolCallId: string, text: string): Promise<ToolLifecycleResponse>`
  - `denyToolCall(runId: string, toolCallId: string, text: string): Promise<ToolLifecycleResponse>`
  - `recordToolResult(runId: string, toolCallId: string, outputJson: unknown): Promise<ToolLifecycleResponse>`
  - `recordToolFailure(runId: string, toolCallId: string, text: string): Promise<ToolLifecycleResponse>`

- [ ] **Step 1: 写失败测试：client endpoints**

In `apps/desktop/src/api/client.test.ts` add:

```ts
it('requests a tool call through the local API', async () => {
  const fetchMock = vi.fn(async () =>
    new Response(
      JSON.stringify({
        run_id: 'run-123',
        tool_call_id: 'tool-123',
        approval: 'auto_approved',
      }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    ),
  );
  const client = createMnemeApiClient('http://127.0.0.1:3766/', fetchMock as typeof fetch);

  const response = await client.requestToolCall('run-123', {
    tool_name: 'read_file',
    effect: 'read_only',
    input_json: { path: 'README.md' },
  });

  expect(response.tool_call_id).toBe('tool-123');
  expect(response.approval).toBe('auto_approved');
  expect(fetchMock).toHaveBeenCalledWith('http://127.0.0.1:3766/api/v1/runs/run-123/tool-calls', {
    method: 'POST',
    headers: { accept: 'application/json', 'content-type': 'application/json' },
    body: JSON.stringify({
      tool_name: 'read_file',
      effect: 'read_only',
      input_json: { path: 'README.md' },
    }),
  });
});

it('records a tool result through the local API', async () => {
  const fetchMock = vi.fn(async () =>
    new Response(JSON.stringify({ run_id: 'run-123', tool_call_id: 'tool-123' }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    }),
  );
  const client = createMnemeApiClient('http://127.0.0.1:3766', fetchMock as typeof fetch);

  const response = await client.recordToolResult('run-123', 'tool-123', { content: 'hello' });

  expect(response).toEqual({ run_id: 'run-123', tool_call_id: 'tool-123' });
  expect(fetchMock).toHaveBeenCalledWith(
    'http://127.0.0.1:3766/api/v1/runs/run-123/tool-calls/tool-123/result',
    {
      method: 'POST',
      headers: { accept: 'application/json', 'content-type': 'application/json' },
      body: JSON.stringify({ output_json: { content: 'hello' } }),
    },
  );
});
```

- [ ] **Step 2: 运行失败测试**

Run:

```bash
pnpm --dir apps/desktop test -- src/api/client.test.ts
```

Expected: FAIL，包含 `client.requestToolCall is not a function`。

- [ ] **Step 3: 实现 TypeScript 类型与方法**

In `apps/desktop/src/api/client.ts` add types:

```ts
export type ToolEffect = 'read_only' | 'local_write' | 'network' | 'shell';
export type ToolApprovalDecision = 'auto_approved' | 'requires_approval';

export interface ToolCallRequest {
  tool_name: string;
  effect: ToolEffect;
  input_json: unknown;
}

export interface ToolCallResponse {
  run_id: string;
  tool_call_id: string;
  approval: ToolApprovalDecision;
}

export interface ToolLifecycleRequest {
  text: string;
}

export interface ToolResultRequest {
  output_json: unknown;
}

export interface ToolLifecycleResponse {
  run_id: string;
  tool_call_id: string;
}
```

Extend `MnemeApiClient` with all methods from **Interfaces**. Add helper:

```ts
async function postToolText(
  normalizedBaseUrl: string,
  fetchImpl: FetchLike,
  runId: string,
  toolCallId: string,
  suffix: string,
  text: string,
): Promise<ToolLifecycleResponse> {
  const response = await fetchImpl(
    `${normalizedBaseUrl}/api/v1/runs/${runId}/tool-calls/${toolCallId}/${suffix}`,
    {
      method: 'POST',
      headers: { accept: 'application/json', 'content-type': 'application/json' },
      body: JSON.stringify({ text }),
    },
  );
  return parseJsonResponse<ToolLifecycleResponse>(response);
}
```

Implement methods in `createMnemeApiClient()` using exact endpoint suffixes from Task 3.

Update `apps/desktop/src/app/App.test.tsx` test double to include all new methods returning `{ run_id: 'run-123', tool_call_id: 'tool-123' }` where needed.

- [ ] **Step 4: 运行验证**

Run:

```bash
pnpm --dir apps/desktop check
pnpm --dir apps/desktop test -- src/api/client.test.ts
. "$HOME/.cargo/env" && pnpm check
git diff --check
```

Expected: all PASS。

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/api/client.ts apps/desktop/src/api/client.test.ts apps/desktop/src/app/App.test.tsx
git commit -m "feat: add tool approval API client"
```

---

### Task 5: Workbench 展示 tool activity

**Files:**
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Modify: `apps/desktop/src/styles.css`

**Interfaces:**
- Consumes: `EventsResponse.events: OperationEvent[]` and tool event payloads from Task 1。
- Produces: Workbench 中一个只读 `Tool Activity` 区块，展示 tool request / approval / result / failure 的摘要。

- [ ] **Step 1: 写失败测试：显示 tool activity**

In `apps/desktop/src/app/App.test.tsx`, add an event fixture with kind payloads:

```ts
it('shows tool activity from the operation ledger', async () => {
  const client = createTestClient({
    events: {
      events: [
        {
          id: 'event-1',
          run_id: 'run-123',
          occurred_at: '2026-11-10T00:00:00Z',
          kind: {
            type: 'tool_call_requested',
            tool_call_id: 'tool-123',
            tool_name: 'read_file',
            effect: 'read_only',
            input_json: { path: 'README.md' },
          },
        },
        {
          id: 'event-2',
          run_id: 'run-123',
          occurred_at: '2026-11-10T00:00:01Z',
          kind: {
            type: 'tool_call_approved',
            tool_call_id: 'tool-123',
            approved_by: 'mneme-policy',
          },
        },
      ],
    },
  });

  render(<App client={client} />);

  expect(await screen.findByText('Tool Activity')).toBeInTheDocument();
  expect(await screen.findByText('read_file')).toBeInTheDocument();
  expect(await screen.findByText('read_only')).toBeInTheDocument();
  expect(await screen.findByText('mneme-policy')).toBeInTheDocument();
});
```

- [ ] **Step 2: 运行失败测试**

Run:

```bash
pnpm --dir apps/desktop test -- src/app/App.test.tsx
```

Expected: FAIL，Testing Library 找不到 `Tool Activity`。

- [ ] **Step 3: 实现 tool activity 解析和展示**

In `App.tsx`, add local types and helper:

```ts
interface ToolActivityItem {
  eventId: string;
  runId: string;
  occurredAt: string;
  toolCallId: string;
  label: string;
  detail: string;
  effect?: string;
}

function toolActivityFromEvents(events: EventsResponse): ToolActivityItem[] {
  return events.events.flatMap((event) => {
    const kind = event.kind as Record<string, unknown>;
    if (kind.type === 'tool_call_requested') {
      return [
        {
          eventId: event.id,
          runId: event.run_id,
          occurredAt: event.occurred_at,
          toolCallId: String(kind.tool_call_id),
          label: String(kind.tool_name),
          detail: 'requested',
          effect: String(kind.effect),
        },
      ];
    }
    if (kind.type === 'tool_call_approved') {
      return [
        {
          eventId: event.id,
          runId: event.run_id,
          occurredAt: event.occurred_at,
          toolCallId: String(kind.tool_call_id),
          label: 'approved',
          detail: String(kind.approved_by),
        },
      ];
    }
    if (kind.type === 'tool_call_denied' || kind.type === 'tool_call_failed') {
      return [
        {
          eventId: event.id,
          runId: event.run_id,
          occurredAt: event.occurred_at,
          toolCallId: String(kind.tool_call_id),
          label: String(kind.type),
          detail: String(kind.reason),
        },
      ];
    }
    if (kind.type === 'tool_call_completed') {
      return [
        {
          eventId: event.id,
          runId: event.run_id,
          occurredAt: event.occurred_at,
          toolCallId: String(kind.tool_call_id),
          label: 'completed',
          detail: 'result recorded',
        },
      ];
    }
    return [];
  });
}
```

Render after Runs section:

```tsx
<section className="panel">
  <h2>Tool Activity</h2>
  {toolActivityFromEvents(events).length === 0 ? (
    <p className="muted">还没有工具请求。Milestone 4 只展示工具意图和审批记录，不执行真实工具。</p>
  ) : (
    <ul className="tool-activity-list">
      {toolActivityFromEvents(events).map((activity) => (
        <li key={activity.eventId} className="tool-activity-card">
          <span className="tool-activity-kind">{activity.label}</span>
          {activity.effect ? <span className="tool-effect">{activity.effect}</span> : null}
          <strong>{activity.detail}</strong>
          <small>{activity.occurredAt}</small>
        </li>
      ))}
    </ul>
  )}
</section>
```

Add CSS classes in `styles.css` for `.tool-activity-list`、`.tool-activity-card`、`.tool-activity-kind`、`.tool-effect`.

- [ ] **Step 4: 运行验证**

Run:

```bash
pnpm --dir apps/desktop check
pnpm --dir apps/desktop test -- src/app/App.test.tsx
. "$HOME/.cargo/env" && pnpm check
git diff --check
```

Expected: all PASS。

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/app/App.tsx apps/desktop/src/app/App.test.tsx apps/desktop/src/styles.css
git commit -m "feat: show tool activity in workbench"
```

---

### Task 6: 文档化 Effect-Typed Tools 与 Approval 边界

**Files:**
- Modify: `README.md`
- Create: `docs/architecture/milestone-4-effect-typed-tools-approval.md`
- Modify: `docs/architecture/milestone-3-agent-run-loop-mvp.md`
- Modify: `docs/architecture/phase-1-foundation.md`

**Interfaces:**
- Consumes: Milestone 4 code/API/UI behavior。
- Produces: Chinese architecture/runbook docs for tool event model, approval policy, HTTP curl examples, and explicit non-goals.

- [ ] **Step 1: 写架构文档**

Create `docs/architecture/milestone-4-effect-typed-tools-approval.md` with sections:

```markdown
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
```

- [ ] **Step 2: 更新 README runbook**

Add curl examples for:

```bash
curl -s http://127.0.0.1:3766/api/v1/runs \
  -H 'content-type: application/json' \
  -d '{"instruction":"读取 README"}'

curl -s http://127.0.0.1:3766/api/v1/runs/<run_id>/tool-calls \
  -H 'content-type: application/json' \
  -d '{"tool_name":"read_file","effect":"read_only","input_json":{"path":"README.md"}}'

curl -s http://127.0.0.1:3766/api/v1/runs/<run_id>/tool-calls/<tool_call_id>/result \
  -H 'content-type: application/json' \
  -d '{"output_json":{"content":"示例内容"}}'
```

Also state in Chinese: “这些 endpoint 只记录工具事件，不执行真实文件读取。”

- [ ] **Step 3: 更新 M3 和 phase doc handoff**

In `docs/architecture/milestone-3-agent-run-loop-mvp.md`, add a short section saying Milestone 4 extends the same ledger/service boundary with tool events.

In `docs/architecture/phase-1-foundation.md`, add one sentence to the milestone evolution section: Milestone 4 adds effect-typed tool approval event records while keeping daemon/API adapters behind `RunService`.

- [ ] **Step 4: 运行文档验证**

Run:

```bash
git diff --check
python3 - <<'PY'
from pathlib import Path
needles = ['TB' + 'D', 'TO' + 'DO', 'place' + 'holder', 'FIX' + 'ME']
for path in [Path('README.md'), *Path('docs/architecture').glob('*.md')]:
    text = path.read_text()
    for needle in needles:
        if needle in text:
            raise SystemExit(f'{path}: contains {needle}')
print('docs scan clean')
PY
. "$HOME/.cargo/env" && pnpm check
```

Expected: all PASS。

- [ ] **Step 5: Commit**

```bash
git add README.md docs/architecture/milestone-4-effect-typed-tools-approval.md docs/architecture/milestone-3-agent-run-loop-mvp.md docs/architecture/phase-1-foundation.md
git commit -m "docs: document effect typed tools milestone"
```

---

### Task 7: Milestone 4 final verification and review

**Files:**
- No code changes expected.
- Append progress only to `.superpowers/sdd/progress.md` after final review approval.

**Interfaces:**
- Consumes: Tasks 1-6。
- Produces: final verification evidence and broad review package for Milestone 4.

- [ ] **Step 1: Run full verification**

Run:

```bash
. "$HOME/.cargo/env"
git status --short --branch
git log --oneline 16e2781..HEAD
pnpm check
git diff --check
git diff --stat 16e2781..HEAD
git status --short --branch
```

Expected:
- Status clean before and after.
- `pnpm check` PASS for Rust and desktop checks.
- `git diff --check` PASS.
- Diff range includes exactly the Milestone 4 plan and implementation commits.

- [ ] **Step 2: Create review package**

Run from the repository root:

```bash
/Users/baiye/.agents/skills/subagent-driven-development/scripts/review-package 16e278117c398c485a0b955529e3e7e39a727dbc HEAD
```

Expected: script prints a review package path under `.superpowers/sdd/`.

- [ ] **Step 3: Dispatch broad reviewer**

Reviewer must check:

- Tool domain and event model exist and serialize cleanly.
- Approval policy matches exact mapping: `read_only` auto-approved; `local_write`、`network`、`shell` require approval.
- All tool writes go through `RunService`; API does not bypass service boundary.
- Terminal runs reject tool events.
- SQLite tool writes are transactional where `SqliteRunService` performs multi-event auto approval.
- HTTP API and TS client are additive.
- Workbench only displays tool activity; it does not claim real tool execution.
- Docs state non-goals clearly.
- `pnpm check` and `git diff --check` pass.

- [ ] **Step 4: Handle findings**

If reviewer reports Critical or Important findings, dispatch one fix implementer with the complete finding list, require targeted tests plus `pnpm check`, then re-run review-package from `16e278117c398c485a0b955529e3e7e39a727dbc..HEAD` and re-review.

If only Minor findings remain, record them in `.superpowers/sdd/progress.md` as deferred follow-ups.

- [ ] **Step 5: Mark Milestone 4 complete**

Append to `.superpowers/sdd/progress.md`:

```markdown
Milestone 4: complete (commits 16e2781..<head7>, final verification passed, overall review approved; follow-ups for Milestone 5: connect tool activity records to artifacts/context inspector views without executing real tools unless a later milestone explicitly adds execution)
```

Do not commit `.superpowers/sdd/progress.md`.

---

## Milestone 4 Success Criteria

- Kernel exposes `ToolCallId`, `ToolEffect`, `ToolApprovalDecision`, `ToolApprovalPolicy`, and tool event constructors.
- Operation Ledger can roundtrip tool events through SQLite.
- `RunService` owns tool request / approval / denial / result / failure methods and rejects terminal runs.
- `read_only` tool requests auto-append approval by `mneme-policy`; side-effect effects require later approval.
- API exposes additive tool endpoints under `/api/v1/runs/:run_id/tool-calls`.
- TypeScript client exposes all tool methods with exact endpoint paths.
- Workbench displays tool activity from ledger events and does not execute tools.
- Docs explain scope, policy, event flow, HTTP runbook, and non-goals.
- `. "$HOME/.cargo/env" && pnpm check` passes.
- `git diff --check` passes.

## Self-Review

- Spec coverage: Milestone 4 roadmap requires effect classification, approval policy, and ledger records for tool call/request/result. Tasks 1-3 implement Kernel/API records; Task 4 adds client; Task 5 displays records; Task 6 documents boundaries; Task 7 verifies the whole milestone.
- Type consistency: `ToolCallId`, `ToolEffect`, `ToolApprovalDecision`, `ToolCallOutcome`, `ToolLifecycleOutcome`, DTO names, endpoint suffixes, and TS client methods are consistent across tasks.
- Scope control: no task executes real tools, invokes MCP/OpenAPI/CLI/local scripts, adds Tauri, or adds cloud sync.
