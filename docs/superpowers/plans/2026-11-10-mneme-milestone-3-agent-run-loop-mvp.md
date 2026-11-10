# Mneme Milestone 3 Agent Run Loop MVP 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在现有 Kernel `RunService` 边界之后，加入一个“无工具、无真实模型调用”的 Agent run 生命周期 MVP，让单个 run 能通过追加事件表达 `started → model_requested → assistant_message → completed/failed`。

**Architecture:** Kernel 继续是事实源和业务边界；所有生命周期推进都通过 `RunService` 方法追加 `OperationEvent` 并更新 `runs` 投影。Milestone 3 不接入真实 LLM，也不实现工具审批；它只提供可测试的生命周期事件、服务方法、HTTP 适配、TypeScript client 与 Workbench 展示，为 Milestone 4/7 的工具与 worker runtime 铺路。

**Tech Stack:** Rust 2021, async-trait 0.1, sqlx 0.8 SQLite, Axum 0.7, Tokio 1.41, React + Vite + TypeScript, Vitest, pnpm 11.9.0。

## Global Constraints

- Mneme 是 Personal AI Kernel（个人 AI 内核），当前主线不是通用 Work Graph Agent OS。
- Runtime shape 保持 Desktop App + Local Daemon/API。
- Kernel 保持 Rust-first；UI 保持 TypeScript/React。
- Operation Ledger 仍是 append-only 事实源；`runs` 仍是可重建 projection/read model。
- HTTP/local API 必须保留；后续 typed RPC/event bus 不在本里程碑实现。
- Milestone 3 只实现无工具、无真实模型调用的 run 生命周期 MVP。
- 不实现 MCP/OpenAPI/CLI 工具协议、权限审批、Artifact 系统、搜索/记忆、Tauri 打包、自托管服务器。
- 中文文档与注释优先；代码标识仍用英文并保持 Rust/TypeScript 惯例。
- 所有代码任务遵循 TDD：先写失败测试，再写最小实现，再运行验证。
- 每个 task 完成后单独 commit，并通过 task reviewer 审查。

---

## Milestone 3 范围

Milestone 3 的边界是“生命周期可记录、可查询、可投影、可在 UI 中看见”。它不是一个真正会调用模型的 agent executor。

### 本里程碑新增概念

- `RunModelRequested`：Kernel 记录“准备向模型请求回复”的事件，包含要发送给模型的 `prompt`。
- `RunAssistantMessage`：Kernel 记录“助手回复内容已产生”的事件，包含 `content`。
- 服务方法：API、daemon、未来 worker 都通过 `RunService` 推进生命周期。
- API 适配：提供最小 HTTP 端点让开发者手动推进生命周期，方便验证事件流和投影。
- Workbench 展示：Run 列表显示更丰富状态信息，能看见 completed/failed summary/error。

### 本里程碑不做

- 不接入 OpenAI/DeepSeek/本地模型。
- 不实现 token streaming。
- 不实现 tool calls、tool approval 或权限模型。
- 不实现 artifact/context inspector。
- 不实现后台 worker runtime 或队列。
- 不改变 daemon 的网络暴露策略。

---

## 文件职责规划

- `crates/mneme-kernel/src/events.rs`
  - 扩展 `OperationEventKind`，新增 `RunModelRequested` 与 `RunAssistantMessage`。
  - 新增构造函数 `OperationEvent::new_run_model_requested(...)` 与 `OperationEvent::new_run_assistant_message(...)`。
- `crates/mneme-kernel/src/projections.rs`
  - 保持 `RunStatus` 三态：`running | completed | failed`。
  - 在 replay 时遇到中间生命周期事件，保持 run 为 `running` 并更新 `updated_at`。
- `crates/mneme-kernel/src/runs.rs`
  - `RunProjectionStore::apply()` 的内存实现支持新事件。
- `crates/mneme-kernel/src/storage/sqlite.rs`
  - SQLite ledger 自动持久化新事件 JSON。
  - SQLite projection store 支持新事件更新 `updated_at`。
  - `SqliteRunService` 为 lifecycle 方法提供事务性写入。
- `crates/mneme-kernel/src/service.rs`
  - 扩展 `RunService` trait。
  - 增加 lifecycle outcome 类型与错误映射。
  - `ComposedRunService` 通过已有 ledger/store 组合实现生命周期方法。
- `crates/mneme-api/src/types.rs`
  - 增加 lifecycle request/response DTO。
- `crates/mneme-api/src/routes.rs`
  - 增加 `/api/v1/runs/:run_id/model-requests`、`/api/v1/runs/:run_id/assistant-messages`、`/api/v1/runs/:run_id/complete`、`/api/v1/runs/:run_id/fail`。
- `apps/desktop/src/api/client.ts`
  - 增加 TypeScript lifecycle client 方法。
- `apps/desktop/src/app/App.tsx`
  - 展示 completed summary、failed error、中间状态说明。
- `README.md` 与 `docs/architecture/milestone-3-agent-run-loop-mvp.md`
  - 记录 run lifecycle MVP 的边界、端点、验证命令和后续 Milestone 4/7 衔接。

---

### Task 1: 扩展 OperationEvent lifecycle 事件模型

**Files:**
- Modify: `crates/mneme-kernel/src/events.rs`
- Modify: `crates/mneme-kernel/src/projections.rs`
- Modify: `crates/mneme-kernel/src/runs.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`

**Interfaces:**
- Consumes: `OperationEvent`, `OperationEventKind`, `RunProjector::replay(events: &[OperationEvent]) -> Vec<RunProjection>`。
- Produces:
  - `OperationEventKind::RunModelRequested { prompt: String }`
  - `OperationEventKind::RunAssistantMessage { content: String }`
  - `OperationEvent::new_run_model_requested(run_id: RunId, prompt: impl Into<String>) -> Self`
  - `OperationEvent::new_run_assistant_message(run_id: RunId, content: impl Into<String>) -> Self`
  - Projection stores treat both new events as lifecycle activity for existing running runs.

- [ ] **Step 1: 写失败测试：事件构造函数**

在 `crates/mneme-kernel/src/events.rs` 的 `#[cfg(test)] mod tests` 中追加：

```rust
#[test]
fn run_model_requested_event_captures_prompt() {
    let run_id = RunId::new();
    let event = OperationEvent::new_run_model_requested(run_id, "请总结今天的工作");

    assert_eq!(event.run_id, run_id);
    assert!(matches!(
        event.kind,
        OperationEventKind::RunModelRequested { ref prompt }
            if prompt == "请总结今天的工作"
    ));
}

#[test]
fn run_assistant_message_event_captures_content() {
    let run_id = RunId::new();
    let event = OperationEvent::new_run_assistant_message(run_id, "今天完成了服务边界收敛。");

    assert_eq!(event.run_id, run_id);
    assert!(matches!(
        event.kind,
        OperationEventKind::RunAssistantMessage { ref content }
            if content == "今天完成了服务边界收敛。"
    ));
}
```

- [ ] **Step 2: 运行失败测试**

Run:

```bash
. "$HOME/.cargo/env"
cargo test -p mneme-kernel run_model_requested_event_captures_prompt
cargo test -p mneme-kernel run_assistant_message_event_captures_content
```

Expected: FAIL，包含 `no variant named 'RunModelRequested'` 或 `no associated function named 'new_run_model_requested'`。

- [ ] **Step 3: 实现事件枚举与构造函数**

在 `OperationEventKind` 中加入：

```rust
RunModelRequested { prompt: String },
RunAssistantMessage { content: String },
```

在 `impl OperationEvent` 中加入：

```rust
pub fn new_run_model_requested(run_id: RunId, prompt: impl Into<String>) -> Self {
    Self {
        id: EventId::new(),
        run_id,
        occurred_at: OffsetDateTime::now_utc(),
        kind: OperationEventKind::RunModelRequested {
            prompt: prompt.into(),
        },
    }
}

pub fn new_run_assistant_message(run_id: RunId, content: impl Into<String>) -> Self {
    Self {
        id: EventId::new(),
        run_id,
        occurred_at: OffsetDateTime::now_utc(),
        kind: OperationEventKind::RunAssistantMessage {
            content: content.into(),
        },
    }
}
```

- [ ] **Step 4: 写失败测试：projection replay 处理中间事件**

在 `crates/mneme-kernel/src/projections.rs` 的 tests 中追加：

```rust
#[test]
fn replay_keeps_run_running_after_model_and_assistant_events() {
    let run_id = RunId::new();
    let events = vec![
        OperationEvent::new_run_started(run_id, "整理今天的工作"),
        OperationEvent::new_run_model_requested(run_id, "请总结今天的工作"),
        OperationEvent::new_run_assistant_message(run_id, "今天完成了服务边界收敛。"),
    ];

    let runs = RunProjector::replay(&events);

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].run_id, run_id);
    assert_eq!(runs[0].status, RunStatus::Running);
    assert_eq!(runs[0].instruction, "整理今天的工作");
    assert_eq!(runs[0].updated_at, events[2].occurred_at);
}
```

- [ ] **Step 5: 运行 projection 失败测试**

Run:

```bash
. "$HOME/.cargo/env"
cargo test -p mneme-kernel replay_keeps_run_running_after_model_and_assistant_events
```

Expected: FAIL，`updated_at` 仍停留在 `RunStarted` 事件时间，或 match 未覆盖新事件导致编译失败。

- [ ] **Step 6: 实现 replay 中间事件行为**

在 `RunProjector::replay()` 的 `match &event.kind` 中加入：

```rust
OperationEventKind::RunModelRequested { .. } | OperationEventKind::RunAssistantMessage { .. } => {
    if let Some(run) = runs.iter_mut().find(|run| run.run_id == event.run_id) {
        run.status = RunStatus::Running;
        run.updated_at = event.occurred_at;
    }
}
```

- [ ] **Step 7: 更新内存与 SQLite projection store**

在 `InMemoryRunProjectionStore::apply()` 中确保新事件使用 `RunProjector::replay()` 或等价逻辑后能更新 `updated_at`。

在 `SqliteRunProjectionStore::apply()` 的 match 中加入：

```rust
OperationEventKind::RunModelRequested { .. } | OperationEventKind::RunAssistantMessage { .. } => {
    sqlx::query("update runs set status = 'running', updated_at = ?2 where run_id = ?1")
        .bind(event.run_id.to_string())
        .bind(occurred_at)
        .execute(self.store.pool())
        .await
        .map_err(|error| RunProjectionError::Storage(error.to_string()))?;
}
```

- [ ] **Step 8: 写 SQLite ledger roundtrip 测试**

在 `crates/mneme-kernel/src/storage/sqlite.rs` tests 中追加：

```rust
#[tokio::test]
async fn sqlite_ledger_roundtrips_model_and_assistant_events() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let ledger = SqliteOperationLedger::new(store);
    let run_id = RunId::new();

    ledger
        .append(OperationEvent::new_run_started(run_id, "整理今天"))
        .await
        .unwrap();
    ledger
        .append(OperationEvent::new_run_model_requested(run_id, "请总结今天"))
        .await
        .unwrap();
    ledger
        .append(OperationEvent::new_run_assistant_message(run_id, "今天完成了 M2。"))
        .await
        .unwrap();

    let events = ledger.list_for_run(run_id).await.unwrap();

    assert_eq!(events.len(), 3);
    assert!(matches!(events[1].kind, OperationEventKind::RunModelRequested { .. }));
    assert!(matches!(events[2].kind, OperationEventKind::RunAssistantMessage { .. }));
}
```

- [ ] **Step 9: 验证 Task 1**

Run:

```bash
. "$HOME/.cargo/env"
cargo fmt --all --check
cargo test -p mneme-kernel events
cargo test -p mneme-kernel replay_
cargo test -p mneme-kernel sqlite_ledger_roundtrips_model_and_assistant_events
cargo test -p mneme-kernel run_store
cargo test -p mneme-kernel sqlite_run_store
git diff --check
```

Expected: all PASS。

- [ ] **Step 10: Commit**

```bash
git add crates/mneme-kernel/src/events.rs crates/mneme-kernel/src/projections.rs crates/mneme-kernel/src/runs.rs crates/mneme-kernel/src/storage/sqlite.rs
git commit -m "feat: add agent lifecycle events"
```

---

### Task 2: 扩展 RunService lifecycle 方法

**Files:**
- Modify: `crates/mneme-kernel/src/service.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 lifecycle event constructors。
- Produces:
  - `RunLifecycleOutcome { pub run_id: RunId }`
  - `RunServiceError::InvalidRunId`
  - `RunServiceError::InvalidLifecycleInput`
  - `RunService::request_model(&self, run_id: RunId, prompt: String) -> Result<RunLifecycleOutcome, RunServiceError>`
  - `RunService::record_assistant_message(&self, run_id: RunId, content: String) -> Result<RunLifecycleOutcome, RunServiceError>`
  - `RunService::complete_run(&self, run_id: RunId, summary: String) -> Result<RunLifecycleOutcome, RunServiceError>`
  - `RunService::fail_run(&self, run_id: RunId, reason: String) -> Result<RunLifecycleOutcome, RunServiceError>`

- [ ] **Step 1: 写失败测试：ComposedRunService lifecycle**

在 `crates/mneme-kernel/src/service.rs` tests 中追加：

```rust
#[tokio::test]
async fn composed_service_records_full_success_lifecycle() {
    let ledger = InMemoryOperationLedger::new();
    let run_store = InMemoryRunProjectionStore::new();
    let service = ComposedRunService::new(ledger.clone().shared(), run_store.clone().shared());

    let created = service.create_run("整理今天".to_string()).await.unwrap();
    service
        .request_model(created.run_id, "请总结今天".to_string())
        .await
        .unwrap();
    service
        .record_assistant_message(created.run_id, "今天完成了 M2。".to_string())
        .await
        .unwrap();
    service
        .complete_run(created.run_id, "总结完成".to_string())
        .await
        .unwrap();

    let events = service.list_events().await.unwrap();
    let runs = service.list_runs().await.unwrap();

    assert_eq!(events.len(), 4);
    assert_eq!(runs[0].run_id, created.run_id);
    assert_eq!(runs[0].status, RunStatus::Completed);
    assert_eq!(runs[0].summary.as_deref(), Some("总结完成"));
}

#[tokio::test]
async fn composed_service_rejects_blank_lifecycle_input() {
    let service = ComposedRunService::new(
        InMemoryOperationLedger::new().shared(),
        InMemoryRunProjectionStore::new().shared(),
    );
    let run_id = RunId::new();

    let error = service.request_model(run_id, "   ".to_string()).await.unwrap_err();

    assert_eq!(error, RunServiceError::InvalidLifecycleInput);
}
```

- [ ] **Step 2: 运行失败测试**

Run:

```bash
. "$HOME/.cargo/env"
cargo test -p mneme-kernel composed_service_records_full_success_lifecycle
cargo test -p mneme-kernel composed_service_rejects_blank_lifecycle_input
```

Expected: FAIL，缺少 trait 方法或错误枚举。

- [ ] **Step 3: 扩展 service 类型与 trait**

在 `service.rs` 增加：

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunLifecycleOutcome {
    pub run_id: RunId,
}
```

扩展 `RunServiceError`：

```rust
#[error("run id is not known to the service")]
InvalidRunId,
#[error("run lifecycle input must not be blank")]
InvalidLifecycleInput,
```

扩展 `RunService` trait：

```rust
async fn request_model(&self, run_id: RunId, prompt: String) -> Result<RunLifecycleOutcome, RunServiceError>;
async fn record_assistant_message(&self, run_id: RunId, content: String) -> Result<RunLifecycleOutcome, RunServiceError>;
async fn complete_run(&self, run_id: RunId, summary: String) -> Result<RunLifecycleOutcome, RunServiceError>;
async fn fail_run(&self, run_id: RunId, reason: String) -> Result<RunLifecycleOutcome, RunServiceError>;
```

- [ ] **Step 4: 给 ComposedRunService 添加 helper 并实现方法**

在 `impl ComposedRunService` 中加入私有 helper：

```rust
async fn append_and_apply_lifecycle_event(
    &self,
    event: OperationEvent,
) -> Result<RunLifecycleOutcome, RunServiceError> {
    let run_id = event.run_id;
    self.ledger
        .append(event.clone())
        .await
        .map_err(|error| RunServiceError::Ledger(error.to_string()))?;
    self.run_store
        .apply(&event)
        .await
        .map_err(|error| RunServiceError::Projection(error.to_string()))?;
    Ok(RunLifecycleOutcome { run_id })
}

fn non_blank_lifecycle_input(value: String) -> Result<String, RunServiceError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(RunServiceError::InvalidLifecycleInput);
    }
    Ok(value)
}
```

在 `impl RunService for ComposedRunService` 中实现四个方法，分别构造 `OperationEvent::new_run_model_requested`、`new_run_assistant_message`、`new_run_completed`、`new_run_failed` 后调用 helper。

- [ ] **Step 5: 扩展 `lib.rs` re-export**

在 `pub use service::{ ... }` 中加入 `RunLifecycleOutcome`。

- [ ] **Step 6: 写失败测试：SQLite service 生命周期事务**

在 `crates/mneme-kernel/src/storage/sqlite.rs` tests 中追加：

```rust
#[tokio::test]
async fn sqlite_run_service_records_full_success_lifecycle() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let service = SqliteRunService::new(store);

    let created = service.create_run("整理今天".to_string()).await.unwrap();
    service
        .request_model(created.run_id, "请总结今天".to_string())
        .await
        .unwrap();
    service
        .record_assistant_message(created.run_id, "今天完成了 M2。".to_string())
        .await
        .unwrap();
    service
        .complete_run(created.run_id, "总结完成".to_string())
        .await
        .unwrap();

    let events = service.list_events().await.unwrap();
    let runs = service.list_runs().await.unwrap();

    assert_eq!(events.len(), 4);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].run_id, created.run_id);
    assert_eq!(runs[0].status, RunStatus::Completed);
    assert_eq!(runs[0].summary.as_deref(), Some("总结完成"));
}

#[tokio::test]
async fn sqlite_run_service_records_failed_lifecycle() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let service = SqliteRunService::new(store);

    let created = service.create_run("会失败的 run".to_string()).await.unwrap();
    service
        .request_model(created.run_id, "请总结今天".to_string())
        .await
        .unwrap();
    service
        .fail_run(created.run_id, "模型不可用".to_string())
        .await
        .unwrap();

    let runs = service.list_runs().await.unwrap();

    assert_eq!(runs[0].status, RunStatus::Failed);
    assert_eq!(runs[0].error.as_deref(), Some("模型不可用"));
}
```

- [ ] **Step 7: 实现 SQLiteRunService lifecycle 方法**

在 `SqliteRunService` 中加入私有 helper：

```rust
async fn append_lifecycle_event_transactionally(
    &self,
    event: OperationEvent,
) -> Result<RunLifecycleOutcome, RunServiceError> {
    let run_id = event.run_id;
    let occurred_at = event
        .occurred_at
        .format(&Rfc3339)
        .map_err(|error| RunServiceError::Ledger(error.to_string()))?;
    let kind_type = kind_type_for_event(&event.kind);
    let payload_json = serde_json::to_string(&event.kind)
        .map_err(|error| RunServiceError::Ledger(error.to_string()))?;

    let mut tx = self
        .store
        .pool()
        .begin()
        .await
        .map_err(|error| RunServiceError::Ledger(error.to_string()))?;

    sqlx::query(
        "insert into operation_events (event_id, run_id, occurred_at, kind_type, payload_json) values (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(event.id.to_string())
    .bind(run_id.to_string())
    .bind(&occurred_at)
    .bind(kind_type)
    .bind(payload_json)
    .execute(&mut *tx)
    .await
    .map_err(|error| RunServiceError::Ledger(error.to_string()))?;

    match &event.kind {
        OperationEventKind::RunModelRequested { .. }
        | OperationEventKind::RunAssistantMessage { .. } => {
            sqlx::query("update runs set status = 'running', updated_at = ?2 where run_id = ?1")
                .bind(run_id.to_string())
                .bind(&occurred_at)
                .execute(&mut *tx)
                .await
                .map_err(|error| RunServiceError::Projection(error.to_string()))?;
        }
        OperationEventKind::RunCompleted { summary } => {
            sqlx::query("update runs set status = 'completed', completed_at = ?2, summary = ?3, updated_at = ?2 where run_id = ?1")
                .bind(run_id.to_string())
                .bind(&occurred_at)
                .bind(summary)
                .execute(&mut *tx)
                .await
                .map_err(|error| RunServiceError::Projection(error.to_string()))?;
        }
        OperationEventKind::RunFailed { reason } => {
            sqlx::query("update runs set status = 'failed', failed_at = ?2, error = ?3, updated_at = ?2 where run_id = ?1")
                .bind(run_id.to_string())
                .bind(&occurred_at)
                .bind(reason)
                .execute(&mut *tx)
                .await
                .map_err(|error| RunServiceError::Projection(error.to_string()))?;
        }
        OperationEventKind::RunStarted { .. } => unreachable!("create_run handles run_started"),
    }

    tx.commit()
        .await
        .map_err(|error| RunServiceError::Ledger(error.to_string()))?;

    Ok(RunLifecycleOutcome { run_id })
}
```

用该 helper 实现四个 trait 方法。

- [ ] **Step 8: 验证 Task 2**

Run:

```bash
. "$HOME/.cargo/env"
cargo fmt --all --check
cargo test -p mneme-kernel service
cargo test -p mneme-kernel sqlite_run_service
cargo test -p mneme-kernel
git diff --check
```

Expected: all PASS。

- [ ] **Step 9: Commit**

```bash
git add crates/mneme-kernel/src/service.rs crates/mneme-kernel/src/storage/sqlite.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add run lifecycle service methods"
```

---

### Task 3: 暴露 lifecycle HTTP API

**Files:**
- Modify: `crates/mneme-api/src/types.rs`
- Modify: `crates/mneme-api/src/routes.rs`

**Interfaces:**
- Consumes: Task 2 `RunService` lifecycle methods。
- Produces:
  - `LifecycleTextRequest { text: String }`
  - `RunLifecycleResponse { run_id: String }`
  - `POST /api/v1/runs/:run_id/model-requests`
  - `POST /api/v1/runs/:run_id/assistant-messages`
  - `POST /api/v1/runs/:run_id/complete`
  - `POST /api/v1/runs/:run_id/fail`

- [ ] **Step 1: 写失败测试：API 成功 lifecycle**

在 `crates/mneme-api/src/routes.rs` tests 中追加：

```rust
#[tokio::test]
async fn api_records_success_lifecycle_through_service() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let app = build_router_with_service(SqliteRunService::new(store).shared());

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"instruction":"端到端生命周期"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);
    let create_body = create_response.into_body().collect().await.unwrap().to_bytes();
    let created: CreateRunResponse = serde_json::from_slice(&create_body).unwrap();

    for (path, text) in [
        (format!("/api/v1/runs/{}/model-requests", created.run_id), "请总结今天"),
        (format!("/api/v1/runs/{}/assistant-messages", created.run_id), "今天完成了 M2。"),
        (format!("/api/v1/runs/{}/complete", created.run_id), "总结完成"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(path)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({ "text": text }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let runs_response = app
        .oneshot(Request::builder().uri("/api/v1/runs").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = runs_response.into_body().collect().await.unwrap().to_bytes();
    let runs: RunsResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(runs.runs[0].run_id.to_string(), created.run_id);
    assert_eq!(runs.runs[0].status, mneme_kernel::RunStatus::Completed);
    assert_eq!(runs.runs[0].summary.as_deref(), Some("总结完成"));
}
```

- [ ] **Step 2: 运行失败测试**

Run:

```bash
. "$HOME/.cargo/env"
cargo test -p mneme-api api_records_success_lifecycle_through_service
```

Expected: FAIL，返回 404 或缺少 DTO。

- [ ] **Step 3: 增加 DTO**

在 `crates/mneme-api/src/types.rs` 加入：

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleTextRequest {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunLifecycleResponse {
    pub run_id: String,
}
```

- [ ] **Step 4: 增加路由与 handler**

在 `routes.rs` 中引入 `Path`：

```rust
use axum::{extract::{Path, State}, http::StatusCode, routing::get, Json, Router};
```

增加 route：

```rust
.route("/api/v1/runs/:run_id/model-requests", axum::routing::post(request_model))
.route("/api/v1/runs/:run_id/assistant-messages", axum::routing::post(record_assistant_message))
.route("/api/v1/runs/:run_id/complete", axum::routing::post(complete_run))
.route("/api/v1/runs/:run_id/fail", axum::routing::post(fail_run))
```

增加 helper：

```rust
fn parse_run_id(value: &str) -> Result<mneme_kernel::RunId, StatusCode> {
    value.parse().map_err(|_| StatusCode::BAD_REQUEST)
}

fn lifecycle_response(outcome: mneme_kernel::RunLifecycleOutcome) -> Json<RunLifecycleResponse> {
    Json(RunLifecycleResponse {
        run_id: outcome.run_id.to_string(),
    })
}
```

增加四个 handler，示例：

```rust
async fn request_model(
    State(state): State<ApiState>,
    Path(run_id): Path<String>,
    Json(request): Json<LifecycleTextRequest>,
) -> Result<Json<RunLifecycleResponse>, StatusCode> {
    let run_id = parse_run_id(&run_id)?;
    let outcome = state
        .run_service
        .request_model(run_id, request.text)
        .await
        .map_err(service_error_to_status)?;
    Ok(lifecycle_response(outcome))
}
```

另外三个 handler 分别调用 `record_assistant_message`、`complete_run`、`fail_run`。

扩展 `service_error_to_status`：`InvalidRunId`、`InvalidLifecycleInput` 返回 `BAD_REQUEST`。

- [ ] **Step 5: 写失败测试：API blank lifecycle input 返回 400**

在 `routes.rs` tests 中追加：

```rust
#[tokio::test]
async fn api_rejects_blank_lifecycle_text() {
    let app = build_router(InMemoryOperationLedger::new());
    let run_id = mneme_kernel::RunId::new();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/runs/{run_id}/model-requests"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"text":"   "}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 6: 验证 Task 3**

Run:

```bash
. "$HOME/.cargo/env"
cargo fmt --all --check
cargo test -p mneme-api lifecycle
cargo test -p mneme-api
cargo test --workspace
git diff --check
```

Expected: all PASS。

- [ ] **Step 7: Commit**

```bash
git add crates/mneme-api/src/types.rs crates/mneme-api/src/routes.rs
git commit -m "feat: expose run lifecycle API"
```

---

### Task 4: TypeScript client 支持 lifecycle API

**Files:**
- Modify: `apps/desktop/src/api/client.ts`
- Modify: `apps/desktop/src/api/client.test.ts`

**Interfaces:**
- Consumes: Task 3 HTTP lifecycle endpoints。
- Produces:
  - `LifecycleTextRequest`
  - `RunLifecycleResponse`
  - `MnemeApiClient.requestModel(runId: string, text: string)`
  - `MnemeApiClient.recordAssistantMessage(runId: string, text: string)`
  - `MnemeApiClient.completeRun(runId: string, text: string)`
  - `MnemeApiClient.failRun(runId: string, text: string)`

- [ ] **Step 1: 写失败测试：client lifecycle 方法**

在 `apps/desktop/src/api/client.test.ts` 中追加：

```ts
it('posts lifecycle updates to run-specific endpoints', async () => {
  const calls: Array<{ url: string; body?: string }> = [];
  const fetchImpl = vi.fn(async (url: string, init?: RequestInit) => {
    calls.push({ url, body: init?.body?.toString() });
    return new Response(JSON.stringify({ run_id: 'run-1' }), { status: 200 });
  }) as unknown as typeof fetch;

  const client = createMnemeApiClient('http://127.0.0.1:3766/', fetchImpl);

  await client.requestModel('run-1', '请总结今天');
  await client.recordAssistantMessage('run-1', '今天完成了 M2。');
  await client.completeRun('run-1', '总结完成');
  await client.failRun('run-1', '模型不可用');

  expect(calls.map((call) => call.url)).toEqual([
    'http://127.0.0.1:3766/api/v1/runs/run-1/model-requests',
    'http://127.0.0.1:3766/api/v1/runs/run-1/assistant-messages',
    'http://127.0.0.1:3766/api/v1/runs/run-1/complete',
    'http://127.0.0.1:3766/api/v1/runs/run-1/fail',
  ]);
  expect(calls[0].body).toBe(JSON.stringify({ text: '请总结今天' }));
});
```

- [ ] **Step 2: 运行失败测试**

Run:

```bash
pnpm --dir apps/desktop test -- src/api/client.test.ts
```

Expected: FAIL，`client.requestModel is not a function`。

- [ ] **Step 3: 实现 client 类型与方法**

在 `client.ts` 增加：

```ts
export interface LifecycleTextRequest {
  text: string;
}

export interface RunLifecycleResponse {
  run_id: string;
}
```

扩展 `MnemeApiClient`：

```ts
requestModel(runId: string, text: string): Promise<RunLifecycleResponse>;
recordAssistantMessage(runId: string, text: string): Promise<RunLifecycleResponse>;
completeRun(runId: string, text: string): Promise<RunLifecycleResponse>;
failRun(runId: string, text: string): Promise<RunLifecycleResponse>;
```

增加 helper：

```ts
async function postLifecycleText(
  normalizedBaseUrl: string,
  fetchImpl: FetchLike,
  runId: string,
  suffix: string,
  text: string,
): Promise<RunLifecycleResponse> {
  const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/runs/${runId}/${suffix}`, {
    method: 'POST',
    headers: { accept: 'application/json', 'content-type': 'application/json' },
    body: JSON.stringify({ text }),
  });
  return parseJsonResponse<RunLifecycleResponse>(response);
}
```

在 return object 中加入四个方法，分别传入 `model-requests`、`assistant-messages`、`complete`、`fail`。

- [ ] **Step 4: 验证 Task 4**

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
git add apps/desktop/src/api/client.ts apps/desktop/src/api/client.test.ts
git commit -m "feat: add lifecycle API client methods"
```

---

### Task 5: Workbench 展示 lifecycle 状态与结果

**Files:**
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Modify: `apps/desktop/src/styles.css`

**Interfaces:**
- Consumes: `RunProjection.summary`, `RunProjection.error`, `RunProjection.status`, `RunProjection.updated_at`。
- Produces: Workbench 中 run card 展示 lifecycle 信息，不新增复杂交互。

- [ ] **Step 1: 写失败测试：completed/failed run 展示**

在 `apps/desktop/src/app/App.test.tsx` 中追加：

```tsx
it('shows lifecycle result details for completed and failed runs', async () => {
  const client = {
    getHealth: vi.fn(async () => ({ service: 'mneme-kernel', status: 'ready', version: '0.1.0' })),
    listEvents: vi.fn(async () => ({ events: [] })),
    listRuns: vi.fn(async () => ({
      runs: [
        {
          run_id: 'run-completed',
          status: 'completed',
          instruction: '整理今天',
          started_at: '2026-11-10T00:00:00Z',
          completed_at: '2026-11-10T00:01:00Z',
          failed_at: null,
          summary: '总结完成',
          error: null,
          updated_at: '2026-11-10T00:01:00Z',
        },
        {
          run_id: 'run-failed',
          status: 'failed',
          instruction: '失败示例',
          started_at: '2026-11-10T00:00:00Z',
          completed_at: null,
          failed_at: '2026-11-10T00:02:00Z',
          summary: null,
          error: '模型不可用',
          updated_at: '2026-11-10T00:02:00Z',
        },
      ],
    })),
    createRun: vi.fn(),
    requestModel: vi.fn(),
    recordAssistantMessage: vi.fn(),
    completeRun: vi.fn(),
    failRun: vi.fn(),
  };

  render(<App client={client} />);

  expect(await screen.findByText('总结完成')).toBeInTheDocument();
  expect(await screen.findByText('模型不可用')).toBeInTheDocument();
});
```

- [ ] **Step 2: 运行失败测试**

Run:

```bash
pnpm --dir apps/desktop test -- src/app/App.test.tsx
```

Expected: FAIL，找不到 `总结完成` 或 `模型不可用`。

- [ ] **Step 3: 更新 App run card**

在 run card 中加入：

```tsx
{run.summary ? <p className="run-result">总结：{run.summary}</p> : null}
{run.error ? <p className="run-error">错误：{run.error}</p> : null}
```

保留已有 status、instruction、updated_at 展示。

- [ ] **Step 4: 更新 CSS**

在 `apps/desktop/src/styles.css` 加入：

```css
.run-result,
.run-error {
  margin: 0;
  color: var(--mneme-muted);
  font-size: 0.92rem;
  line-height: 1.5;
}

.run-error {
  color: #fca5a5;
}
```

- [ ] **Step 5: 验证 Task 5**

Run:

```bash
pnpm --dir apps/desktop check
pnpm --dir apps/desktop test -- src/app/App.test.tsx
. "$HOME/.cargo/env" && pnpm check
git diff --check
```

Expected: all PASS。

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/app/App.tsx apps/desktop/src/app/App.test.tsx apps/desktop/src/styles.css
git commit -m "feat: show lifecycle results in workbench"
```

---

### Task 6: 文档记录 Agent Run Loop MVP 边界

**Files:**
- Create: `docs/architecture/milestone-3-agent-run-loop-mvp.md`
- Modify: `README.md`
- Modify: `docs/architecture/milestone-2-kernel-service-boundary.md`
- Modify: `docs/architecture/phase-1-foundation.md`

**Interfaces:**
- Consumes: Tasks 1–5 的事件、服务、HTTP、UI 行为。
- Produces: 可复现的本地验证说明与后续 Milestone 4/7 衔接说明。

- [ ] **Step 1: 创建 Milestone 3 架构文档**

创建 `docs/architecture/milestone-3-agent-run-loop-mvp.md`：

```markdown
# Milestone 3: Agent Run Loop MVP

Milestone 3 为 Mneme 增加无工具、无真实模型调用的 run 生命周期骨架。它把 run 从单个 `RunStarted` 事件扩展为可追加的事件流：`RunStarted → RunModelRequested → RunAssistantMessage → RunCompleted/RunFailed`。

## 边界

本里程碑实现生命周期记录、查询、投影和 Workbench 展示；不接入真实 LLM，不实现 streaming，不实现工具调用或审批，不实现 worker runtime。

## 事件流

- `RunStarted { instruction }`：用户创建 run。
- `RunModelRequested { prompt }`：Kernel 记录准备请求模型的 prompt。
- `RunAssistantMessage { content }`：Kernel 记录助手回复内容。
- `RunCompleted { summary }`：run 成功结束。
- `RunFailed { reason }`：run 失败结束。

所有事件写入 `operation_events`，`runs` 表只是可重建读模型。

## Service Boundary

HTTP API、daemon 和未来 worker 都必须调用 `RunService`。Milestone 3 新增 lifecycle 方法，让后续真实 Agent loop 不需要绕过 Kernel 业务边界。

## HTTP 验证

```bash
cargo run -p mneme-daemon
```

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

## 后续衔接

Milestone 4 可以把 tool request/approval 作为新的事件类型追加到同一个 ledger；Milestone 7 可以让 worker runtime 调用同一组 `RunService` lifecycle 方法，而不是直接写 SQLite。
```

- [ ] **Step 2: 更新 README**

在 README 的运行说明中加入 lifecycle 示例，说明这是手动推进的本地验证路径，不代表真实模型调用。

- [ ] **Step 3: 更新 Milestone 2 架构文档**

在 `docs/architecture/milestone-2-kernel-service-boundary.md` 加一段“Milestone 3 之后”说明：`RunService` 现在不仅负责 create/query/repair，也负责 lifecycle event append。

- [ ] **Step 4: 修正 Phase 1 架构文档旧表述**

把 `docs/architecture/phase-1-foundation.md` 中“当前存储为 in-memory”的表述改为历史上下文：Phase 1 初始为 in-memory，Milestone 1/2 后 daemon 默认 SQLite/service data plane。

- [ ] **Step 5: 文档自检**

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
            raise SystemExit(f'{path}: found {needle}')
PY
. "$HOME/.cargo/env" && pnpm check
```

Expected: `git diff --check` 和 `pnpm check` PASS；grep 不应出现本计划外的占位命中。

- [ ] **Step 6: Commit**

```bash
git add README.md docs/architecture/milestone-3-agent-run-loop-mvp.md docs/architecture/milestone-2-kernel-service-boundary.md docs/architecture/phase-1-foundation.md
git commit -m "docs: document agent run loop milestone"
```

---

### Task 7: Milestone 3 final verification and progress notes

**Files:**
- Modify: `.superpowers/sdd/progress.md` only as untracked local progress ledger; do not commit this file.

**Interfaces:**
- Consumes: Tasks 1–6 commits。
- Produces: verified milestone summary and known follow-up list for Milestone 4。

- [ ] **Step 1: Run final verification**

Run:

```bash
. "$HOME/.cargo/env"
git status --short --branch
git log --oneline a9e8f25..HEAD
pnpm check
git diff --check
git diff --stat a9e8f25..HEAD
git status --short --branch
```

Expected:
- branch `phase-1-foundation`
- status clean
- all Rust/UI checks pass
- Milestone 3 commits present after `a9e8f25`

- [ ] **Step 2: Dispatch overall review**

Use the subagent-driven-development final review flow. Review range: `a9e8f25..HEAD`.

The reviewer should check:
- lifecycle event stream supports `started → model_requested → assistant_message → completed/failed`。
- API and daemon still go through `RunService` boundary。
- SQLite lifecycle writes append `operation_events` and update `runs` consistently。
- HTTP shape remains compatible and new endpoints are additive。
- Workbench only displays lifecycle state/results and does not pretend to run a real model。
- no tools/approval/worker runtime/Tauri/cloud scope creep。

- [ ] **Step 3: Update progress ledger after approval**

Append to `.superpowers/sdd/progress.md`:

```markdown
Milestone 3: complete (commits a9e8f25..<HEAD7>, final verification passed, overall review approved; follow-ups for Milestone 4: tool request/approval events should reuse RunService lifecycle boundary and Operation Ledger append semantics)
```

Use the actual short HEAD hash after final review.

- [ ] **Step 4: Commit status**

No git commit is needed for `.superpowers/sdd/progress.md` because `.superpowers/` is gitignored. If final review requires doc/code fixes, make one fix commit and rerun final verification before marking complete.

---

## Milestone 3 Success Criteria

- `OperationEventKind` includes `RunModelRequested` and `RunAssistantMessage`.
- `RunProjector::replay()` and both projection stores treat lifecycle middle events as running activity and update `updated_at`.
- `RunService` exposes lifecycle methods for model request, assistant message, completion, and failure.
- `SqliteRunService` appends lifecycle events and updates `runs` in one service method boundary.
- HTTP API adds additive lifecycle endpoints without breaking existing `GET /api/v1/health`, `GET /api/v1/events`, `GET /api/v1/runs`, `POST /api/v1/runs`.
- TypeScript client can call lifecycle endpoints.
- Workbench displays completed summary and failed error from `RunProjection`.
- Docs clearly state this is a no-tool, no-real-model lifecycle MVP.
- `pnpm check` and `git diff --check` pass.
