# Mneme Phase 1 Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first testable Mneme repository foundation: Rust workspace, kernel domain types, append-only in-memory Operation Ledger, Axum daemon API, TypeScript API client, and a minimal React workbench that proves UI communicates with the local kernel boundary.

**Architecture:** This plan implements the thinnest vertical slice of the approved technical direction. Rust owns kernel/domain/API authority; the UI consumes a generated-compatible TypeScript client shape but does not write state directly. Storage starts as an in-memory ledger to prove domain and API boundaries before SQLite migrations are introduced in the next plan.

**Tech Stack:** Rust stable, Cargo workspace, Tokio, Axum, Serde, thiserror, uuid, time, tracing, Vite, React, TypeScript, Vitest, pnpm.

## Global Constraints

- Product direction: Personal AI Kernel named Mneme.
- First runtime shape: Desktop App + Local Daemon/API.
- Kernel strategy: Rust-first Kernel with TypeScript UI.
- Interface strategy: full dual interface: typed internal service boundary plus external HTTP/local API.
- Data direction: Operation Ledger as source of truth; SQLite projections and search/vector sidecars are deferred to a later plan.
- Runtime direction: Rust Kernel is authoritative control plane; workers do not write kernel state directly.
- Tool direction: Mneme-native effect-typed Tool Protocol; MCP/OpenAPI/CLI adapters are compatibility layers in later plans.
- Frontend route: React + Vite + Radix primitives + self-owned design system.
- Sync route: personal cloud server/self-hosted server is a future milestone; this plan keeps local API boundaries sync-ready.
- The current workspace may not have Rust installed. If `rustc --version` fails, install Rust stable with `rustup` before starting Task 1.
- Every task must end with tests or verification commands and a git commit.

---

## Ordered Product Roadmap

This product is too large for one implementation plan. Build it in these milestones, each with its own plan and review gate.

### Milestone 0 — Repository and vertical-slice foundation

Outcome: a clean monorepo skeleton with Rust crates, daemon health/events API, TypeScript client, and React workbench consuming the API. This is the scope of this plan.

### Milestone 1 — Durable Operation Ledger and projections

Outcome: SQLite-backed append-only events, SQLx migrations, projection tables for runs/tasks/artifacts/tool calls, and deterministic replay tests.

### Milestone 2 — Kernel service boundary

Outcome: AgentService, TaskService, RunService, ContextService, ToolService, PolicyService, and EventService traits with tested command/query flows.

### Milestone 3 — Agent run loop MVP

Outcome: create a run, compile a simple ContextBundle, call one OpenAI-compatible or Ollama provider, stream model output, and record all run events.

### Milestone 4 — Effect-typed tools and approvals

Outcome: ToolDescriptor, ToolCallIntent, PolicyService approval decisions, NativeExecutor for read-only filesystem/artifact operations, and UI approval panel.

### Milestone 5 — Artifact and context inspector workspace

Outcome: content-addressed artifact store, ContextBundle explainability, Work Room, Operation Timeline, Context Inspector, and Artifact Workspace.

### Milestone 6 — Search, memory, and knowledge

Outcome: Tantivy full-text index, vector index abstraction, memory metadata, knowledge sources, document chunks, and retrieval traces in the ledger.

### Milestone 7 — Worker runtime protocol

Outcome: stdio JSON-RPC worker protocol, Rust-native provider worker, Node worker adapter, worker supervision, and event-streamed runtime errors.

### Milestone 8 — Tauri desktop packaging

Outcome: Tauri 2 shell that starts/manages the local daemon, embeds the React UI, and exposes native OS integrations through a thin bridge.

### Milestone 9 — Self-hosted personal server

Outcome: authenticated device registration, event/object sync, backup/restore, remote web console, and conflict rules built around the ledger.

---

## File Structure for This Plan

Create this structure:

```text
mneme/
  Cargo.toml
  crates/
    mneme-kernel/
      Cargo.toml
      src/
        lib.rs
        ids.rs
        events.rs
        ledger.rs
        health.rs
    mneme-api/
      Cargo.toml
      src/
        lib.rs
        routes.rs
        types.rs
    mneme-daemon/
      Cargo.toml
      src/
        main.rs
  apps/
    desktop/
      package.json
      index.html
      tsconfig.json
      tsconfig.node.json
      vite.config.ts
      src/
        main.tsx
        app/App.tsx
        app/App.test.tsx
        api/client.ts
        api/client.test.ts
        design-system/tokens.css
        styles.css
  package.json
  pnpm-workspace.yaml
  docs/
    superpowers/
      specs/2026-11-09-mneme-technical-selection-design.md
      plans/2026-11-09-mneme-phase-1-foundation.md
```

Responsibilities:

- `Cargo.toml`: Rust workspace membership and shared dependency versions.
- `crates/mneme-kernel/src/ids.rs`: typed identifiers for events and runs.
- `crates/mneme-kernel/src/events.rs`: canonical event model for the initial Operation Ledger.
- `crates/mneme-kernel/src/ledger.rs`: append-only ledger trait and in-memory implementation.
- `crates/mneme-kernel/src/health.rs`: kernel health snapshot used by API/UI.
- `crates/mneme-api/src/types.rs`: external API DTOs, separate from domain types.
- `crates/mneme-api/src/routes.rs`: Axum router and route handlers.
- `crates/mneme-daemon/src/main.rs`: binary entry point that serves the local API.
- `apps/desktop/src/api/client.ts`: typed UI API client boundary.
- `apps/desktop/src/app/App.tsx`: minimal workbench view that reads kernel health and latest ledger events.
- `apps/desktop/src/design-system/tokens.css`: first Mneme design tokens.

---

### Task 1: Bootstrap Rust workspace and kernel health model

**Files:**
- Create: `Cargo.toml`
- Create: `crates/mneme-kernel/Cargo.toml`
- Create: `crates/mneme-kernel/src/lib.rs`
- Create: `crates/mneme-kernel/src/health.rs`

**Interfaces:**
- Consumes: none.
- Produces: `mneme_kernel::KernelHealth`, `mneme_kernel::kernel_health() -> KernelHealth`.

- [ ] **Step 1: Verify Rust is available**

Run:

```bash
rustc --version
cargo --version
```

Expected: both commands print versions and exit 0. If either command is missing, install Rust stable with:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then open a new shell and re-run the version commands.

- [ ] **Step 2: Create root Rust workspace manifest**

Write `Cargo.toml`:

```toml
[workspace]
members = [
  "crates/mneme-kernel",
]
resolver = "2"

[workspace.package]
edition = "2021"
license = "MIT"
repository = "https://github.com/cyeonix/mneme"

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
time = { version = "0.3", features = ["serde", "formatting", "macros"] }
uuid = { version = "1.11", features = ["v4", "serde"] }
```

- [ ] **Step 3: Create kernel crate manifest**

Write `crates/mneme-kernel/Cargo.toml`:

```toml
[package]
name = "mneme-kernel"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
serde.workspace = true
time.workspace = true
```

- [ ] **Step 4: Create kernel public module file**

Write `crates/mneme-kernel/src/lib.rs`:

```rust
pub mod health;

pub use health::{kernel_health, KernelHealth};
```

- [ ] **Step 5: Write the failing kernel health test**

Write `crates/mneme-kernel/src/health.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelHealth {
    pub service: String,
    pub status: String,
    pub version: String,
}

pub fn kernel_health() -> KernelHealth {
    KernelHealth {
        service: "mneme-kernel".to_string(),
        status: "starting".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_health_reports_ready_service_identity() {
        let health = kernel_health();

        assert_eq!(health.service, "mneme-kernel");
        assert_eq!(health.status, "ready");
        assert_eq!(health.version, "0.1.0");
    }
}
```

- [ ] **Step 6: Run test to verify it fails**

Run:

```bash
cargo test -p mneme-kernel kernel_health_reports_ready_service_identity
```

Expected: FAIL with an assertion showing left value `"starting"` and right value `"ready"`.

- [ ] **Step 7: Implement minimal fix**

Edit `crates/mneme-kernel/src/health.rs` so `kernel_health()` returns `ready`:

```rust
pub fn kernel_health() -> KernelHealth {
    KernelHealth {
        service: "mneme-kernel".to_string(),
        status: "ready".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}
```

- [ ] **Step 8: Run test to verify it passes**

Run:

```bash
cargo test -p mneme-kernel kernel_health_reports_ready_service_identity
```

Expected: PASS.

- [ ] **Step 9: Run crate formatting and all kernel tests**

Run:

```bash
cargo fmt --all --check
cargo test -p mneme-kernel
```

Expected: formatting check exits 0 and all kernel tests pass.

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml crates/mneme-kernel
git commit -m "feat: bootstrap Rust kernel crate"
```

---

### Task 2: Add canonical event IDs and Operation Ledger event model

**Files:**
- Modify: `crates/mneme-kernel/src/lib.rs`
- Create: `crates/mneme-kernel/src/ids.rs`
- Create: `crates/mneme-kernel/src/events.rs`
- Modify: `crates/mneme-kernel/Cargo.toml`

**Interfaces:**
- Consumes: `mneme_kernel::KernelHealth` from Task 1.
- Produces: `EventId::new() -> EventId`, `RunId::new() -> RunId`, `OperationEvent`, `OperationEventKind`, `OperationEvent::new_run_started(run_id: RunId, instruction: impl Into<String>) -> OperationEvent`.

- [ ] **Step 1: Add uuid/time dependencies to kernel crate**

Edit `crates/mneme-kernel/Cargo.toml`:

```toml
[dependencies]
serde.workspace = true
time.workspace = true
uuid.workspace = true
```

- [ ] **Step 2: Create typed ID module with tests**

Write `crates/mneme-kernel/src/ids.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(Uuid);

impl EventId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl RunId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_event_ids_are_unique() {
        let first = EventId::new();
        let second = EventId::new();

        assert_ne!(first, second);
    }

    #[test]
    fn generated_run_ids_are_displayable() {
        let run_id = RunId::new().to_string();

        assert_eq!(run_id.len(), 36);
        assert!(run_id.contains('-'));
    }
}
```

- [ ] **Step 3: Create operation event model with test**

Write `crates/mneme-kernel/src/events.rs`:

```rust
use crate::ids::{EventId, RunId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OperationEventKind {
    RunStarted { instruction: String },
    RunCompleted { summary: String },
    RunFailed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationEvent {
    pub id: EventId,
    pub run_id: RunId,
    pub occurred_at: OffsetDateTime,
    pub kind: OperationEventKind,
}

impl OperationEvent {
    pub fn new_run_started(run_id: RunId, instruction: impl Into<String>) -> Self {
        Self {
            id: EventId::new(),
            run_id,
            occurred_at: OffsetDateTime::now_utc(),
            kind: OperationEventKind::RunStarted {
                instruction: instruction.into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_started_event_captures_instruction_and_run_id() {
        let run_id = RunId::new();
        let event = OperationEvent::new_run_started(run_id, "Draft the Mneme roadmap");

        assert_eq!(event.run_id, run_id);
        assert!(matches!(
            event.kind,
            OperationEventKind::RunStarted { ref instruction }
                if instruction == "Draft the Mneme roadmap"
        ));
    }
}
```

- [ ] **Step 4: Export new modules**

Edit `crates/mneme-kernel/src/lib.rs`:

```rust
pub mod events;
pub mod health;
pub mod ids;

pub use events::{OperationEvent, OperationEventKind};
pub use health::{kernel_health, KernelHealth};
pub use ids::{EventId, RunId};
```

- [ ] **Step 5: Run tests**

Run:

```bash
cargo fmt --all --check
cargo test -p mneme-kernel
```

Expected: formatting check exits 0 and tests for health, IDs, and events pass.

- [ ] **Step 6: Commit**

```bash
git add crates/mneme-kernel/Cargo.toml crates/mneme-kernel/src
git commit -m "feat: add kernel operation event model"
```

---

### Task 3: Add append-only in-memory Operation Ledger service

**Files:**
- Modify: `crates/mneme-kernel/src/lib.rs`
- Create: `crates/mneme-kernel/src/ledger.rs`
- Modify: `crates/mneme-kernel/Cargo.toml`

**Interfaces:**
- Consumes: `OperationEvent`, `RunId` from Task 2.
- Produces: `OperationLedger` trait, `InMemoryOperationLedger`, `LedgerError`.

- [ ] **Step 1: Add thiserror dependency to kernel crate**

Edit `crates/mneme-kernel/Cargo.toml`:

```toml
[dependencies]
serde.workspace = true
thiserror.workspace = true
time.workspace = true
uuid.workspace = true
```

- [ ] **Step 2: Write ledger module with append/list tests**

Write `crates/mneme-kernel/src/ledger.rs`:

```rust
use crate::{OperationEvent, RunId};
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LedgerError {
    #[error("operation ledger lock is poisoned")]
    Poisoned,
}

pub trait OperationLedger: Send + Sync {
    fn append(&self, event: OperationEvent) -> Result<(), LedgerError>;
    fn list(&self) -> Result<Vec<OperationEvent>, LedgerError>;
    fn list_for_run(&self, run_id: RunId) -> Result<Vec<OperationEvent>, LedgerError>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryOperationLedger {
    events: Arc<Mutex<Vec<OperationEvent>>>,
}

impl InMemoryOperationLedger {
    pub fn new() -> Self {
        Self::default()
    }
}

impl OperationLedger for InMemoryOperationLedger {
    fn append(&self, event: OperationEvent) -> Result<(), LedgerError> {
        let mut events = self.events.lock().map_err(|_| LedgerError::Poisoned)?;
        events.push(event);
        Ok(())
    }

    fn list(&self) -> Result<Vec<OperationEvent>, LedgerError> {
        let events = self.events.lock().map_err(|_| LedgerError::Poisoned)?;
        Ok(events.clone())
    }

    fn list_for_run(&self, run_id: RunId) -> Result<Vec<OperationEvent>, LedgerError> {
        let events = self.events.lock().map_err(|_| LedgerError::Poisoned)?;
        Ok(events
            .iter()
            .filter(|event| event.run_id == run_id)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OperationEvent;

    #[test]
    fn ledger_appends_events_in_order() {
        let ledger = InMemoryOperationLedger::new();
        let run_id = RunId::new();
        let first = OperationEvent::new_run_started(run_id, "First instruction");
        let second = OperationEvent::new_run_started(run_id, "Second instruction");

        ledger.append(first.clone()).expect("append first event");
        ledger.append(second.clone()).expect("append second event");

        assert_eq!(ledger.list().expect("list events"), vec![first, second]);
    }

    #[test]
    fn ledger_lists_events_for_one_run() {
        let ledger = InMemoryOperationLedger::new();
        let selected_run = RunId::new();
        let other_run = RunId::new();
        let selected_event = OperationEvent::new_run_started(selected_run, "Selected run");
        let other_event = OperationEvent::new_run_started(other_run, "Other run");

        ledger.append(selected_event.clone()).expect("append selected event");
        ledger.append(other_event).expect("append other event");

        assert_eq!(
            ledger.list_for_run(selected_run).expect("list selected run"),
            vec![selected_event]
        );
    }
}
```

- [ ] **Step 3: Export ledger interfaces**

Edit `crates/mneme-kernel/src/lib.rs`:

```rust
pub mod events;
pub mod health;
pub mod ids;
pub mod ledger;

pub use events::{OperationEvent, OperationEventKind};
pub use health::{kernel_health, KernelHealth};
pub use ids::{EventId, RunId};
pub use ledger::{InMemoryOperationLedger, LedgerError, OperationLedger};
```

- [ ] **Step 4: Run tests**

Run:

```bash
cargo fmt --all --check
cargo test -p mneme-kernel
```

Expected: all kernel tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/mneme-kernel/Cargo.toml crates/mneme-kernel/src
git commit -m "feat: add in-memory operation ledger"
```

---

### Task 4: Add local HTTP API crate with health and events routes

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/mneme-api/Cargo.toml`
- Create: `crates/mneme-api/src/lib.rs`
- Create: `crates/mneme-api/src/types.rs`
- Create: `crates/mneme-api/src/routes.rs`

**Interfaces:**
- Consumes: `kernel_health()`, `InMemoryOperationLedger`, `OperationLedger`, `OperationEvent`, `RunId`.
- Produces: `mneme_api::build_router(ledger: InMemoryOperationLedger) -> axum::Router`, HTTP `GET /api/v1/health`, `GET /api/v1/events`, `POST /api/v1/runs`.

- [ ] **Step 1: Add API crate to workspace**

Edit root `Cargo.toml` workspace members:

```toml
[workspace]
members = [
  "crates/mneme-kernel",
  "crates/mneme-api",
]
resolver = "2"
```

Add workspace dependencies:

```toml
[workspace.dependencies]
axum = "0.7"
http-body-util = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
time = { version = "0.3", features = ["serde", "formatting", "macros"] }
tokio = { version = "1.41", features = ["macros", "rt-multi-thread", "net"] }
tower = "0.5"
uuid = { version = "1.11", features = ["v4", "serde"] }
```

- [ ] **Step 2: Create API crate manifest**

Write `crates/mneme-api/Cargo.toml`:

```toml
[package]
name = "mneme-api"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
axum.workspace = true
mneme-kernel = { path = "../mneme-kernel" }
serde.workspace = true
serde_json.workspace = true
time.workspace = true

[dev-dependencies]
http-body-util.workspace = true
tokio.workspace = true
tower.workspace = true
```

- [ ] **Step 3: Create API DTOs**

Write `crates/mneme-api/src/types.rs`:

```rust
use mneme_kernel::{KernelHealth, OperationEvent};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub service: String,
    pub status: String,
    pub version: String,
}

impl From<KernelHealth> for HealthResponse {
    fn from(value: KernelHealth) -> Self {
        Self {
            service: value.service,
            status: value.status,
            version: value.version,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRunRequest {
    pub instruction: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRunResponse {
    pub run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventsResponse {
    pub events: Vec<OperationEvent>,
}
```

- [ ] **Step 4: Write failing route tests and router implementation**

Write `crates/mneme-api/src/routes.rs`:

```rust
use crate::types::{CreateRunRequest, CreateRunResponse, EventsResponse, HealthResponse};
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use mneme_kernel::{kernel_health, InMemoryOperationLedger, OperationEvent, OperationLedger, RunId};

#[derive(Clone)]
pub struct ApiState {
    pub ledger: InMemoryOperationLedger,
}

pub fn build_router(ledger: InMemoryOperationLedger) -> Router {
    Router::new()
        .route("/api/v1/health", get(get_health))
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/runs", post(create_run))
        .with_state(ApiState { ledger })
}

async fn get_health() -> Json<HealthResponse> {
    Json(kernel_health().into())
}

async fn list_events(State(state): State<ApiState>) -> Result<Json<EventsResponse>, StatusCode> {
    let events = state
        .ledger
        .list()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(EventsResponse { events }))
}

async fn create_run(
    State(state): State<ApiState>,
    Json(request): Json<CreateRunRequest>,
) -> Result<Json<CreateRunResponse>, StatusCode> {
    if request.instruction.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let run_id = RunId::new();
    let event = OperationEvent::new_run_started(run_id, request.instruction);
    state
        .ledger
        .append(event)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(CreateRunResponse {
        run_id: run_id.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use serde_json::json;
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_endpoint_returns_ready_kernel_status() {
        let app = build_router(InMemoryOperationLedger::new());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.expect("body").to_bytes();
        let payload: HealthResponse = serde_json::from_slice(&body).expect("health response");

        assert_eq!(payload.service, "mneme-kernel");
        assert_eq!(payload.status, "ready");
    }

    #[tokio::test]
    async fn create_run_appends_run_started_event() {
        let ledger = InMemoryOperationLedger::new();
        let app = build_router(ledger.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/runs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({ "instruction": "Build the first Mneme run" }).to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(ledger.list().expect("events").len(), 1);
    }

    #[tokio::test]
    async fn create_run_rejects_blank_instruction() {
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
}
```

- [ ] **Step 5: Export API module**

Write `crates/mneme-api/src/lib.rs`:

```rust
pub mod routes;
pub mod types;

pub use routes::build_router;
```

- [ ] **Step 6: Run API tests**

Run:

```bash
cargo fmt --all --check
cargo test -p mneme-api
cargo test --workspace
```

Expected: API route tests and kernel tests pass.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/mneme-api
git commit -m "feat: add local API routes"
```

---

### Task 5: Add daemon binary serving the local API

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/mneme-daemon/Cargo.toml`
- Create: `crates/mneme-daemon/src/main.rs`

**Interfaces:**
- Consumes: `mneme_api::build_router`, `mneme_kernel::InMemoryOperationLedger`.
- Produces: `mneme-daemon` binary with default bind address `127.0.0.1:3766`.

- [ ] **Step 1: Add daemon crate to workspace**

Edit root `Cargo.toml` workspace members:

```toml
[workspace]
members = [
  "crates/mneme-kernel",
  "crates/mneme-api",
  "crates/mneme-daemon",
]
resolver = "2"
```

Add `tracing` dependencies if missing:

```toml
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

- [ ] **Step 2: Create daemon crate manifest**

Write `crates/mneme-daemon/Cargo.toml`:

```toml
[package]
name = "mneme-daemon"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
mneme-api = { path = "../mneme-api" }
mneme-kernel = { path = "../mneme-kernel" }
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
```

- [ ] **Step 3: Create daemon main**

Write `crates/mneme-daemon/src/main.rs`:

```rust
use mneme_api::build_router;
use mneme_kernel::InMemoryOperationLedger;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mneme_daemon=info,tower_http=info".into()),
        )
        .init();

    let addr: SocketAddr = std::env::var("MNEME_BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3766".to_string())
        .parse()?;
    let ledger = InMemoryOperationLedger::new();
    let app = build_router(ledger);
    let listener = TcpListener::bind(addr).await?;

    info!(%addr, "mneme daemon listening");
    axum::serve(listener, app).await?;

    Ok(())
}
```

- [ ] **Step 4: Fix missing axum dependency for daemon**

If `cargo check -p mneme-daemon` reports `use of undeclared crate or module 'axum'`, add this line to `crates/mneme-daemon/Cargo.toml` under `[dependencies]`:

```toml
axum.workspace = true
```

- [ ] **Step 5: Run daemon checks**

Run:

```bash
cargo fmt --all --check
cargo check -p mneme-daemon
cargo test --workspace
```

Expected: daemon compiles and workspace tests pass.

- [ ] **Step 6: Manually verify health endpoint**

Run daemon in one terminal:

```bash
cargo run -p mneme-daemon
```

In another terminal:

```bash
curl http://127.0.0.1:3766/api/v1/health
```

Expected JSON contains:

```json
{"service":"mneme-kernel","status":"ready","version":"0.1.0"}
```

Stop the daemon with `Ctrl+C`.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/mneme-daemon
git commit -m "feat: add local daemon binary"
```

---

### Task 6: Bootstrap pnpm workspace and TypeScript API client

**Files:**
- Create: `package.json`
- Create: `pnpm-workspace.yaml`
- Create: `apps/desktop/package.json`
- Create: `apps/desktop/tsconfig.json`
- Create: `apps/desktop/vite.config.ts`
- Create: `apps/desktop/src/api/client.ts`
- Create: `apps/desktop/src/api/client.test.ts`

**Interfaces:**
- Consumes: HTTP API from Task 4/5.
- Produces: `MnemeApiClient`, `createMnemeApiClient(baseUrl: string): MnemeApiClient`, `KernelHealth`, `OperationEvent`, `CreateRunResponse`.

- [ ] **Step 1: Create root pnpm workspace files**

Write `package.json`:

```json
{
  "name": "mneme",
  "private": true,
  "version": "0.1.0",
  "packageManager": "pnpm@11.9.0",
  "scripts": {
    "check": "pnpm --recursive check",
    "test": "pnpm --recursive test"
  }
}
```

Write `pnpm-workspace.yaml`:

```yaml
packages:
  - apps/*
  - packages/*
```

- [ ] **Step 2: Create desktop package manifest**

Write `apps/desktop/package.json`:

```json
{
  "name": "@mneme/desktop",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "check": "tsc --noEmit",
    "test": "vitest run",
    "dev": "vite",
    "build": "tsc --noEmit && vite build"
  },
  "dependencies": {
    "@vitejs/plugin-react": "latest",
    "vite": "latest",
    "typescript": "latest",
    "react": "latest",
    "react-dom": "latest"
  },
  "devDependencies": {
    "vitest": "latest",
    "jsdom": "latest",
    "@testing-library/react": "latest",
    "@testing-library/jest-dom": "latest",
    "@types/react": "latest",
    "@types/react-dom": "latest"
  }
}
```

- [ ] **Step 3: Create TypeScript and Vite config**

Write `apps/desktop/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "allowJs": false,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "allowSyntheticDefaultImports": true,
    "strict": true,
    "forceConsistentCasingInFileNames": true,
    "module": "ESNext",
    "moduleResolution": "Node",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx"
  },
  "include": ["src"]
}
```

Write `apps/desktop/vite.config.ts`:

```ts
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
  },
});
```

- [ ] **Step 4: Write API client test first**

Write `apps/desktop/src/api/client.test.ts`:

```ts
import { describe, expect, it, vi } from 'vitest';
import { createMnemeApiClient } from './client';

describe('createMnemeApiClient', () => {
  it('fetches kernel health from the configured base URL', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ service: 'mneme-kernel', status: 'ready', version: '0.1.0' }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    const client = createMnemeApiClient('http://127.0.0.1:3766', fetchMock);

    const health = await client.getHealth();

    expect(fetchMock).toHaveBeenCalledWith('http://127.0.0.1:3766/api/v1/health', {
      headers: { accept: 'application/json' },
    });
    expect(health.status).toBe('ready');
  });

  it('posts run instructions and returns the new run id', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ run_id: 'run-123' }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    const client = createMnemeApiClient('http://127.0.0.1:3766', fetchMock);

    const response = await client.createRun('Draft the first Mneme task');

    expect(fetchMock).toHaveBeenCalledWith('http://127.0.0.1:3766/api/v1/runs', {
      method: 'POST',
      headers: { accept: 'application/json', 'content-type': 'application/json' },
      body: JSON.stringify({ instruction: 'Draft the first Mneme task' }),
    });
    expect(response.run_id).toBe('run-123');
  });
});
```

- [ ] **Step 5: Run test to verify it fails**

Run:

```bash
pnpm install
pnpm --filter @mneme/desktop test -- src/api/client.test.ts
```

Expected: FAIL because `./client` does not exist.

- [ ] **Step 6: Implement API client**

Write `apps/desktop/src/api/client.ts`:

```ts
export interface KernelHealth {
  service: string;
  status: string;
  version: string;
}

export interface OperationEvent {
  id: string;
  run_id: string;
  occurred_at: string;
  kind: Record<string, unknown>;
}

export interface EventsResponse {
  events: OperationEvent[];
}

export interface CreateRunResponse {
  run_id: string;
}

export interface MnemeApiClient {
  getHealth(): Promise<KernelHealth>;
  listEvents(): Promise<EventsResponse>;
  createRun(instruction: string): Promise<CreateRunResponse>;
}

type FetchLike = typeof fetch;

async function parseJsonResponse<T>(response: Response): Promise<T> {
  if (!response.ok) {
    throw new Error(`Mneme API request failed with status ${response.status}`);
  }
  return (await response.json()) as T;
}

export function createMnemeApiClient(baseUrl: string, fetchImpl: FetchLike = fetch): MnemeApiClient {
  const normalizedBaseUrl = baseUrl.replace(/\/$/, '');

  return {
    async getHealth() {
      const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/health`, {
        headers: { accept: 'application/json' },
      });
      return parseJsonResponse<KernelHealth>(response);
    },

    async listEvents() {
      const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/events`, {
        headers: { accept: 'application/json' },
      });
      return parseJsonResponse<EventsResponse>(response);
    },

    async createRun(instruction: string) {
      const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/runs`, {
        method: 'POST',
        headers: { accept: 'application/json', 'content-type': 'application/json' },
        body: JSON.stringify({ instruction }),
      });
      return parseJsonResponse<CreateRunResponse>(response);
    },
  };
}
```

- [ ] **Step 7: Run client tests and typecheck**

Run:

```bash
pnpm --filter @mneme/desktop test -- src/api/client.test.ts
pnpm --filter @mneme/desktop check
```

Expected: test passes and TypeScript check exits 0.

- [ ] **Step 8: Commit**

```bash
git add package.json pnpm-workspace.yaml pnpm-lock.yaml apps/desktop
git commit -m "feat: add TypeScript API client"
```

---

### Task 7: Add minimal React workbench shell

**Files:**
- Create: `apps/desktop/index.html`
- Create: `apps/desktop/src/main.tsx`
- Create: `apps/desktop/src/app/App.tsx`
- Create: `apps/desktop/src/app/App.test.tsx`
- Create: `apps/desktop/src/design-system/tokens.css`
- Create: `apps/desktop/src/styles.css`

**Interfaces:**
- Consumes: `MnemeApiClient`, `KernelHealth`, `EventsResponse` from Task 6.
- Produces: `App({ client }: { client: MnemeApiClient }): JSX.Element`.

- [ ] **Step 1: Write app component test first**

Write `apps/desktop/src/app/App.test.tsx`:

```tsx
import '@testing-library/jest-dom/vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { MnemeApiClient } from '../api/client';
import { App } from './App';

function createClient(): MnemeApiClient {
  return {
    async getHealth() {
      return { service: 'mneme-kernel', status: 'ready', version: '0.1.0' };
    },
    async listEvents() {
      return { events: [] };
    },
    async createRun() {
      return { run_id: 'run-123' };
    },
  };
}

describe('App', () => {
  it('renders the Mneme workbench and kernel health', async () => {
    render(<App client={createClient()} />);

    expect(screen.getByRole('heading', { name: 'Mneme Workbench' })).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByText('Kernel ready')).toBeInTheDocument();
    });
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
pnpm --filter @mneme/desktop test -- src/app/App.test.tsx
```

Expected: FAIL because `./App` does not exist.

- [ ] **Step 3: Implement app component**

Write `apps/desktop/src/app/App.tsx`:

```tsx
import { useEffect, useState } from 'react';
import type { EventsResponse, KernelHealth, MnemeApiClient } from '../api/client';

interface AppProps {
  client: MnemeApiClient;
}

export function App({ client }: AppProps) {
  const [health, setHealth] = useState<KernelHealth | null>(null);
  const [events, setEvents] = useState<EventsResponse>({ events: [] });
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadKernelState() {
      try {
        const [nextHealth, nextEvents] = await Promise.all([
          client.getHealth(),
          client.listEvents(),
        ]);
        if (!cancelled) {
          setHealth(nextHealth);
          setEvents(nextEvents);
        }
      } catch (unknownError) {
        if (!cancelled) {
          setError(unknownError instanceof Error ? unknownError.message : 'Unknown Mneme API error');
        }
      }
    }

    void loadKernelState();

    return () => {
      cancelled = true;
    };
  }, [client]);

  return (
    <main className="mneme-shell">
      <section className="hero-panel">
        <p className="eyebrow">Personal AI Kernel</p>
        <h1>Mneme Workbench</h1>
        <p className="lede">A local-first control room for runs, context, tools, artifacts, and memory.</p>
      </section>

      <section className="status-grid" aria-label="Kernel status">
        <article className="status-card">
          <span className="label">Kernel</span>
          <strong>{health ? `Kernel ${health.status}` : 'Connecting to kernel'}</strong>
          <span className="meta">{health ? `${health.service} ${health.version}` : 'Waiting for local daemon'}</span>
        </article>
        <article className="status-card">
          <span className="label">Operation Ledger</span>
          <strong>{events.events.length} events</strong>
          <span className="meta">Append-only event stream foundation</span>
        </article>
      </section>

      {error ? <p className="error-banner">{error}</p> : null}
    </main>
  );
}
```

- [ ] **Step 4: Create browser entry files and styles**

Write `apps/desktop/index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Mneme</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

Write `apps/desktop/src/main.tsx`:

```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import { createMnemeApiClient } from './api/client';
import { App } from './app/App';
import './design-system/tokens.css';
import './styles.css';

const client = createMnemeApiClient(import.meta.env.VITE_MNEME_API_BASE_URL ?? 'http://127.0.0.1:3766');

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App client={client} />
  </React.StrictMode>,
);
```

Write `apps/desktop/src/design-system/tokens.css`:

```css
:root {
  color-scheme: dark;
  --mneme-color-bg: #080a0f;
  --mneme-color-panel: #10141d;
  --mneme-color-panel-strong: #171d2a;
  --mneme-color-text: #eef3ff;
  --mneme-color-muted: #9aa7bd;
  --mneme-color-accent: #9fd3ff;
  --mneme-color-risk: #ffb4a8;
  --mneme-radius-lg: 24px;
  --mneme-radius-md: 16px;
  --mneme-space-1: 0.25rem;
  --mneme-space-2: 0.5rem;
  --mneme-space-3: 0.75rem;
  --mneme-space-4: 1rem;
  --mneme-space-6: 1.5rem;
  --mneme-space-8: 2rem;
  --mneme-font-sans: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}
```

Write `apps/desktop/src/styles.css`:

```css
* {
  box-sizing: border-box;
}

body {
  margin: 0;
  min-width: 320px;
  min-height: 100vh;
  background: radial-gradient(circle at top left, rgba(159, 211, 255, 0.16), transparent 30%), var(--mneme-color-bg);
  color: var(--mneme-color-text);
  font-family: var(--mneme-font-sans);
}

.mneme-shell {
  width: min(1120px, calc(100vw - 48px));
  margin: 0 auto;
  padding: 72px 0;
}

.hero-panel,
.status-card {
  border: 1px solid rgba(255, 255, 255, 0.08);
  background: linear-gradient(135deg, rgba(16, 20, 29, 0.92), rgba(23, 29, 42, 0.72));
  border-radius: var(--mneme-radius-lg);
  box-shadow: 0 24px 80px rgba(0, 0, 0, 0.35);
}

.hero-panel {
  padding: var(--mneme-space-8);
}

.eyebrow,
.label,
.meta {
  color: var(--mneme-color-muted);
}

.eyebrow,
.label {
  margin: 0;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.72rem;
  font-weight: 700;
}

h1 {
  margin: var(--mneme-space-3) 0;
  font-size: clamp(2.5rem, 7vw, 5.5rem);
  line-height: 0.95;
}

.lede {
  max-width: 680px;
  color: var(--mneme-color-muted);
  font-size: 1.1rem;
  line-height: 1.7;
}

.status-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--mneme-space-4);
  margin-top: var(--mneme-space-4);
}

.status-card {
  display: grid;
  gap: var(--mneme-space-2);
  padding: var(--mneme-space-6);
}

.status-card strong {
  font-size: 1.4rem;
}

.error-banner {
  margin-top: var(--mneme-space-4);
  padding: var(--mneme-space-4);
  color: var(--mneme-color-risk);
  background: rgba(255, 180, 168, 0.1);
  border: 1px solid rgba(255, 180, 168, 0.22);
  border-radius: var(--mneme-radius-md);
}

@media (max-width: 720px) {
  .status-grid {
    grid-template-columns: 1fr;
  }
}
```

- [ ] **Step 5: Run app tests and build**

Run:

```bash
pnpm --filter @mneme/desktop test -- src/app/App.test.tsx
pnpm --filter @mneme/desktop check
pnpm --filter @mneme/desktop build
```

Expected: app test passes, TypeScript check exits 0, and Vite build succeeds.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop
git commit -m "feat: add minimal Mneme workbench"
```

---

### Task 8: Add root verification scripts and foundation documentation

**Files:**
- Modify: `package.json`
- Create: `docs/architecture/phase-1-foundation.md`
- Create: `.gitignore`
- Create: `README.md`

**Interfaces:**
- Consumes: Rust workspace and desktop workspace from prior tasks.
- Produces: documented verification commands for future agents and humans.

- [ ] **Step 1: Add root scripts for Rust and UI checks**

Edit root `package.json`:

```json
{
  "name": "mneme",
  "private": true,
  "version": "0.1.0",
  "packageManager": "pnpm@11.9.0",
  "scripts": {
    "check": "pnpm check:rust && pnpm check:ui",
    "check:rust": "cargo fmt --all --check && cargo test --workspace",
    "check:ui": "pnpm --recursive check && pnpm --recursive test",
    "test": "pnpm test:rust && pnpm test:ui",
    "test:rust": "cargo test --workspace",
    "test:ui": "pnpm --recursive test"
  }
}
```

- [ ] **Step 2: Create gitignore**

Write `.gitignore`:

```gitignore
/target/
/node_modules/
/apps/*/dist/
/.DS_Store
*.log
.env
.env.*
!.env.example
```

- [ ] **Step 3: Create README**

Write `README.md`:

````markdown
# Mneme

Mneme is a Personal AI Kernel: a local-first, auditable workbench for agents, context, tools, memory, artifacts, and operation history.

## Current phase

Phase 1 builds the foundation:

- Rust kernel crate
- append-only in-memory Operation Ledger
- local Axum API
- daemon binary
- TypeScript API client
- minimal React workbench

## Prerequisites

- Rust stable via rustup
- Node.js 24+
- pnpm 11+

## Verify

```bash
pnpm check
```

## Run local daemon

```bash
cargo run -p mneme-daemon
```

Health endpoint:

```bash
curl http://127.0.0.1:3766/api/v1/health
```

## Run desktop web shell during foundation phase

```bash
pnpm --filter @mneme/desktop dev
```
````

- [ ] **Step 4: Create architecture note**

Write `docs/architecture/phase-1-foundation.md`:

````markdown
# Phase 1 Foundation Architecture

Phase 1 proves the first vertical slice of Mneme without durable storage or model calls.

## Boundaries

- `mneme-kernel` owns domain types and the Operation Ledger interface.
- `mneme-api` exposes external HTTP DTOs and routes.
- `mneme-daemon` owns process startup and the local bind address.
- `apps/desktop` consumes the API through a typed client and does not write kernel state directly.

## Data flow

```text
React Workbench
  → MnemeApiClient
  → HTTP /api/v1/*
  → mneme-api routes
  → mneme-kernel ledger
```

## Current storage

The ledger is in-memory in this phase. Durable SQLite storage starts in the next milestone so event schemas and API boundaries can be validated first.

## Verification

Run:

```bash
pnpm check
```
````

- [ ] **Step 5: Run complete verification**

Run:

```bash
pnpm check
```

Expected: Rust formatting and tests pass; TypeScript checks and Vitest tests pass.

- [ ] **Step 6: Commit**

```bash
git add .gitignore README.md package.json docs/architecture/phase-1-foundation.md
git commit -m "docs: document phase 1 foundation"
```

---

## Final Verification for This Plan

After Task 8, run:

```bash
git status --short --branch
pnpm check
git log --oneline -8
```

Expected:

- `git status --short --branch` shows a clean branch tracking `origin/main` or only intentional unpushed commits.
- `pnpm check` exits 0.
- `git log --oneline -8` shows one commit per task in this plan.

Push after verification:

```bash
git push origin main
```

---

## Explicitly Out of Scope for This Plan

These are intentionally deferred to later milestone plans:

- SQLite persistence and SQLx migrations.
- Tauri 2 packaging.
- generated OpenAPI / generated TypeScript SDK.
- real LLM provider calls.
- worker protocol implementation.
- effect-typed tool execution.
- MCP/OpenAPI/CLI adapters.
- artifact content-addressed store.
- Tantivy or vector indexes.
- self-hosted server.
- authentication or device registration.
