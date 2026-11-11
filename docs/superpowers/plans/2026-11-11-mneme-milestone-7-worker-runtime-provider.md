# Mneme Milestone 7 Worker Runtime Provider Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the real OpenAI-compatible non-streaming Provider Worker MVP behind a Mneme-native Worker Runtime boundary.

**Architecture:** Kernel owns worker job lifecycle, Operation Ledger events, SQLite projections, and all run lifecycle writes through `RunService`. Provider workers only execute typed model-completion requests and return typed responses; tests default to mock workers and mock HTTP transport. API, TypeScript client, and Workbench expose the minimal model-completion vertical slice without streaming, tool execution, process supervision, or prompt compilation.

**Tech Stack:** Rust 2021, Tokio, async-trait, serde/serde_json, SQLx SQLite, Axum, TypeScript React/Vite/Vitest, reqwest with rustls for the real OpenAI-compatible HTTP provider.

## Global Constraints

- M7 scope is `真实 OpenAI-compatible 非流式 Provider Worker MVP + Mneme-native Worker Runtime 边界`.
- Tests must not require external network or a real API key.
- Real provider configuration uses `MNEME_OPENAI_BASE_URL`, `MNEME_OPENAI_API_KEY`, `MNEME_OPENAI_MODEL`, `MNEME_OPENAI_TIMEOUT_SECS`.
- `MNEME_OPENAI_API_KEY` must never be written to logs, Operation Ledger, SQLite projection rows, docs examples, reports, or test fixtures.
- Worker code must not directly mutate `runs`; run lifecycle changes must go through `RunService` methods.
- Worker job state is append-only in Operation Ledger and rebuildable into a SQLite projection.
- `OperationEvent` still requires `run_id: RunId`; M7 introduces no global no-run events.
- Worker-driven retrieval/recall defaults to `run_id` scope; cross-run/global recall requires an explicit future opt-in field and is not added in M7.
- No streaming token UI.
- No tool-call execution, MCP, OpenAPI, CLI, or local-script execution.
- No Tauri process management.
- No Node/Python worker process adapter.
- No embedding worker, document parser, OCR worker, prompt compiler, or automatic agent recall.
- UI and docs must not claim full Agent Runtime completion.
- Run Rust commands with `. "$HOME/.cargo/env"`.
- Use one task per commit.

---

## File Structure

### Kernel

- Modify `Cargo.toml`: add workspace dependency `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }`.
- Modify `crates/mneme-kernel/Cargo.toml`: add `reqwest.workspace = true` for the real provider worker.
- Modify `crates/mneme-kernel/src/ids.rs`: add `WorkerId` and `WorkerJobId` following the existing UUID newtype pattern.
- Modify `crates/mneme-kernel/src/events.rs`: add worker job event variants and constructors.
- Create `crates/mneme-kernel/src/workers.rs`: provider worker types, mock provider, OpenAI-compatible provider, worker job records, projection traits, composed runtime service.
- Modify `crates/mneme-kernel/src/storage/sqlite.rs`: add `worker_jobs` projection table support, SQLite projection store, SQLite worker runtime service, ledger mapping.
- Modify `crates/mneme-kernel/src/lib.rs`: export worker IDs, worker runtime/provider types, and SQLite worker services.
- Modify `crates/mneme-kernel/migrations/0001_operation_ledger.sql`: add `worker_jobs` table and index.
- Modify `crates/mneme-kernel/src/projections.rs` and `crates/mneme-kernel/src/runs.rs` only if new `OperationEventKind` variants make exhaustive matches fail; worker job events should update running-run `updated_at` without changing terminal status.

### API / daemon

- Modify `crates/mneme-api/src/types.rs`: add worker model completion request/response DTOs and worker jobs response.
- Modify `crates/mneme-api/src/routes.rs`: add route-level worker runtime service enum, routes, error mapping, and test wiring.
- Modify `crates/mneme-daemon/src/main.rs`: read provider env, build real OpenAI-compatible provider worker or disabled provider, wire SQLite worker runtime service.

### Desktop

- Modify `apps/desktop/src/api/client.ts`: add worker DTOs and non-optional client methods.
- Modify `apps/desktop/src/api/client.test.ts`: add path/body tests.
- Modify `apps/desktop/src/app/App.tsx`: load and render worker jobs.
- Modify `apps/desktop/src/app/App.test.tsx`: update fake client and UI assertions.
- Modify `apps/desktop/src/styles.css`: worker job panel styling.

### Docs

- Modify `README.md`: add worker runtime/provider verification notes and curl examples with fake IDs only.
- Create `docs/architecture/milestone-7-worker-runtime-provider.md`.
- Modify `docs/architecture/phase-1-foundation.md`: add M7 status and M8 handoff.

---

### Task 1: Worker IDs, domain records, and ledger events

**Files:**
- Modify: `crates/mneme-kernel/src/ids.rs`
- Modify: `crates/mneme-kernel/src/events.rs`
- Create: `crates/mneme-kernel/src/workers.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify if exhaustive matches require it: `crates/mneme-kernel/src/projections.rs`
- Modify if exhaustive matches require it: `crates/mneme-kernel/src/runs.rs`

**Interfaces:**
- Consumes: existing `RunId`, `OperationEvent`, `OperationEventKind`, SQLite ledger serialization pattern.
- Produces:
  - `pub struct WorkerId(Uuid);`
  - `pub struct WorkerJobId(Uuid);`
  - `pub enum WorkerJobKind { ModelCompletion }`
  - `pub enum WorkerJobStatus { Started, Completed, Failed }`
  - `pub struct WorkerJobRecord { pub worker_job_id: WorkerJobId, pub worker_id: WorkerId, pub run_id: RunId, pub kind: WorkerJobKind, pub status: WorkerJobStatus, pub model: Option<String>, pub error: Option<String>, pub started_at: OffsetDateTime, pub completed_at: Option<OffsetDateTime> }`
  - `OperationEventKind::WorkerJobStarted { worker_job_id, worker_id, kind, model }`
  - `OperationEventKind::WorkerJobCompleted { worker_job_id, worker_id, kind }`
  - `OperationEventKind::WorkerJobFailed { worker_job_id, worker_id, kind, reason }`
  - `OperationEvent::new_worker_job_started(...)`
  - `OperationEvent::new_worker_job_completed(...)`
  - `OperationEvent::new_worker_job_failed(...)`

- [ ] **Step 1: Add failing ID and event tests**

Add tests in `crates/mneme-kernel/src/workers.rs` and `crates/mneme-kernel/src/events.rs` that assert:

```rust
#[test]
fn worker_ids_are_displayable_and_parseable() {
    let worker_id = WorkerId::new();
    assert_eq!(worker_id.to_string().parse::<WorkerId>().unwrap(), worker_id);

    let job_id = WorkerJobId::new();
    assert_eq!(job_id.to_string().parse::<WorkerJobId>().unwrap(), job_id);
}

#[test]
fn worker_job_started_event_records_runtime_metadata() {
    let run_id = RunId::new();
    let worker_id = WorkerId::new();
    let worker_job_id = WorkerJobId::new();

    let event = OperationEvent::new_worker_job_started(
        run_id,
        worker_job_id,
        worker_id,
        WorkerJobKind::ModelCompletion,
        Some("gpt-4o-mini".to_string()),
    );

    assert_eq!(event.run_id, run_id);
    assert!(matches!(
        event.kind,
        OperationEventKind::WorkerJobStarted { worker_job_id: actual_job_id, worker_id: actual_worker_id, kind: WorkerJobKind::ModelCompletion, model: Some(ref model) }
            if actual_job_id == worker_job_id && actual_worker_id == worker_id && model == "gpt-4o-mini"
    ));
}

#[test]
fn worker_job_failed_event_records_reason_without_api_key() {
    let event = OperationEvent::new_worker_job_failed(
        RunId::new(),
        WorkerJobId::new(),
        WorkerId::new(),
        WorkerJobKind::ModelCompletion,
        "provider returned 500",
    );

    assert!(matches!(
        event.kind,
        OperationEventKind::WorkerJobFailed { reason, .. } if reason == "provider returned 500"
    ));
}
```

Add a SQLite ledger roundtrip test in `crates/mneme-kernel/src/storage/sqlite.rs` named `sqlite_ledger_roundtrips_worker_job_events` that appends started/completed/failed worker events and asserts `list()` returns the same event kinds.

- [ ] **Step 2: Run tests to verify RED**

Run:

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-kernel worker_ids_are_displayable_and_parseable
. "$HOME/.cargo/env" && cargo test -p mneme-kernel worker_job_started_event_records_runtime_metadata
. "$HOME/.cargo/env" && cargo test -p mneme-kernel sqlite_ledger_roundtrips_worker_job_events
```

Expected: fail with missing `WorkerId`, `WorkerJobId`, `WorkerJobKind`, or worker event constructors.

- [ ] **Step 3: Implement IDs and worker domain skeleton**

In `crates/mneme-kernel/src/ids.rs`, add `WorkerId` and `WorkerJobId` following the same `new`, `Default`, `Display`, and `FromStr` pattern as `RetrievalTraceId`.

In `crates/mneme-kernel/src/workers.rs`, add:

```rust
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{RunId, WorkerId, WorkerJobId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerJobKind {
    ModelCompletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerJobStatus {
    Started,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerJobRecord {
    pub worker_job_id: WorkerJobId,
    pub worker_id: WorkerId,
    pub run_id: RunId,
    pub kind: WorkerJobKind,
    pub status: WorkerJobStatus,
    pub model: Option<String>,
    pub error: Option<String>,
    pub started_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
}
```

- [ ] **Step 4: Implement event variants and SQLite ledger mapping**

In `crates/mneme-kernel/src/events.rs`, add worker job variants and constructors exactly matching this plan. Update `kind_type_for_event` in `crates/mneme-kernel/src/storage/sqlite.rs` to map:

```rust
OperationEventKind::WorkerJobStarted { .. } => "worker_job_started",
OperationEventKind::WorkerJobCompleted { .. } => "worker_job_completed",
OperationEventKind::WorkerJobFailed { .. } => "worker_job_failed",
```

If `projections.rs` or `runs.rs` exhaustive matches fail, include worker job events in the same running-run `updated_at` behavior as M6 retrieval traces. Do not change terminal status semantics.

- [ ] **Step 5: Export types and run focused verification**

Update `crates/mneme-kernel/src/lib.rs`:

```rust
pub mod workers;

pub use ids::{WorkerId, WorkerJobId};
pub use workers::{WorkerJobKind, WorkerJobRecord, WorkerJobStatus};
```

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check
. "$HOME/.cargo/env" && cargo test -p mneme-kernel worker_ids
. "$HOME/.cargo/env" && cargo test -p mneme-kernel worker_job_started_event_records_runtime_metadata
. "$HOME/.cargo/env" && cargo test -p mneme-kernel sqlite_ledger_roundtrips_worker_job_events
. "$HOME/.cargo/env" && cargo test -p mneme-kernel
. "$HOME/.cargo/env" && git diff --check
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/mneme-kernel/src/ids.rs crates/mneme-kernel/src/events.rs crates/mneme-kernel/src/workers.rs crates/mneme-kernel/src/lib.rs crates/mneme-kernel/src/storage/sqlite.rs crates/mneme-kernel/src/projections.rs crates/mneme-kernel/src/runs.rs
git commit -m "feat: add worker runtime event model"
```

---

### Task 2: Provider worker abstraction and OpenAI-compatible HTTP mapping

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/mneme-kernel/Cargo.toml`
- Modify: `crates/mneme-kernel/src/workers.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: `WorkerId`, `RunId` from Task 1.
- Produces:
  - `pub struct ModelCompletionRequest { pub run_id: RunId, pub prompt: String, pub model: String }`
  - `pub struct ModelCompletionResponse { pub content: String, pub usage: Option<ModelUsage> }`
  - `pub struct ModelUsage { pub prompt_tokens: Option<u64>, pub completion_tokens: Option<u64>, pub total_tokens: Option<u64> }`
  - `pub enum ProviderWorkerError { MissingConfiguration, InvalidRequest, Http(String), ProviderStatus { status: u16, body: String }, MalformedResponse(String), Timeout }`
  - `#[async_trait] pub trait ProviderWorker: Send + Sync { fn worker_id(&self) -> WorkerId; async fn complete(&self, request: ModelCompletionRequest) -> Result<ModelCompletionResponse, ProviderWorkerError>; }`
  - `pub type SharedProviderWorker = Arc<dyn ProviderWorker>;`
  - `pub struct MockProviderWorker`
  - `pub struct OpenAiCompatibleProviderWorker`
  - `pub struct OpenAiProviderConfig`
  - `pub trait OpenAiHttpClient: Send + Sync`

- [ ] **Step 1: Add failing provider mapping tests**

In `crates/mneme-kernel/src/workers.rs`, add tests:

```rust
#[tokio::test]
async fn mock_provider_returns_configured_completion() {
    let worker = MockProviderWorker::new("assistant reply");
    let response = worker
        .complete(ModelCompletionRequest {
            run_id: RunId::new(),
            prompt: "hello".to_string(),
            model: "mock-model".to_string(),
        })
        .await
        .unwrap();

    assert_eq!(response.content, "assistant reply");
}

#[tokio::test]
async fn openai_provider_maps_chat_completion_response() {
    let http = MockOpenAiHttpClient::new(200, r#"{"choices":[{"message":{"role":"assistant","content":"mapped reply"}}],"usage":{"prompt_tokens":3,"completion_tokens":4,"total_tokens":7}}"#);
    let provider = OpenAiCompatibleProviderWorker::new(
        WorkerId::new(),
        OpenAiProviderConfig {
            base_url: "https://example.test/v1".to_string(),
            api_key: Some("secret-value".to_string()),
            model: "gpt-test".to_string(),
            timeout_secs: 30,
        },
        http.shared(),
    );

    let response = provider
        .complete(ModelCompletionRequest {
            run_id: RunId::new(),
            prompt: "hello".to_string(),
            model: "gpt-test".to_string(),
        })
        .await
        .unwrap();

    assert_eq!(response.content, "mapped reply");
    assert_eq!(response.usage.unwrap().total_tokens, Some(7));
    assert_eq!(http.last_request().unwrap().url, "https://example.test/v1/chat/completions");
    assert!(http.last_request().unwrap().body.contains("\"stream\":false"));
    assert!(http.last_request().unwrap().body.contains("\"content\":\"hello\""));
}

#[tokio::test]
async fn openai_provider_requires_api_key_without_leaking_it() {
    let provider = OpenAiCompatibleProviderWorker::new(
        WorkerId::new(),
        OpenAiProviderConfig {
            base_url: "https://example.test/v1".to_string(),
            api_key: None,
            model: "gpt-test".to_string(),
            timeout_secs: 30,
        },
        MockOpenAiHttpClient::new(200, "{}").shared(),
    );

    let error = provider
        .complete(ModelCompletionRequest {
            run_id: RunId::new(),
            prompt: "hello".to_string(),
            model: "gpt-test".to_string(),
        })
        .await
        .unwrap_err();

    assert_eq!(error, ProviderWorkerError::MissingConfiguration);
    assert!(!error.to_string().contains("secret"));
}
```

- [ ] **Step 2: Run tests to verify RED**

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-kernel provider
```

Expected: fail with missing provider types.

- [ ] **Step 3: Add reqwest dependency and provider types**

Modify root `Cargo.toml`:

```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
```

Modify `crates/mneme-kernel/Cargo.toml`:

```toml
reqwest.workspace = true
```

In `workers.rs`, add the interfaces from this task. `ProviderWorkerError` must derive `Debug`, `Error`, `PartialEq`, `Eq` and must not include the API key in any variant.

- [ ] **Step 4: Implement mockable OpenAI HTTP transport**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiHttpRequest {
    pub url: String,
    pub bearer_token: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiHttpResponse {
    pub status: u16,
    pub body: String,
}

#[async_trait]
pub trait OpenAiHttpClient: Send + Sync {
    async fn post_chat_completions(
        &self,
        request: OpenAiHttpRequest,
        timeout_secs: u64,
    ) -> Result<OpenAiHttpResponse, ProviderWorkerError>;
}

pub type SharedOpenAiHttpClient = Arc<dyn OpenAiHttpClient>;
```

Add `ReqwestOpenAiHttpClient` implementation using `reqwest::Client` and `tokio::time::timeout` or reqwest timeout configuration. The implementation must set bearer auth from `OpenAiHttpRequest.bearer_token` and never log it.

- [ ] **Step 5: Implement `OpenAiCompatibleProviderWorker`**

Build request body with `serde_json::json!`:

```rust
{
    "model": request.model,
    "messages": [{ "role": "user", "content": request.prompt }],
    "stream": false
}
```

Normalize `base_url` by removing trailing `/`; post to `{base_url}/chat/completions`. Parse response JSON and return `MalformedResponse` if `choices[0].message.content` is absent or blank. Return `ProviderStatus { status, body }` for non-2xx HTTP responses.

- [ ] **Step 6: Verify provider tests**

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check
. "$HOME/.cargo/env" && cargo test -p mneme-kernel provider
. "$HOME/.cargo/env" && cargo test -p mneme-kernel
. "$HOME/.cargo/env" && git diff --check
```

Expected: all pass offline.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock crates/mneme-kernel/Cargo.toml crates/mneme-kernel/src/workers.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add openai compatible provider worker"
```

---

### Task 3: Worker job projection stores and SQLite rebuild

**Files:**
- Modify: `crates/mneme-kernel/migrations/0001_operation_ledger.sql`
- Modify: `crates/mneme-kernel/src/workers.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: worker job events and `WorkerJobRecord` from Task 1.
- Produces:
  - `#[async_trait] pub trait WorkerJobProjectionStore: Send + Sync { async fn apply(&self, event: &OperationEvent) -> Result<(), WorkerRuntimeError>; async fn list_worker_jobs(&self) -> Result<Vec<WorkerJobRecord>, WorkerRuntimeError>; async fn rebuild(&self, events: &[OperationEvent]) -> Result<(), WorkerRuntimeError>; }`
  - `pub type SharedWorkerJobProjectionStore = Arc<dyn WorkerJobProjectionStore>;`
  - `pub struct InMemoryWorkerJobProjectionStore`
  - `pub struct SqliteWorkerJobProjectionStore`
  - `pub enum WorkerRuntimeError`

- [ ] **Step 1: Add failing projection tests**

Add tests:

```rust
#[tokio::test]
async fn in_memory_worker_job_projection_tracks_started_completed_failed() {
    let store = InMemoryWorkerJobProjectionStore::new();
    let run_id = RunId::new();
    let worker_id = WorkerId::new();
    let job_id = WorkerJobId::new();

    let started = OperationEvent::new_worker_job_started(
        run_id,
        job_id,
        worker_id,
        WorkerJobKind::ModelCompletion,
        Some("gpt-test".to_string()),
    );
    store.apply(&started).await.unwrap();
    assert_eq!(store.list_worker_jobs().await.unwrap()[0].status, WorkerJobStatus::Started);

    let completed = OperationEvent::new_worker_job_completed(run_id, job_id, worker_id, WorkerJobKind::ModelCompletion);
    store.apply(&completed).await.unwrap();
    let job = &store.list_worker_jobs().await.unwrap()[0];
    assert_eq!(job.status, WorkerJobStatus::Completed);
    assert!(job.completed_at.is_some());
}
```

Add SQLite test `sqlite_ledger_roundtrips_worker_job_events_can_rebuild_projection` that appends worker job events to `SqliteOperationLedger`, calls `SqliteWorkerJobProjectionStore::rebuild(&events)`, then asserts `list_worker_jobs()` returns the job with `Completed` or `Failed` status.

- [ ] **Step 2: Run tests to verify RED**

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-kernel worker_job_projection
. "$HOME/.cargo/env" && cargo test -p mneme-kernel sqlite_ledger_roundtrips_worker_job
```

Expected: fail with missing projection store types or `worker_jobs` table.

- [ ] **Step 3: Add migration table and projection trait**

Modify `crates/mneme-kernel/migrations/0001_operation_ledger.sql`:

```sql
create table if not exists worker_jobs (
  worker_job_id text primary key,
  worker_id text not null,
  run_id text not null,
  kind text not null,
  status text not null,
  model text,
  error text,
  started_at text not null,
  completed_at text
);

create index if not exists idx_worker_jobs_run_id_started_at
  on worker_jobs(run_id, started_at);
```

Implement in-memory projection in `workers.rs`. Started creates/overwrites the row with `Started`; Completed updates status and `completed_at`; Failed updates status, `error`, and `completed_at`. If Completed/Failed arrives before Started, create a minimal row using event time for both `started_at` and `completed_at`.

- [ ] **Step 4: Implement SQLite projection store**

In `storage/sqlite.rs`, add `SqliteWorkerJobProjectionStore`. `apply()` should upsert started jobs and update completed/failed jobs. `rebuild()` should delete from `worker_jobs`, then apply worker job events in ledger order.

- [ ] **Step 5: Verify projection tests**

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check
. "$HOME/.cargo/env" && cargo test -p mneme-kernel worker_job_projection
. "$HOME/.cargo/env" && cargo test -p mneme-kernel sqlite_ledger_roundtrips_worker_job
. "$HOME/.cargo/env" && cargo test -p mneme-kernel
. "$HOME/.cargo/env" && git diff --check
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/mneme-kernel/migrations/0001_operation_ledger.sql crates/mneme-kernel/src/workers.rs crates/mneme-kernel/src/storage/sqlite.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add worker job projection store"
```

---

### Task 4: WorkerRuntimeService success and failure flow

**Files:**
- Modify: `crates/mneme-kernel/src/workers.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: `ProviderWorker`, `WorkerJobProjectionStore`, `RunService`, `OperationLedger`.
- Produces:
  - `pub struct RunModelWithWorkerOutcome { pub run_id: RunId, pub worker_job_id: WorkerJobId }`
  - `#[async_trait] pub trait WorkerRuntimeService: Send + Sync { async fn run_model_completion(&self, run_id: RunId, prompt: String) -> Result<RunModelWithWorkerOutcome, WorkerRuntimeServiceError>; async fn list_worker_jobs(&self) -> Result<Vec<WorkerJobRecord>, WorkerRuntimeServiceError>; }`
  - `pub type SharedWorkerRuntimeService = Arc<dyn WorkerRuntimeService>;`
  - `pub enum WorkerRuntimeServiceError { InvalidInput, UnknownRun, RunAlreadyTerminal, ProviderMissingConfiguration, ProviderBadGateway(String), ProviderTimeout, Ledger(String), Projection(String), RunService(String) }`
  - `pub struct ComposedWorkerRuntimeService`
  - `pub struct SqliteWorkerRuntimeService`

- [ ] **Step 1: Add failing composed runtime tests**

Add tests in `workers.rs`:

```rust
#[tokio::test]
async fn composed_worker_runtime_success_drives_run_lifecycle() {
    let ledger = InMemoryOperationLedger::new().shared();
    let run_store = InMemoryRunProjectionStore::new().shared();
    let run_service = ComposedRunService::new(ledger.clone(), run_store.clone()).shared();
    let run = run_service.create_run("Summarize".to_string()).await.unwrap();
    let worker_jobs = InMemoryWorkerJobProjectionStore::new().shared();
    let provider = MockProviderWorker::new("assistant completion").shared();
    let service = ComposedWorkerRuntimeService::new(
        WorkerId::new(),
        "mock-model".to_string(),
        ledger.clone(),
        run_service.clone(),
        worker_jobs.clone(),
        provider,
    );

    let outcome = service
        .run_model_completion(run.run_id, "Say hi".to_string())
        .await
        .unwrap();

    assert_eq!(outcome.run_id, run.run_id);
    let events = run_service.list_events().await.unwrap();
    assert!(events.iter().any(|event| matches!(event.kind, OperationEventKind::WorkerJobStarted { .. })));
    assert!(events.iter().any(|event| matches!(event.kind, OperationEventKind::RunModelRequested { .. })));
    assert!(events.iter().any(|event| matches!(event.kind, OperationEventKind::RunAssistantMessage { ref content } if content == "assistant completion")));
    assert!(events.iter().any(|event| matches!(event.kind, OperationEventKind::RunCompleted { .. })));
    assert_eq!(service.list_worker_jobs().await.unwrap()[0].status, WorkerJobStatus::Completed);
}

#[tokio::test]
async fn composed_worker_runtime_provider_failure_fails_run() {
    let ledger = InMemoryOperationLedger::new().shared();
    let run_store = InMemoryRunProjectionStore::new().shared();
    let run_service = ComposedRunService::new(ledger.clone(), run_store.clone()).shared();
    let run = run_service.create_run("Summarize".to_string()).await.unwrap();
    let worker_jobs = InMemoryWorkerJobProjectionStore::new().shared();
    let provider = MockProviderWorker::failing(ProviderWorkerError::ProviderStatus {
        status: 500,
        body: "provider failed".to_string(),
    })
    .shared();
    let service = ComposedWorkerRuntimeService::new(
        WorkerId::new(),
        "mock-model".to_string(),
        ledger.clone(),
        run_service.clone(),
        worker_jobs,
        provider,
    );

    let error = service
        .run_model_completion(run.run_id, "Say hi".to_string())
        .await
        .unwrap_err();

    assert!(matches!(error, WorkerRuntimeServiceError::ProviderBadGateway(_)));
    let runs = run_service.list_runs().await.unwrap();
    assert_eq!(runs[0].status, RunStatus::Failed);
    assert!(run_service
        .list_events()
        .await
        .unwrap()
        .iter()
        .any(|event| matches!(event.kind, OperationEventKind::WorkerJobFailed { .. })));
}
```

- [ ] **Step 2: Add failing SQLite runtime test**

Add `sqlite_worker_runtime_success_records_job_and_run_lifecycle` in `storage/sqlite.rs`. Use `SqliteStore::connect("sqlite::memory:")`, `SqliteRunService`, `SqliteWorkerJobProjectionStore`, and `MockProviderWorker`. Assert `list_worker_jobs()` returns a completed job and `SqliteRunService::list_runs()` returns a completed run.

- [ ] **Step 3: Run tests to verify RED**

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-kernel worker_runtime
. "$HOME/.cargo/env" && cargo test -p mneme-kernel sqlite_worker_runtime
```

Expected: fail with missing runtime service types.

- [ ] **Step 4: Implement composed runtime service**

Implement `ComposedWorkerRuntimeService` in `workers.rs`. Validation rules:

- trim prompt; blank prompt returns `InvalidInput` before writing events.
- unknown run maps from `RunServiceError::InvalidRunId` to `UnknownRun`.
- terminal run maps from `RunServiceError::RunAlreadyTerminal` to `RunAlreadyTerminal`.
- provider `MissingConfiguration` maps to `ProviderMissingConfiguration`.
- provider `Timeout` maps to `ProviderTimeout`.
- provider status/http/malformed/invalid request maps to `ProviderBadGateway(String)`.

Ordering:

1. create `WorkerJobId`.
2. append/apply `WorkerJobStarted`.
3. call `run_service.request_model(run_id, prompt.clone())`.
4. call `provider.complete(ModelCompletionRequest { run_id, prompt, model })`.
5. on success append/apply `WorkerJobCompleted`, then `record_assistant_message`, then `complete_run`.
6. on provider failure append/apply `WorkerJobFailed`, then `fail_run` with sanitized error text.

- [ ] **Step 5: Implement SQLite runtime service**

In `storage/sqlite.rs`, add `SqliteWorkerRuntimeService` that composes:

- `SqliteOperationLedger::new(store.clone())`
- `SqliteRunService::new(store.clone())`
- `SqliteWorkerJobProjectionStore::new(store.clone())`
- `SharedProviderWorker`

It may delegate to `ComposedWorkerRuntimeService` only if the ledger/run/projection objects share the same SQLite store. Preserve transaction safety as far as existing service boundaries allow; do not write runs directly.

- [ ] **Step 6: Verify runtime service**

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check
. "$HOME/.cargo/env" && cargo test -p mneme-kernel worker_runtime
. "$HOME/.cargo/env" && cargo test -p mneme-kernel sqlite_worker_runtime
. "$HOME/.cargo/env" && cargo test -p mneme-kernel
. "$HOME/.cargo/env" && git diff --check
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/mneme-kernel/src/workers.rs crates/mneme-kernel/src/storage/sqlite.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add worker runtime service"
```

---

### Task 5: HTTP API and daemon provider wiring

**Files:**
- Modify: `crates/mneme-api/src/types.rs`
- Modify: `crates/mneme-api/src/routes.rs`
- Modify: `crates/mneme-daemon/src/main.rs`

**Interfaces:**
- Consumes: `SharedWorkerRuntimeService`, `ComposedWorkerRuntimeService`, `SqliteWorkerRuntimeService`, `MockProviderWorker`, `OpenAiCompatibleProviderWorker`, `ReqwestOpenAiHttpClient`.
- Produces:
  - `RunWorkerModelRequest { pub prompt: String }`
  - `RunWorkerModelResponse { pub run_id: String, pub worker_job_id: String }`
  - `WorkerJobsResponse { pub worker_jobs: Vec<WorkerJobRecord> }`
  - Routes `POST /api/v1/runs/:run_id/worker-model-completion`, `GET /api/v1/worker-jobs`
  - Daemon env helpers for `MNEME_OPENAI_BASE_URL`, `MNEME_OPENAI_API_KEY`, `MNEME_OPENAI_MODEL`, `MNEME_OPENAI_TIMEOUT_SECS`

- [ ] **Step 1: Add failing API tests**

In `routes.rs`, add tests:

```rust
#[tokio::test]
async fn api_runs_worker_model_completion_with_mock_provider() {
    let app = build_router_with_in_memory_worker_provider(MockProviderWorker::new("api reply").shared());
    let run = create_run_for_test(&app, "worker run").await;

    let response = json_request(
        &app,
        http::Method::POST,
        &format!("/api/v1/runs/{}/worker-model-completion", run.run_id),
        serde_json::json!({ "prompt": "hello" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: RunWorkerModelResponse = response_json(response).await;
    assert_eq!(body.run_id, run.run_id);

    let jobs = empty_request(&app, http::Method::GET, "/api/v1/worker-jobs").await;
    assert_eq!(jobs.status(), StatusCode::OK);
    let body: WorkerJobsResponse = response_json(jobs).await;
    assert_eq!(body.worker_jobs.len(), 1);
}

#[tokio::test]
async fn api_rejects_blank_worker_prompt() {
    let app = build_router_with_in_memory_worker_provider(MockProviderWorker::new("api reply").shared());
    let run = create_run_for_test(&app, "worker run").await;

    let response = json_request(
        &app,
        http::Method::POST,
        &format!("/api/v1/runs/{}/worker-model-completion", run.run_id),
        serde_json::json!({ "prompt": "   " }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn api_maps_missing_provider_config_to_service_unavailable() {
    let app = build_router_with_in_memory_worker_provider(MockProviderWorker::failing(ProviderWorkerError::MissingConfiguration).shared());
    let run = create_run_for_test(&app, "worker run").await;

    let response = json_request(
        &app,
        http::Method::POST,
        &format!("/api/v1/runs/{}/worker-model-completion", run.run_id),
        serde_json::json!({ "prompt": "hello" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
```

Add daemon unit tests for env parsing:

```rust
#[test]
fn openai_provider_config_uses_defaults_without_api_key() {
    let config = openai_provider_config_from_env_values(None, None, None, None);
    assert_eq!(config.base_url, "https://api.openai.com/v1");
    assert_eq!(config.model, "gpt-4o-mini");
    assert_eq!(config.timeout_secs, 30);
    assert!(config.api_key.is_none());
}
```

- [ ] **Step 2: Run tests to verify RED**

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-api worker
. "$HOME/.cargo/env" && cargo test -p mneme-daemon openai_provider_config
```

Expected: fail with missing DTO/routes/env helpers.

- [ ] **Step 3: Add DTOs and route state**

Add DTOs in `types.rs`. Extend `ApiState` in `routes.rs` with `worker_runtime_service: SharedWorkerRuntimeService` or route-local enum if direct trait object export is unavailable. Update all router builders to pass a composed in-memory worker runtime using `MockProviderWorker::new("mock worker completion")` for compatibility.

- [ ] **Step 4: Add API routes and error mapping**

Routes:

```rust
.route(
    "/api/v1/runs/:run_id/worker-model-completion",
    axum::routing::post(run_worker_model_completion),
)
.route("/api/v1/worker-jobs", get(list_worker_jobs))
```

Handlers parse `RunId` with `RunId::from_str`; invalid UUID returns `BAD_REQUEST`. Map errors:

- `InvalidInput` → `BAD_REQUEST`
- `UnknownRun` → `NOT_FOUND`
- `RunAlreadyTerminal` → `BAD_REQUEST`
- `ProviderMissingConfiguration` → `SERVICE_UNAVAILABLE`
- `ProviderTimeout` → `GATEWAY_TIMEOUT`
- `ProviderBadGateway(_)` → `BAD_GATEWAY`
- ledger/projection/run-service storage → `INTERNAL_SERVER_ERROR`

- [ ] **Step 5: Wire daemon real provider**

In `crates/mneme-daemon/src/main.rs`, add env parsing functions:

```rust
fn openai_provider_config_from_env() -> OpenAiProviderConfig
fn openai_provider_config_from_env_values(
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    timeout_secs: Option<String>,
) -> OpenAiProviderConfig
```

Defaults:

- base URL: `https://api.openai.com/v1`
- model: `gpt-4o-mini`
- timeout: `30`
- API key: `None` if env missing or blank

Build:

```rust
let provider = OpenAiCompatibleProviderWorker::new(
    WorkerId::new(),
    openai_provider_config_from_env(),
    ReqwestOpenAiHttpClient::new().shared(),
)
.shared();
let worker_runtime_service = SqliteWorkerRuntimeService::new(
    store.clone(),
    WorkerId::new(),
    openai_model,
    provider,
);
```

Do not log API key. The existing daemon `info!` line must not include provider config except possibly model/base URL without key.

- [ ] **Step 6: Verify API and daemon**

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check
. "$HOME/.cargo/env" && cargo test -p mneme-api worker
. "$HOME/.cargo/env" && cargo test -p mneme-daemon openai_provider_config
. "$HOME/.cargo/env" && cargo test -p mneme-api
. "$HOME/.cargo/env" && cargo test --workspace
. "$HOME/.cargo/env" && git diff --check
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/mneme-api/src/types.rs crates/mneme-api/src/routes.rs crates/mneme-daemon/src/main.rs
git commit -m "feat: expose worker model completion API"
```

---

### Task 6: TypeScript API client worker methods

**Files:**
- Modify: `apps/desktop/src/api/client.ts`
- Modify: `apps/desktop/src/api/client.test.ts`
- Modify: `apps/desktop/src/app/App.test.tsx`

**Interfaces:**
- Consumes: Task 5 API DTO shape.
- Produces:
  - `RunWorkerModelRequest`
  - `RunWorkerModelResponse`
  - `WorkerJobKind`, `WorkerJobStatus`, `WorkerJobRecord`, `WorkerJobsResponse`
  - `MnemeApiClient.runWorkerModelCompletion(runId: string, request: RunWorkerModelRequest): Promise<RunWorkerModelResponse>`
  - `MnemeApiClient.listWorkerJobs(): Promise<WorkerJobsResponse>`

- [ ] **Step 1: Add failing client tests**

In `client.test.ts`, add:

```ts
it('runs worker model completion through the API', async () => {
  const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ run_id: 'run-1', worker_job_id: 'job-1' }));
  const client = createMnemeApiClient('http://localhost:3766/', fetchMock);

  await expect(
    client.runWorkerModelCompletion('run-1', { prompt: 'hello worker' }),
  ).resolves.toEqual({ run_id: 'run-1', worker_job_id: 'job-1' });

  expect(fetchMock).toHaveBeenCalledWith(
    'http://localhost:3766/api/v1/runs/run-1/worker-model-completion',
    expect.objectContaining({
      method: 'POST',
      body: JSON.stringify({ prompt: 'hello worker' }),
    }),
  );
});

it('lists worker jobs through the API', async () => {
  const worker_jobs = [
    {
      worker_job_id: 'job-1',
      worker_id: 'worker-1',
      run_id: 'run-1',
      kind: 'model_completion',
      status: 'completed',
      model: 'gpt-test',
      error: null,
      started_at: '2026-01-01T00:00:00Z',
      completed_at: '2026-01-01T00:00:01Z',
    },
  ];
  const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ worker_jobs }));
  const client = createMnemeApiClient('http://localhost:3766', fetchMock);

  await expect(client.listWorkerJobs()).resolves.toEqual({ worker_jobs });
  expect(fetchMock).toHaveBeenCalledWith(
    'http://localhost:3766/api/v1/worker-jobs',
    expect.objectContaining({ headers: { accept: 'application/json' } }),
  );
});
```

Update `App.test.tsx` fake client type to include the two new non-optional methods.

- [ ] **Step 2: Run tests to verify RED**

```bash
pnpm --dir apps/desktop test -- src/api/client.test.ts
```

Expected: fail with `client.runWorkerModelCompletion is not a function` or missing fake methods.

- [ ] **Step 3: Implement DTOs and client methods**

Add the interfaces to `client.ts` and extend `MnemeApiClient`. Implement:

```ts
async runWorkerModelCompletion(runId: string, request: RunWorkerModelRequest) {
  const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/runs/${runId}/worker-model-completion`, {
    method: 'POST',
    headers: { accept: 'application/json', 'content-type': 'application/json' },
    body: JSON.stringify(request),
  });
  return parseJsonResponse<RunWorkerModelResponse>(response);
},

async listWorkerJobs() {
  const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/worker-jobs`, {
    headers: { accept: 'application/json' },
  });
  return parseJsonResponse<WorkerJobsResponse>(response);
},
```

- [ ] **Step 4: Verify desktop client**

```bash
pnpm --dir apps/desktop check
pnpm --dir apps/desktop test -- src/api/client.test.ts
. "$HOME/.cargo/env" && pnpm check
. "$HOME/.cargo/env" && git diff --check
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/api/client.ts apps/desktop/src/api/client.test.ts apps/desktop/src/app/App.test.tsx
git commit -m "feat: add worker runtime API client"
```

---

### Task 7: Workbench worker jobs panel

**Files:**
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Modify: `apps/desktop/src/styles.css`

**Interfaces:**
- Consumes: `MnemeApiClient.listWorkerJobs()` and `WorkerJobRecord` from Task 6.
- Produces: Read-only Workbench section `Worker Runtime` listing worker jobs.

- [ ] **Step 1: Add failing UI tests**

In `App.test.tsx`, extend the fake client worker jobs response and add assertions:

```ts
it('shows worker runtime jobs without claiming streaming or tool execution', async () => {
  const client = createFakeClient({
    workerJobs: {
      worker_jobs: [
        {
          worker_job_id: 'job-1',
          worker_id: 'worker-1',
          run_id: 'run-1',
          kind: 'model_completion',
          status: 'completed',
          model: 'gpt-test',
          error: null,
          started_at: '2026-01-01T00:00:00Z',
          completed_at: '2026-01-01T00:00:01Z',
        },
        {
          worker_job_id: 'job-2',
          worker_id: 'worker-1',
          run_id: 'run-2',
          kind: 'model_completion',
          status: 'failed',
          model: 'gpt-test',
          error: 'provider returned 500',
          started_at: '2026-01-01T00:00:02Z',
          completed_at: '2026-01-01T00:00:03Z',
        },
      ],
    },
  });

  render(<App client={client} />);

  expect(await screen.findByText('Worker Runtime')).toBeInTheDocument();
  expect(screen.getByText('model_completion')).toBeInTheDocument();
  expect(screen.getByText('completed')).toBeInTheDocument();
  expect(screen.getByText('failed')).toBeInTheDocument();
  expect(screen.getByText('provider returned 500')).toBeInTheDocument();
  expect(screen.getByText('Provider keys are configured outside the Workbench. No streaming token UI or tool execution is available in Milestone 7.')).toBeInTheDocument();
  expect(client.runWorkerModelCompletion).not.toHaveBeenCalled();
});
```

- [ ] **Step 2: Run test to verify RED**

```bash
pnpm --dir apps/desktop test -- src/app/App.test.tsx
```

Expected: fail with missing `Worker Runtime`.

- [ ] **Step 3: Implement read-only panel**

In `App.tsx`:

- import `WorkerJobsResponse`.
- add state `const [workerJobs, setWorkerJobs] = useState<WorkerJobsResponse>({ worker_jobs: [] });`
- include `client.listWorkerJobs()` in the `Promise.all` load call.
- render a section after `Retrieval Traces`:

```tsx
<section className="panel">
  <h2>Worker Runtime</h2>
  <p className="muted">Provider keys are configured outside the Workbench. No streaming token UI or tool execution is available in Milestone 7.</p>
  {workerJobs.worker_jobs.length === 0 ? (
    <p className="muted">No worker jobs have been recorded yet.</p>
  ) : (
    <ul className="worker-job-list">
      {workerJobs.worker_jobs.map((job) => (
        <li key={job.worker_job_id} className="worker-job-card">
          <div className="artifact-card-header">
            <strong>{job.kind}</strong>
            <span className="artifact-kind">{job.status}</span>
          </div>
          <p>run {job.run_id}</p>
          {job.model ? <p>model {job.model}</p> : null}
          {job.error ? <p className="run-error">{job.error}</p> : null}
          <small>{job.started_at}{job.completed_at ? ` → ${job.completed_at}` : ''}</small>
        </li>
      ))}
    </ul>
  )}
</section>
```

Do not add a button or form in M7 UI. API/client provides the trigger; Workbench only displays jobs.

- [ ] **Step 4: Style panel**

In `styles.css`, add `.worker-job-list` and `.worker-job-card` using the existing card/list style language. Do not introduce a new design system.

- [ ] **Step 5: Verify UI**

```bash
pnpm --dir apps/desktop check
pnpm --dir apps/desktop test -- src/app/App.test.tsx
. "$HOME/.cargo/env" && pnpm check
. "$HOME/.cargo/env" && git diff --check
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/app/App.tsx apps/desktop/src/app/App.test.tsx apps/desktop/src/styles.css
git commit -m "feat: show worker runtime jobs"
```

---

### Task 8: Milestone 7 documentation

**Files:**
- Modify: `README.md`
- Create: `docs/architecture/milestone-7-worker-runtime-provider.md`
- Modify: `docs/architecture/phase-1-foundation.md`

**Interfaces:**
- Consumes: final M7 architecture and API shape.
- Produces: user-facing and architecture documentation.

- [ ] **Step 1: Draft docs with required sections**

Create `docs/architecture/milestone-7-worker-runtime-provider.md` with these sections:

```markdown
# Milestone 7: Worker Runtime Provider

## Scope

## Event Flow

## Provider Configuration

## Worker Runtime Boundary

## HTTP API

## Workbench

## Run-scoping Rule

## Non-goals

## Verification
```

Required statements:

- M7 uses real OpenAI-compatible non-streaming chat completion when configured.
- Tests default to mock provider and do not require external network or real API key.
- `MNEME_OPENAI_API_KEY` is never written to ledger or projection.
- Worker runtime writes run lifecycle only via `RunService`.
- Workbench does not provide streaming token UI or tool execution.
- Worker-driven retrieval/recall defaults to `run_id` scope; global recall must be explicit in a future milestone.
- M8 owns desktop-native provider config and process management.

- [ ] **Step 2: Update README with curl examples**

Add examples:

```bash
curl -s -X POST http://127.0.0.1:3766/api/v1/runs/<run-id>/worker-model-completion \
  -H 'content-type: application/json' \
  -d '{"prompt":"Summarize this run"}'

curl -s http://127.0.0.1:3766/api/v1/worker-jobs
```

Use literal `<run-id>` only. Do not include real provider keys.

- [ ] **Step 3: Update phase doc**

In `docs/architecture/phase-1-foundation.md`, add that M7 adds a real OpenAI-compatible provider worker MVP, worker job ledger/projection, worker API/client/UI, and M8 owns Tauri desktop packaging/provider config UI/process management.

- [ ] **Step 4: Verify docs**

Run:

```bash
git diff --check
python3 - <<'PY'
from pathlib import Path
paths=[Path('README.md'), *Path('docs/architecture').glob('*.md')]
needles=['TB'+'D','TO'+'DO','place'+'holder','FIX'+'ME','?'+'?']
hits=[]
for path in paths:
    text=path.read_text()
    for needle in needles:
        if needle in text:
            hits.append((str(path), needle))
print('docs scan hits:', hits)
raise SystemExit(1 if hits else 0)
PY
. "$HOME/.cargo/env" && pnpm check
```

Expected: all pass and docs scan prints `docs scan hits: []`.

- [ ] **Step 5: Commit**

```bash
git add README.md docs/architecture/milestone-7-worker-runtime-provider.md docs/architecture/phase-1-foundation.md
git commit -m "docs: document worker runtime provider milestone"
```

---

### Task 9: Final verification and broad review

**Files:**
- Read: all changed files in range `af28d0f..HEAD`
- Write: `.superpowers/sdd/progress.md` ledger line only after broad review approval

**Interfaces:**
- Consumes: all M7 tasks.
- Produces: final verified M7 milestone.

- [ ] **Step 1: Run final verification**

M7 base commit is `af28d0f`.

Run:

```bash
. "$HOME/.cargo/env" && git status --short --branch && git log --oneline af28d0f..HEAD && pnpm check && git diff --check && git diff --stat af28d0f..HEAD && git status --short --branch
```

Expected:

- status clean before and after;
- `pnpm check` passes all Rust workspace and desktop tests;
- `git diff --check` passes;
- diff stat includes kernel worker runtime, API/daemon, desktop client/UI, docs.

- [ ] **Step 2: Generate broad review package**

```bash
bash /Users/baiye/.agents/skills/subagent-driven-development/scripts/review-package af28d0f HEAD .superpowers/sdd/review-af28d0f..HEAD.diff
```

Dispatch a reviewer with this checklist:

- Does M7 include real OpenAI-compatible request/response mapping?
- Do tests avoid real network and real API keys?
- Is API key excluded from logs/ledger/projection/docs/tests?
- Are worker job started/completed/failed events ledgered and projection rebuildable?
- Does runtime service use `RunService` lifecycle methods rather than direct runs projection writes?
- Do provider failures record worker job failure and fail the run?
- Are API status mappings correct?
- Do TS client and Workbench avoid streaming/tool execution claims?
- Do docs state run-scoping rule and M8 handoff?

- [ ] **Step 3: Resolve review findings**

If reviewer returns Critical or Important, use `receiving-code-review`, verify each finding in files/tests, fix in a new commit or amend the relevant task commit according to SDD direction, rerun focused and final verification, then request re-review.

If reviewer approves with only minor follow-ups, append the M7 completion ledger line.

- [ ] **Step 4: Append final progress ledger line**

Append to `.superpowers/sdd/progress.md`:

```text
Milestone 7: complete (commits af28d0f..HEAD, final verification passed, overall review approved; follow-ups for Milestone 8: move provider configuration and worker process lifecycle into Tauri desktop packaging without changing M7 ledger facts)
```

- [ ] **Step 5: Report next step**

Report that M7 is complete and Milestone 8 is next. Do not mark the overall goal complete because Milestones 8 and 9 remain.

---

## Self-Review Notes

- Spec coverage: Tasks 1-4 cover Kernel worker IDs/events/provider/runtime/projection and RunService integration. Task 5 covers API and daemon provider env wiring. Task 6 covers TS client. Task 7 covers Workbench read-only job display. Task 8 covers docs and run-scoping/M8 handoff. Task 9 covers final verification and review.
- Type consistency: `WorkerId`, `WorkerJobId`, `WorkerJobKind`, `WorkerJobStatus`, `WorkerJobRecord`, `ProviderWorker`, `OpenAiCompatibleProviderWorker`, `MockProviderWorker`, `WorkerRuntimeService`, and API/TS DTO names match across tasks.
- Scope check: Plan keeps M7 to non-streaming provider worker MVP; streaming UI, tool execution, worker process management, embedding/document/OCR workers, and Tauri provider config UI stay out of scope.
