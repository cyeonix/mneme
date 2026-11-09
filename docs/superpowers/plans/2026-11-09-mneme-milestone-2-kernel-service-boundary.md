# Mneme Milestone 2 Kernel 服务边界 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把“创建 run、追加 OperationEvent、更新/修复 runs 投影、查询 run 数据”收敛到 Kernel service 层，让 HTTP API、daemon 和未来 typed RPC/event bus 共享同一业务入口。

**Architecture:** Milestone 2 在 Milestone 1 的 SQLite 数据面之上新增 `RunService` 边界。API 不再直接编排 ledger 与 projection store；兼容测试路径使用组合式 service，daemon 默认使用 SQLite 专用 service，在同一个 SQL transaction 中完成 run-started event append 与 runs projection upsert。projection repair 通过 service 暴露给 daemon 启动选项，作为从事实源恢复读模型的第一条内部运维路径。

**Tech Stack:** Rust 2021, async-trait, Axum 0.7, Tokio 1.41, SQLx 0.8 + SQLite, Serde/serde_json, time, uuid, React + Vite + Vitest + TypeScript。

## Global Constraints

- 产品方向：Mneme = Personal AI Kernel（个人 AI 内核）。
- 当前运行形态：Desktop App + Local Daemon/API；不要引入云端依赖。
- Kernel 路线：Rust-first Kernel + TypeScript UI。
- 数据路线：append-only Operation Ledger 是事实源；SQLite 投影只作为可重建读模型。
- 服务边界路线：API、daemon、未来 typed RPC/event bus 都应调用 Kernel service，不直接拼业务写入流程。
- API 兼容：保留 `GET /api/v1/health`、`GET /api/v1/events`、`GET /api/v1/runs`、`POST /api/v1/runs` 的 HTTP shape。
- SQLite daemon 写入一致性：daemon 默认路径中的 run 创建必须通过 SQLite service 的单事务写入完成 event append + runs projection upsert。
- projection repair：必须保留从 `operation_events` rebuild `runs` 的 service 入口；daemon 可通过显式环境变量触发启动修复。
- 验证命令：每个任务结束前至少运行该任务指定测试；整阶段最终运行 `. "$HOME/.cargo/env" && pnpm check` 和 `git diff --check`。
- 提交节奏：每个任务用一个 git commit 结束，commit message 使用 Conventional Commits。
- 本计划只做 Milestone 2：Kernel service boundary、SQLite transactional create-run、projection repair hook、相关文档；不实现模型执行、Agent run loop、工具审批、Tauri 打包、自托管服务。

---

## Route Overview

Milestone 1 已完成持久化 Operation Ledger 与 runs projection。Milestone 2 只把业务写入入口收敛到 Kernel service：

```text
HTTP / typed RPC / daemon adapters
  → SharedRunService
  → Kernel service implementation
  → Operation Ledger + RunProjectionStore
  → SQLite operation_events / runs
```

完成后，后续 Milestone 3 的 Agent run loop 只需要调用 service 追加 run 生命周期事件，而不需要理解 SQL、projection 或 HTTP route 细节。

## File Structure

- `crates/mneme-kernel/src/service.rs`：新增 service trait、共享类型、组合式 service、service errors/outcomes。
- `crates/mneme-kernel/src/lib.rs`：导出 service 类型。
- `crates/mneme-kernel/src/storage/mod.rs`：统一导出 SQLite 类型，包含 `SqliteRunProjectionStore` 与 `SqliteRunService`。
- `crates/mneme-kernel/src/storage/sqlite.rs`：新增 SQLite transactional run service，实现同事务 append event + update projection，并提供 rebuild。
- `crates/mneme-api/src/routes.rs`：API state 改为 `SharedRunService`，route 调 service，不直接调用 ledger/store。
- `crates/mneme-api/src/types.rs`：保持 HTTP DTO shape，必要时只补 service error 映射测试。
- `crates/mneme-daemon/src/main.rs`：daemon 使用 `SqliteRunService`，增加 `MNEME_REBUILD_RUNS_ON_START=1` 启动修复选项。
- `README.md`：记录 service boundary、启动修复选项和验证方式。
- `docs/architecture/milestone-2-kernel-service-boundary.md`：记录服务边界、事务写入与 repair 规则。
- `docs/architecture/milestone-1-persistent-ledger.md`：补充 Milestone 2 已接管写入编排，Milestone 1 数据面继续保留。

---

### Task 1: 新增 Kernel RunService 组合式边界

**Files:**
- Create: `crates/mneme-kernel/src/service.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: `SharedOperationLedger`, `SharedRunProjectionStore`, `OperationEvent`, `RunProjection`, `RunId`。
- Produces: `pub type SharedRunService = Arc<dyn RunService>`；`#[async_trait] pub trait RunService`；`ComposedRunService`；`CreateRunOutcome`；`RunServiceError`。

- [ ] **Step 1: Write failing service tests**

Create `crates/mneme-kernel/src/service.rs` with the tests first:

```rust
use crate::{
    InMemoryOperationLedger, InMemoryRunProjectionStore, OperationLedger, RunProjectionStore,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn composed_service_creates_run_event_and_projection() {
        let ledger = InMemoryOperationLedger::new();
        let run_store = InMemoryRunProjectionStore::new();
        let service = ComposedRunService::new(ledger.clone().shared(), run_store.clone().shared());

        let outcome = service.create_run("整理服务边界".to_string()).await.unwrap();

        assert_eq!(ledger.list().await.unwrap().len(), 1);
        assert_eq!(run_store.list_runs().await.unwrap().len(), 1);
        assert_eq!(outcome.run_id, run_store.list_runs().await.unwrap()[0].run_id);
    }

    #[tokio::test]
    async fn composed_service_rejects_blank_instruction() {
        let service = ComposedRunService::new(
            InMemoryOperationLedger::new().shared(),
            InMemoryRunProjectionStore::new().shared(),
        );

        let error = service.create_run("   ".to_string()).await.unwrap_err();

        assert_eq!(error, RunServiceError::InvalidInstruction);
    }

    #[tokio::test]
    async fn composed_service_rebuilds_run_projection_from_events() {
        let ledger = InMemoryOperationLedger::new();
        let run_store = InMemoryRunProjectionStore::new();
        let service = ComposedRunService::new(ledger.clone().shared(), run_store.clone().shared());

        let outcome = service.create_run("需要重建的 run".to_string()).await.unwrap();
        run_store.rebuild(&[]).await.unwrap();
        assert!(run_store.list_runs().await.unwrap().is_empty());

        let rebuilt = service.rebuild_run_projections().await.unwrap();

        assert_eq!(rebuilt.len(), 1);
        assert_eq!(rebuilt[0].run_id, outcome.run_id);
    }
}
```

Add `pub mod service;` to `crates/mneme-kernel/src/lib.rs` only after the test exists, so the first compile proves the service is missing.

- [ ] **Step 2: Run test to verify it fails**

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-kernel service`

Expected: FAIL with missing `ComposedRunService`, `RunService`, `CreateRunOutcome`, or `RunServiceError` symbols.

- [ ] **Step 3: Implement service types**

Replace the top of `crates/mneme-kernel/src/service.rs` with this implementation while keeping the tests below it:

```rust
use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use crate::{
    OperationEvent, OperationLedger, RunId, RunProjection, RunProjectionStore,
    SharedOperationLedger, SharedRunProjectionStore,
};

pub type SharedRunService = Arc<dyn RunService>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateRunOutcome {
    pub run_id: RunId,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RunServiceError {
    #[error("run instruction must not be blank")]
    InvalidInstruction,
    #[error("operation ledger failed: {0}")]
    Ledger(String),
    #[error("run projection failed: {0}")]
    Projection(String),
}

#[async_trait]
pub trait RunService: Send + Sync {
    async fn create_run(&self, instruction: String) -> Result<CreateRunOutcome, RunServiceError>;
    async fn list_events(&self) -> Result<Vec<OperationEvent>, RunServiceError>;
    async fn list_runs(&self) -> Result<Vec<RunProjection>, RunServiceError>;
    async fn rebuild_run_projections(&self) -> Result<Vec<RunProjection>, RunServiceError>;
}

#[derive(Clone)]
pub struct ComposedRunService {
    ledger: SharedOperationLedger,
    run_store: SharedRunProjectionStore,
}

impl ComposedRunService {
    pub fn new(ledger: SharedOperationLedger, run_store: SharedRunProjectionStore) -> Self {
        Self { ledger, run_store }
    }

    pub fn shared(self) -> SharedRunService {
        Arc::new(self)
    }
}

#[async_trait]
impl RunService for ComposedRunService {
    async fn create_run(&self, instruction: String) -> Result<CreateRunOutcome, RunServiceError> {
        let instruction = instruction.trim().to_string();
        if instruction.is_empty() {
            return Err(RunServiceError::InvalidInstruction);
        }

        let run_id = RunId::new();
        let event = OperationEvent::new_run_started(run_id, instruction);
        self.ledger
            .append(event.clone())
            .await
            .map_err(|error| RunServiceError::Ledger(error.to_string()))?;
        self.run_store
            .apply(&event)
            .await
            .map_err(|error| RunServiceError::Projection(error.to_string()))?;

        Ok(CreateRunOutcome { run_id })
    }

    async fn list_events(&self) -> Result<Vec<OperationEvent>, RunServiceError> {
        self.ledger
            .list()
            .await
            .map_err(|error| RunServiceError::Ledger(error.to_string()))
    }

    async fn list_runs(&self) -> Result<Vec<RunProjection>, RunServiceError> {
        self.run_store
            .list_runs()
            .await
            .map_err(|error| RunServiceError::Projection(error.to_string()))
    }

    async fn rebuild_run_projections(&self) -> Result<Vec<RunProjection>, RunServiceError> {
        let events = self.list_events().await?;
        self.run_store
            .rebuild(&events)
            .await
            .map_err(|error| RunServiceError::Projection(error.to_string()))?;
        self.list_runs().await
    }
}
```

- [ ] **Step 4: Export service types**

In `crates/mneme-kernel/src/lib.rs`, add:

```rust
pub mod service;
pub use service::{ComposedRunService, CreateRunOutcome, RunService, RunServiceError, SharedRunService};
```

Keep existing exports intact.

- [ ] **Step 5: Run verification**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-kernel service && cargo test -p mneme-kernel
```

Expected: PASS. Kernel test count increases by the three service tests.

- [ ] **Step 6: Commit**

```bash
git add crates/mneme-kernel/src/service.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add kernel run service boundary"
```

---

### Task 2: 让 HTTP API 调用 RunService

**Files:**
- Modify: `crates/mneme-api/src/routes.rs`

**Interfaces:**
- Consumes: `SharedRunService`, `ComposedRunService`, `RunServiceError`。
- Produces: API state with one service dependency; compatibility constructors keep old public signatures.

- [ ] **Step 1: Write failing API test for service validation**

In `crates/mneme-api/src/routes.rs`, update or add this test:

```rust
#[tokio::test]
async fn create_run_rejects_blank_instruction_through_service() {
    let app = build_router(InMemoryOperationLedger::new());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "instruction": "   " }).to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
```

Then change the test module import expectation to use `build_router_with_service` in at least one test:

```rust
let service = ComposedRunService::new(ledger.shared(), run_store.shared()).shared();
let app = build_router_with_service(service);
```

- [ ] **Step 2: Run test to verify it fails**

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-api runs -- --nocapture`

Expected: FAIL until `build_router_with_service` exists and route code maps service errors.

- [ ] **Step 3: Replace API state with service**

In `crates/mneme-api/src/routes.rs`, change imports and state to:

```rust
use mneme_kernel::{
    kernel_health, ComposedRunService, InMemoryOperationLedger, InMemoryRunProjectionStore,
    RunServiceError, SharedOperationLedger, SharedRunProjectionStore, SharedRunService,
};

#[derive(Clone)]
pub struct ApiState {
    pub run_service: SharedRunService,
}
```

Change constructors to:

```rust
pub fn build_router(ledger: InMemoryOperationLedger) -> Router {
    build_router_with_ledger(ledger.shared())
}

pub fn build_router_with_ledger(ledger: SharedOperationLedger) -> Router {
    build_router_with_state(ledger, InMemoryRunProjectionStore::new().shared())
}

pub fn build_router_with_state(
    ledger: SharedOperationLedger,
    run_store: SharedRunProjectionStore,
) -> Router {
    build_router_with_service(ComposedRunService::new(ledger, run_store).shared())
}

pub fn build_router_with_service(run_service: SharedRunService) -> Router {
    Router::new()
        .route("/api/v1/health", get(get_health))
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/runs", get(list_runs).post(create_run))
        .with_state(ApiState { run_service })
}
```

Change handlers to call service:

```rust
async fn list_events(State(state): State<ApiState>) -> Result<Json<EventsResponse>, StatusCode> {
    let events = state.run_service.list_events().await.map_err(service_error_to_status)?;
    Ok(Json(EventsResponse { events }))
}

async fn list_runs(State(state): State<ApiState>) -> Result<Json<RunsResponse>, StatusCode> {
    let runs = state.run_service.list_runs().await.map_err(service_error_to_status)?;
    Ok(Json(RunsResponse { runs }))
}

async fn create_run(
    State(state): State<ApiState>,
    Json(request): Json<CreateRunRequest>,
) -> Result<Json<CreateRunResponse>, StatusCode> {
    let outcome = state
        .run_service
        .create_run(request.instruction)
        .await
        .map_err(service_error_to_status)?;

    Ok(Json(CreateRunResponse {
        run_id: outcome.run_id.to_string(),
    }))
}

fn service_error_to_status(error: RunServiceError) -> StatusCode {
    match error {
        RunServiceError::InvalidInstruction => StatusCode::BAD_REQUEST,
        RunServiceError::Ledger(_) | RunServiceError::Projection(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
```

- [ ] **Step 4: Update tests to keep existing assertions**

Ensure existing tests still verify:

```rust
assert_eq!(ledger.list().await.expect("events").len(), 1);
assert_eq!(payload.runs.len(), 1);
assert_eq!(payload.runs[0].instruction, "展示 runs");
```

The tests should pass through API → service → in-memory stores.

- [ ] **Step 5: Run verification**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-api && cargo test --workspace
```

Expected: PASS. API route behavior and HTTP shape remain unchanged.

- [ ] **Step 6: Commit**

```bash
git add crates/mneme-api/src/routes.rs
git commit -m "refactor: route API writes through run service"
```

---

### Task 3: 新增 SQLite transactional RunService

**Files:**
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify: `crates/mneme-kernel/src/storage/mod.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: `RunService`, `CreateRunOutcome`, `RunServiceError`, `SqliteStore`。
- Produces: `SqliteRunService::new(store: SqliteStore) -> Self`；`SqliteRunService::shared(self) -> SharedRunService`。

- [ ] **Step 1: Write failing SQLite service tests**

In `crates/mneme-kernel/src/storage/sqlite.rs` test module, add:

```rust
#[tokio::test]
async fn sqlite_run_service_creates_event_and_projection_in_one_service_call() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let service = SqliteRunService::new(store.clone());

    let outcome = service.create_run("事务创建 run".to_string()).await.unwrap();

    let events = SqliteOperationLedger::new(store.clone()).list().await.unwrap();
    let runs = SqliteRunProjectionStore::new(store).list_runs().await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(runs.len(), 1);
    assert_eq!(events[0].run_id, outcome.run_id);
    assert_eq!(runs[0].run_id, outcome.run_id);
}

#[tokio::test]
async fn sqlite_run_service_rebuild_repairs_missing_projection() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let ledger = SqliteOperationLedger::new(store.clone());
    let run_store = SqliteRunProjectionStore::new(store.clone());
    let service = SqliteRunService::new(store);
    let run_id = RunId::new();
    let event = OperationEvent::new_run_started(run_id, "只写入事实源");
    ledger.append(event).await.unwrap();
    assert!(run_store.list_runs().await.unwrap().is_empty());

    let rebuilt = service.rebuild_run_projections().await.unwrap();

    assert_eq!(rebuilt.len(), 1);
    assert_eq!(rebuilt[0].run_id, run_id);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-kernel sqlite_run_service -- --nocapture`

Expected: FAIL with missing `SqliteRunService` or missing `RunService` implementation.

- [ ] **Step 3: Implement SqliteRunService**

In `crates/mneme-kernel/src/storage/sqlite.rs`, add imports:

```rust
use crate::{CreateRunOutcome, RunService, RunServiceError, SharedRunService};
```

Add the type near `SqliteRunProjectionStore`:

```rust
#[derive(Clone, Debug)]
pub struct SqliteRunService {
    store: SqliteStore,
}

impl SqliteRunService {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub fn shared(self) -> SharedRunService {
        Arc::new(self)
    }
}
```

Implement service with transactional create-run:

```rust
#[async_trait]
impl RunService for SqliteRunService {
    async fn create_run(&self, instruction: String) -> Result<CreateRunOutcome, RunServiceError> {
        let instruction = instruction.trim().to_string();
        if instruction.is_empty() {
            return Err(RunServiceError::InvalidInstruction);
        }

        let run_id = RunId::new();
        let event = OperationEvent::new_run_started(run_id, instruction.clone());
        let occurred_at = event
            .occurred_at
            .format(&Rfc3339)
            .map_err(|error| RunServiceError::Ledger(error.to_string()))?;
        let payload_json = serde_json::to_string(&event.kind)
            .map_err(|error| RunServiceError::Ledger(error.to_string()))?;

        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| RunServiceError::Ledger(error.to_string()))?;

        sqlx::query(
            "insert into operation_events (event_id, run_id, occurred_at, kind_type, payload_json) values (?1, ?2, ?3, 'run_started', ?4)",
        )
        .bind(event.id.to_string())
        .bind(run_id.to_string())
        .bind(&occurred_at)
        .bind(payload_json)
        .execute(&mut *tx)
        .await
        .map_err(|error| RunServiceError::Ledger(error.to_string()))?;

        sqlx::query(
            "insert into runs (run_id, status, instruction, started_at, completed_at, failed_at, summary, error, updated_at) values (?1, 'running', ?2, ?3, null, null, null, null, ?3) on conflict(run_id) do update set status = excluded.status, instruction = excluded.instruction, started_at = excluded.started_at, completed_at = excluded.completed_at, failed_at = excluded.failed_at, summary = excluded.summary, error = excluded.error, updated_at = excluded.updated_at",
        )
        .bind(run_id.to_string())
        .bind(instruction)
        .bind(&occurred_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| RunServiceError::Projection(error.to_string()))?;

        tx.commit()
            .await
            .map_err(|error| RunServiceError::Ledger(error.to_string()))?;

        Ok(CreateRunOutcome { run_id })
    }

    async fn list_events(&self) -> Result<Vec<OperationEvent>, RunServiceError> {
        SqliteOperationLedger::new(self.store.clone())
            .list()
            .await
            .map_err(|error| RunServiceError::Ledger(error.to_string()))
    }

    async fn list_runs(&self) -> Result<Vec<RunProjection>, RunServiceError> {
        SqliteRunProjectionStore::new(self.store.clone())
            .list_runs()
            .await
            .map_err(|error| RunServiceError::Projection(error.to_string()))
    }

    async fn rebuild_run_projections(&self) -> Result<Vec<RunProjection>, RunServiceError> {
        let events = self.list_events().await?;
        SqliteRunProjectionStore::new(self.store.clone())
            .rebuild(&events)
            .await
            .map_err(|error| RunServiceError::Projection(error.to_string()))?;
        self.list_runs().await
    }
}
```

- [ ] **Step 4: Export SQLite service uniformly**

In `crates/mneme-kernel/src/storage/mod.rs`, export:

```rust
pub mod sqlite;

pub use sqlite::{
    SqliteOperationLedger, SqliteRunProjectionStore, SqliteRunService, SqliteStore,
    SqliteStoreError,
};
```

In `crates/mneme-kernel/src/lib.rs`, replace the asymmetric SQLite exports with:

```rust
pub use storage::{
    SqliteOperationLedger, SqliteRunProjectionStore, SqliteRunService, SqliteStore,
    SqliteStoreError,
};
```

- [ ] **Step 5: Run verification**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-kernel sqlite_run_service && cargo test -p mneme-kernel
```

Expected: PASS. `sqlite_run_service_creates_event_and_projection_in_one_service_call` proves API/daemon can use a single service call for both writes.

- [ ] **Step 6: Commit**

```bash
git add crates/mneme-kernel/src/storage/sqlite.rs crates/mneme-kernel/src/storage/mod.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add SQLite transactional run service"
```

---

### Task 4: 让 daemon 注入 SQLite RunService 并支持启动 repair

**Files:**
- Modify: `crates/mneme-daemon/src/main.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: `SqliteRunService`, `mneme_api::routes::build_router_with_service`。
- Produces: `MNEME_REBUILD_RUNS_ON_START=1` explicit repair hook。

- [ ] **Step 1: Write failing daemon tests**

In `crates/mneme-daemon/src/main.rs` tests, add pure helper tests first:

```rust
#[test]
fn rebuild_runs_on_start_defaults_to_false() {
    assert!(!rebuild_runs_on_start_value(None));
}

#[test]
fn rebuild_runs_on_start_accepts_one_true_and_yes() {
    assert!(rebuild_runs_on_start_value(Some("1".to_string())));
    assert!(rebuild_runs_on_start_value(Some("true".to_string())));
    assert!(rebuild_runs_on_start_value(Some("yes".to_string())));
}

#[test]
fn rebuild_runs_on_start_rejects_other_values() {
    assert!(!rebuild_runs_on_start_value(Some("0".to_string())));
    assert!(!rebuild_runs_on_start_value(Some("false".to_string())));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-daemon rebuild_runs_on_start`

Expected: FAIL with missing `rebuild_runs_on_start_value`.

- [ ] **Step 3: Implement daemon service wiring**

In `crates/mneme-daemon/src/main.rs`, update imports:

```rust
use mneme_api::routes::build_router_with_service;
use mneme_kernel::{RunService, SqliteRunService, SqliteStore};
```

Add helpers:

```rust
fn rebuild_runs_on_start() -> bool {
    rebuild_runs_on_start_value(std::env::var("MNEME_REBUILD_RUNS_ON_START").ok())
}

fn rebuild_runs_on_start_value(value: Option<String>) -> bool {
    matches!(value.as_deref(), Some("1") | Some("true") | Some("yes"))
}
```

Change `main()` storage wiring to:

```rust
let store = SqliteStore::connect(&database_url).await?;
let service = SqliteRunService::new(store);
if rebuild_runs_on_start() {
    let rebuilt = service.rebuild_run_projections().await?;
    info!(run_count = rebuilt.len(), "rebuilt run projections from operation ledger");
}
let app = build_router_with_service(service.shared());
```

This removes direct daemon construction of `SqliteOperationLedger` and `SqliteRunProjectionStore`.

- [ ] **Step 4: Update README**

In `README.md`, add under database/runbook section:

```markdown
如需在启动时从 `operation_events` 修复 `runs` 读模型，可以显式开启：

```bash
MNEME_REBUILD_RUNS_ON_START=1 cargo run -p mneme-daemon
```

默认不自动 rebuild，避免每次启动都重写读模型；当迁移、调试或怀疑 projection 失配时再开启。
```

- [ ] **Step 5: Run verification**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-daemon && cargo test --workspace
```

Expected: PASS. Daemon tests cover repair flag parsing without mutating process environment.

- [ ] **Step 6: Commit**

```bash
git add crates/mneme-daemon/src/main.rs README.md
git commit -m "feat: route daemon through SQLite run service"
```

---

### Task 5: 增加 service-level SQLite integration test

**Files:**
- Modify: `crates/mneme-api/src/routes.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`

**Interfaces:**
- Consumes: `build_router_with_service(SqliteRunService::new(store).shared())`。
- Produces: integration-style test proving API → SharedRunService → SQLite transaction → runs query。

- [ ] **Step 1: Add API test using SQLite service**

In `crates/mneme-api/src/routes.rs` test module, import:

```rust
use mneme_kernel::{SqliteRunService, SqliteStore};
```

Add:

```rust
#[tokio::test]
async fn api_create_run_uses_sqlite_run_service_projection() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let app = build_router_with_service(SqliteRunService::new(store).shared());

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"instruction":"SQLite service run"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let runs_response = app
        .oneshot(Request::builder().uri("/api/v1/runs").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(runs_response.status(), StatusCode::OK);
    let body = runs_response.into_body().collect().await.unwrap().to_bytes();
    let payload: RunsResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload.runs.len(), 1);
    assert_eq!(payload.runs[0].instruction, "SQLite service run");
}
```

- [ ] **Step 2: Run test to verify it fails if API still bypasses service**

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-api sqlite_run_service -- --nocapture`

Expected: PASS after Tasks 2–3. If it fails, fix route/service integration before continuing.

- [ ] **Step 3: Add SQLite service blank validation test**

In `crates/mneme-kernel/src/storage/sqlite.rs` tests, add:

```rust
#[tokio::test]
async fn sqlite_run_service_rejects_blank_instruction_without_writing() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let service = SqliteRunService::new(store.clone());

    let error = service.create_run("   ".to_string()).await.unwrap_err();

    assert_eq!(error, RunServiceError::InvalidInstruction);
    assert!(SqliteOperationLedger::new(store.clone()).list().await.unwrap().is_empty());
    assert!(SqliteRunProjectionStore::new(store).list_runs().await.unwrap().is_empty());
}
```

- [ ] **Step 4: Run verification**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-api sqlite_run_service && cargo test -p mneme-kernel sqlite_run_service && cargo test --workspace
```

Expected: PASS. This task is a safety net around the new service boundary.

- [ ] **Step 5: Commit**

```bash
git add crates/mneme-api/src/routes.rs crates/mneme-kernel/src/storage/sqlite.rs
git commit -m "test: cover SQLite run service API integration"
```

---

### Task 6: 文档化 Kernel service boundary

**Files:**
- Create: `docs/architecture/milestone-2-kernel-service-boundary.md`
- Modify: `docs/architecture/milestone-1-persistent-ledger.md`
- Modify: `docs/architecture/phase-1-foundation.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: implemented Milestone 2 service architecture。
- Produces: Chinese architecture/runbook docs for service boundary and repair.

- [ ] **Step 1: Create architecture doc**

Create `docs/architecture/milestone-2-kernel-service-boundary.md`:

```markdown
# Milestone 2：Kernel RunService 边界

## 边界

Milestone 2 把 run 写入语义从 HTTP route 和 daemon wiring 中收敛到 Kernel service。外部 adapter 只负责协议转换：HTTP API 把 JSON request 转成 service call，daemon 初始化 SQLite store 并注入 service，未来 typed RPC/event bus 也应调用同一 service。

Milestone 2 不实现模型执行、工具审批、Tauri、自托管服务器或完整 agent run loop。

## 写入路径

```text
POST /api/v1/runs
  → mneme-api route
  → SharedRunService::create_run
  → OperationEvent::RunStarted
  → operation_events append
  → runs projection upsert
```

在 daemon 默认路径中，`SqliteRunService` 使用同一个 SQL transaction 写入 `operation_events` 与 `runs`，避免 append 成功但 projection 更新失败导致 HTTP 创建 run 后立即不可见。

## Repair 路径

`RunService::rebuild_run_projections()` 从 `operation_events` 读取事实源，并用 `RunProjector::replay()` 重建 `runs`。daemon 默认不自动 rebuild；需要修复时显式设置：

```bash
MNEME_REBUILD_RUNS_ON_START=1 cargo run -p mneme-daemon
```

## Adapter 规则

- API route 不直接调用 ledger/store 的写入组合。
- daemon 不直接拼装 ledger + projection 写入流程。
- SQLite 事务细节留在 Kernel storage/service 内部。
- Workbench 与 TypeScript client 继续只通过 HTTP API 访问。

## 验证

```bash
. "$HOME/.cargo/env" && pnpm check
git diff --check
```
```

- [ ] **Step 2: Update README**

Ensure `README.md` has a short section:

```markdown
## Kernel service boundary

HTTP API 不直接拼接 ledger 与 projection 写入流程。创建 run 会进入 Kernel `RunService`；daemon 默认使用 SQLite service，在同一事务内写入 `operation_events` 与 `runs`。
```

- [ ] **Step 3: Update previous architecture docs**

In `docs/architecture/milestone-1-persistent-ledger.md`, add a section:

```markdown
## Milestone 2 之后的服务边界

Milestone 1 建立了 SQLite 数据面；Milestone 2 在其上新增 `RunService`，让 API/daemon/未来 typed RPC 共享同一业务入口。`operation_events` 与 `runs` 的职责不变，但 run 创建不再由 API route 直接编排。
```

In `docs/architecture/phase-1-foundation.md`, add one sentence under the existing evolution section:

```markdown
Milestone 2 进一步把创建 run 的业务流程收敛进 Kernel service，避免 HTTP route 直接组合多个存储写入。
```

- [ ] **Step 4: Run documentation verification**

Run:

```bash
git diff --check
. "$HOME/.cargo/env" && pnpm check
```

Expected: PASS. Docs changed without breaking code checks.

- [ ] **Step 5: Commit**

```bash
git add README.md docs/architecture/milestone-2-kernel-service-boundary.md docs/architecture/milestone-1-persistent-ledger.md docs/architecture/phase-1-foundation.md
git commit -m "docs: document kernel service boundary"
```

---

### Task 7: Milestone 2 final verification and progress notes

**Files:**
- Modify: `.superpowers/sdd/progress.md` only as untracked local progress ledger; do not commit this file.

**Interfaces:**
- Consumes: Tasks 1–6 commits。
- Produces: verified milestone summary and known follow-up list for Milestone 3。

- [ ] **Step 1: Run final verification**

Run:

```bash
. "$HOME/.cargo/env"
git status --short --branch
git log --oneline f26a3ce..HEAD
pnpm check
git diff --check
git diff --stat f26a3ce..HEAD
git status --short --branch
```

Expected:
- branch `phase-1-foundation`
- status clean
- all Rust/UI checks pass
- Milestone 2 commits present after `f26a3ce`

- [ ] **Step 2: Update progress ledger**

Append to `.superpowers/sdd/progress.md`:

```markdown
Milestone 2: complete (commits f26a3ce..<HEAD7>, final verification passed, overall review approved; follow-ups for Milestone 3: service methods for started → model_requested → assistant_message → completed/failed lifecycle)
```

Use the actual short HEAD hash after final review.

- [ ] **Step 3: Dispatch overall review**

Use the subagent-driven-development final review flow. Review range: `f26a3ce..HEAD`.

The reviewer should check:
- API/daemon no longer directly own run write orchestration.
- daemon default create-run path uses SQLite service transaction.
- service repair path can rebuild `runs` from `operation_events`.
- HTTP API shape remains compatible.
- no Agent run loop/model/tool/Tauri/cloud scope creep.

- [ ] **Step 4: Commit status**

No git commit is needed for `.superpowers/sdd/progress.md` because `.superpowers/` is gitignored. If final review requires doc/code fixes, make one fix commit and rerun final verification before marking complete.

---

## Milestone 2 Success Criteria

- `RunService` exists in Kernel and is the business boundary for run creation/query/repair.
- API routes call `SharedRunService`; they do not directly append ledger + apply projection for run creation.
- daemon default path uses `SqliteRunService`, not manual ledger/store write composition.
- SQLite run creation writes `operation_events` and `runs` inside one SQL transaction.
- projection repair is available through `RunService::rebuild_run_projections()` and daemon `MNEME_REBUILD_RUNS_ON_START=1`.
- `GET /api/v1/health`, `GET /api/v1/events`, `GET /api/v1/runs`, `POST /api/v1/runs` retain compatible HTTP behavior.
- `pnpm check` and `git diff --check` pass.
- No model execution, Agent loop, tool approval, Tauri packaging, or self-hosted server implementation is added.
