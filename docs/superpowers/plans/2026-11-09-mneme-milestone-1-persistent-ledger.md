# Mneme Milestone 1 持久化账本与投影 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 Phase 1 的内存 Operation Ledger 升级为可持久化、可重放、可投影的本地 SQLite 数据面，并让 API、daemon、TypeScript client、Workbench 都能围绕 runs 投影工作。

**Architecture:** Kernel 继续拥有事件模型、账本接口、SQLite 存储与投影规则；API 只编排 Kernel 能力，不直接操作 SQL；daemon 负责选择本地数据库路径、初始化存储并注入 API。事件表仍是事实源，runs 表是可重建的读模型，所有投影必须能从事件流确定性 replay 得到。

**Tech Stack:** Rust 2021, Axum 0.7, Tokio 1.41, SQLx 0.8 + SQLite, Serde/serde_json, time, uuid, React + Vite + Vitest + TypeScript。

## Global Constraints

- 产品方向：Mneme = Personal AI Kernel（个人 AI 内核）。
- 当前运行形态：Desktop App + Local Daemon/API；不要引入云端依赖。
- Kernel 路线：Rust-first Kernel + TypeScript UI。
- 数据路线：append-only Operation Ledger 是事实源；SQLite 投影只作为可重建读模型。
- 接口路线：保留外部 HTTP/local API；内部 Kernel 接口必须可被 daemon 和未来 typed RPC/event bus 复用。
- API 兼容：保留 `GET /api/v1/health`、`GET /api/v1/events`、`POST /api/v1/runs`；新增 runs 读接口不能破坏现有 client。
- 验证命令：每个任务结束前至少运行该任务指定测试；整阶段最终运行 `pnpm check` 和 `git diff --check`。
- 提交节奏：每个任务用一个 git commit 结束，commit message 使用 Conventional Commits。
- 本计划只做 Milestone 1：SQLite 持久化 Operation Ledger、SQLx migrations、runs 投影、确定性 replay、UI 展示 runs；不实现 Agent run loop、模型执行、工具审批、Tauri 打包、自托管服务。

---

## Phase 1 之后的路线总览

1. **Milestone 1：持久化 Ledger 与投影**：SQLite 事件表、runs 投影表、replay 测试、API/UI 展示 runs。本计划详细覆盖。
2. **Milestone 2：Kernel 服务边界**：把“创建 run、追加事件、更新投影”收敛成 `RunService`，API 与未来内部 RPC 都调用服务层。
3. **Milestone 3：Agent run loop MVP**：支持一个无工具的 agent run 生命周期：started → model_requested → assistant_message → completed/failed。
4. **Milestone 4：Effect-typed tools 与审批**：定义工具 effect 分类、审批策略、ledger 记录 tool call/request/result。
5. **Milestone 5：Artifact 与 Context Inspector**：把 run 输出、上下文、附件变成可浏览、可引用的工作台对象。
6. **Milestone 6：Search/Memory/Knowledge**：围绕 ledger/artifact 做全文检索、摘要索引与可解释记忆。
7. **Milestone 7：Worker Runtime Protocol**：引入 Node/Python/Rust worker 协议，让 Kernel 成为控制面。
8. **Milestone 8：Tauri 桌面打包**：把本地 daemon 与 React Workbench 变成可安装桌面应用。
9. **Milestone 9：自托管个人服务器**：同步、备份、多设备访问与个人云端服务。

## File Structure

- `Cargo.toml`：新增 workspace 依赖 `async-trait`、`sqlx`。
- `crates/mneme-kernel/Cargo.toml`：接入 `async-trait`、`sqlx`、`serde_json`、Tokio 测试能力。
- `crates/mneme-kernel/migrations/0001_operation_ledger.sql`：SQLite schema，包含 `operation_events` 与 `runs`。
- `crates/mneme-kernel/src/ledger.rs`：把 `OperationLedger` 改成 async trait，引入 `SharedOperationLedger`，保留内存实现。
- `crates/mneme-kernel/src/ids.rs`：为 SQLite 反序列化补 `FromStr`，但继续保持 newtype 内部字段私有。
- `crates/mneme-kernel/src/storage/mod.rs`：存储模块出口。
- `crates/mneme-kernel/src/storage/sqlite.rs`：`SqliteOperationLedger`、migration 入口、事件序列化/反序列化。
- `crates/mneme-kernel/src/projections.rs`：纯函数 runs 投影模型与 deterministic replay。
- `crates/mneme-kernel/src/runs.rs`：`RunProjectionStore` trait、`InMemoryRunProjectionStore`、`SqliteRunProjectionStore`。
- `crates/mneme-kernel/src/events.rs`：补齐 `new_run_completed`、`new_run_failed` 构造函数。
- `crates/mneme-kernel/src/lib.rs`：导出新增模块和类型。
- `crates/mneme-api/src/types.rs`：新增 runs DTO。
- `crates/mneme-api/src/routes.rs`：API state 改为 shared ledger + run store；新增 `GET /api/v1/runs`。
- `crates/mneme-daemon/src/main.rs`：读取 `MNEME_DB_URL`，默认创建本地 SQLite 数据库。
- `apps/desktop/src/api/client.ts`：新增 `listRuns()` 和 runs 类型。
- `apps/desktop/src/api/client.test.ts`：覆盖 `listRuns()`。
- `apps/desktop/src/app/App.tsx`：展示 runs 列表和持久化状态文案。
- `apps/desktop/src/app/App.test.tsx`：覆盖 runs 列表渲染。
- `README.md`：更新持久化启动方式、数据库路径和验证命令。
- `docs/architecture/phase-1-foundation.md`：补一节说明 Phase 1 之后的数据面演进。
- `docs/architecture/milestone-1-persistent-ledger.md`：记录 Milestone 1 架构边界和 replay 规则。

---

### Task 1: 将 Ledger 边界改为异步共享接口

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/mneme-kernel/Cargo.toml`
- Modify: `crates/mneme-kernel/src/ledger.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`
- Modify: `crates/mneme-api/src/routes.rs`
- Modify: `crates/mneme-daemon/src/main.rs`

**Interfaces:**
- Consumes: `OperationEvent`, `RunId`, `InMemoryOperationLedger`。
- Produces: `pub type SharedOperationLedger = Arc<dyn OperationLedger>`；`#[async_trait] pub trait OperationLedger`；`impl InMemoryOperationLedger { pub fn shared(self) -> SharedOperationLedger }`。

- [ ] **Step 1: Write the failing compile/test change**

在 `crates/mneme-kernel/src/ledger.rs` 的测试中把调用改成 async：

```rust
#[tokio::test]
async fn ledger_appends_events_in_order() {
    let ledger = InMemoryOperationLedger::new();
    let first_run = RunId::new();
    let first = OperationEvent::new_run_started(first_run, "第一条指令");
    let second = OperationEvent::new_run_started(RunId::new(), "第二条指令");

    ledger.append(first.clone()).await.unwrap();
    ledger.append(second.clone()).await.unwrap();

    assert_eq!(ledger.list().await.unwrap(), vec![first, second]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-kernel ledger`

Expected: FAIL，出现类似 `Result<..., ...> is not a future` 或 `no method named await`，证明接口还不是 async。

- [ ] **Step 3: Add dependencies**

在根 `Cargo.toml` 的 `[workspace.dependencies]` 增加：

```toml
async-trait = "0.1"
```

在 `crates/mneme-kernel/Cargo.toml` 增加：

```toml
async-trait.workspace = true
tokio.workspace = true
```

- [ ] **Step 4: Implement async shared ledger**

把 `crates/mneme-kernel/src/ledger.rs` 的接口改成：

```rust
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use thiserror::Error;

use crate::{OperationEvent, RunId};

pub type SharedOperationLedger = Arc<dyn OperationLedger>;

#[async_trait]
pub trait OperationLedger: Send + Sync {
    async fn append(&self, event: OperationEvent) -> Result<(), LedgerError>;
    async fn list(&self) -> Result<Vec<OperationEvent>, LedgerError>;
    async fn list_for_run(&self, run_id: RunId) -> Result<Vec<OperationEvent>, LedgerError>;
}

#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("operation ledger lock is poisoned")]
    Poisoned,
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryOperationLedger {
    events: Arc<Mutex<Vec<OperationEvent>>>,
}

impl InMemoryOperationLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared(self) -> SharedOperationLedger {
        Arc::new(self)
    }
}

#[async_trait]
impl OperationLedger for InMemoryOperationLedger {
    async fn append(&self, event: OperationEvent) -> Result<(), LedgerError> {
        let mut events = self.events.lock().map_err(|_| LedgerError::Poisoned)?;
        events.push(event);
        Ok(())
    }

    async fn list(&self) -> Result<Vec<OperationEvent>, LedgerError> {
        let events = self.events.lock().map_err(|_| LedgerError::Poisoned)?;
        Ok(events.clone())
    }

    async fn list_for_run(&self, run_id: RunId) -> Result<Vec<OperationEvent>, LedgerError> {
        let events = self.events.lock().map_err(|_| LedgerError::Poisoned)?;
        Ok(events.iter().filter(|event| event.run_id == run_id).cloned().collect())
    }
}
```

- [ ] **Step 5: Update API/daemon call sites**

在 `crates/mneme-api/src/routes.rs` 中让 state 持有 `SharedOperationLedger`：

```rust
use mneme_kernel::{kernel_health, InMemoryOperationLedger, OperationEvent, OperationLedger, RunId, SharedOperationLedger};

#[derive(Clone)]
pub struct ApiState {
    pub ledger: SharedOperationLedger,
}

pub fn build_router(ledger: InMemoryOperationLedger) -> Router {
    build_router_with_ledger(ledger.shared())
}

pub fn build_router_with_ledger(ledger: SharedOperationLedger) -> Router {
    Router::new()
        .route("/api/v1/health", get(get_health))
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/runs", post(create_run))
        .with_state(ApiState { ledger })
}
```

并把 `state.ledger.list()`、`state.ledger.append(event)` 改成 `.await`。

- [ ] **Step 6: Run verification**

Run: `. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-kernel && cargo test -p mneme-api && cargo test --workspace`

Expected: PASS，`mneme-kernel` 和 `mneme-api` 原有测试全部通过。

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock crates/mneme-kernel crates/mneme-api crates/mneme-daemon
git commit -m "refactor: make operation ledger async and shared"
```

---

### Task 2: 添加 SQLite schema 与连接初始化

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/mneme-kernel/Cargo.toml`
- Create: `crates/mneme-kernel/migrations/0001_operation_ledger.sql`
- Create: `crates/mneme-kernel/src/storage/mod.rs`
- Create: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Produces: `pub struct SqliteStore`; `impl SqliteStore { pub async fn connect(database_url: &str) -> Result<Self, SqliteStoreError>; pub fn pool(&self) -> &SqlitePool; }`。

- [ ] **Step 1: Write failing SQLite initialization test**

创建 `crates/mneme-kernel/src/storage/sqlite.rs`，先写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sqlite_store_runs_migrations() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let count: i64 = sqlx::query_scalar("select count(*) from sqlite_master where type = 'table' and name = 'operation_events'")
            .fetch_one(store.pool())
            .await
            .unwrap();

        assert_eq!(count, 1);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-kernel sqlite_store_runs_migrations`

Expected: FAIL with unresolved `SqliteStore` / missing `sqlx` / missing migration。

- [ ] **Step 3: Add SQLx dependencies**

根 `Cargo.toml` 增加：

```toml
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "time", "uuid", "json", "migrate"] }
```

`crates/mneme-kernel/Cargo.toml` 增加：

```toml
sqlx.workspace = true
serde_json.workspace = true
```

- [ ] **Step 4: Add migration**

创建 `crates/mneme-kernel/migrations/0001_operation_ledger.sql`：

```sql
create table if not exists operation_events (
  sequence integer primary key autoincrement,
  event_id text not null unique,
  run_id text not null,
  occurred_at text not null,
  kind_type text not null,
  payload_json text not null
);

create index if not exists idx_operation_events_run_id_sequence
  on operation_events(run_id, sequence);

create table if not exists runs (
  run_id text primary key,
  status text not null,
  instruction text not null,
  started_at text not null,
  completed_at text,
  failed_at text,
  summary text,
  error text,
  updated_at text not null
);

create index if not exists idx_runs_updated_at
  on runs(updated_at desc);
```

- [ ] **Step 5: Implement store initialization**

`crates/mneme-kernel/src/storage/mod.rs`：

```rust
pub mod sqlite;

pub use sqlite::{SqliteStore, SqliteStoreError};
```

`crates/mneme-kernel/src/storage/sqlite.rs`：

```rust
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn connect(database_url: &str) -> Result<Self, SqliteStoreError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[derive(Debug, Error)]
pub enum SqliteStoreError {
    #[error("sqlite connection failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("sqlite migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}
```

`crates/mneme-kernel/src/lib.rs` 增加：

```rust
pub mod storage;

pub use storage::{SqliteStore, SqliteStoreError};
```

- [ ] **Step 6: Run verification**

Run: `. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-kernel sqlite_store_runs_migrations`

Expected: PASS。

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock crates/mneme-kernel/Cargo.toml crates/mneme-kernel/migrations crates/mneme-kernel/src
git commit -m "feat: add SQLite store migrations"
```

---

### Task 3: 实现 SQLite Operation Ledger

**Files:**
- Modify: `crates/mneme-kernel/src/ledger.rs`
- Modify: `crates/mneme-kernel/src/ids.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Produces: `pub struct SqliteOperationLedger`; `impl SqliteOperationLedger { pub fn new(store: SqliteStore) -> Self; pub fn shared(self) -> SharedOperationLedger; }`。

- [ ] **Step 1: Write failing persistence tests**

在 `crates/mneme-kernel/src/storage/sqlite.rs` 测试模块增加：

```rust
#[tokio::test]
async fn sqlite_ledger_persists_events_in_order() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let ledger = SqliteOperationLedger::new(store);
    let run_id = RunId::new();
    let first = OperationEvent::new_run_started(run_id, "第一条持久化指令");
    let second = OperationEvent::new_run_started(run_id, "第二条持久化指令");

    ledger.append(first.clone()).await.unwrap();
    ledger.append(second.clone()).await.unwrap();

    assert_eq!(ledger.list().await.unwrap(), vec![first, second]);
}

#[tokio::test]
async fn sqlite_ledger_filters_events_by_run() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let ledger = SqliteOperationLedger::new(store);
    let selected_run = RunId::new();
    let selected = OperationEvent::new_run_started(selected_run, "目标 run");
    let other = OperationEvent::new_run_started(RunId::new(), "其它 run");

    ledger.append(selected.clone()).await.unwrap();
    ledger.append(other).await.unwrap();

    assert_eq!(ledger.list_for_run(selected_run).await.unwrap(), vec![selected]);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-kernel sqlite_ledger`

Expected: FAIL with unresolved `SqliteOperationLedger`。

- [ ] **Step 3: Extend LedgerError**

在 `crates/mneme-kernel/src/ledger.rs` 增加：

```rust
#[error("operation ledger storage failed: {0}")]
Storage(String),
#[error("operation event payload is invalid: {0}")]
InvalidPayload(String),
```

- [ ] **Step 4: Add ID parsing helpers**

在 `crates/mneme-kernel/src/ids.rs` 增加 `FromStr`，让 SQLite 读取时可以安全重建强类型 ID：

```rust
use std::str::FromStr;

impl FromStr for EventId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

impl FromStr for RunId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}
```

- [ ] **Step 5: Implement SQLite ledger**

在 `crates/mneme-kernel/src/storage/sqlite.rs` 增加 `SqliteOperationLedger`，序列化策略如下：

```rust
#[derive(Clone, Debug)]
pub struct SqliteOperationLedger {
    store: SqliteStore,
}

impl SqliteOperationLedger {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub fn shared(self) -> SharedOperationLedger {
        Arc::new(self)
    }
}
```

`append` 写入：

```rust
let kind_type = match &event.kind {
    OperationEventKind::RunStarted { .. } => "run_started",
    OperationEventKind::RunCompleted { .. } => "run_completed",
    OperationEventKind::RunFailed { .. } => "run_failed",
};
let payload_json = serde_json::to_string(&event.kind)
    .map_err(|error| LedgerError::InvalidPayload(error.to_string()))?;

sqlx::query(
    "insert into operation_events (event_id, run_id, occurred_at, kind_type, payload_json) values (?1, ?2, ?3, ?4, ?5)",
)
.bind(event.id.to_string())
.bind(event.run_id.to_string())
.bind(event.occurred_at.format(&time::format_description::well_known::Rfc3339).unwrap())
.bind(kind_type)
.bind(payload_json)
.execute(self.store.pool())
.await
.map_err(|error| LedgerError::Storage(error.to_string()))?;
```

读取时按 `sequence asc`，把 `payload_json` 反序列化回 `OperationEventKind`，并用数据库列重建 `OperationEvent`。

- [ ] **Step 6: Export type**

`crates/mneme-kernel/src/storage/mod.rs` 导出：

```rust
pub use sqlite::{SqliteOperationLedger, SqliteStore, SqliteStoreError};
```

`crates/mneme-kernel/src/lib.rs` 导出：

```rust
pub use storage::{SqliteOperationLedger, SqliteStore, SqliteStoreError};
```

- [ ] **Step 7: Run verification**

Run: `. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-kernel sqlite_ledger && cargo test -p mneme-kernel ledger`

Expected: PASS。

- [ ] **Step 8: Commit**

```bash
git add crates/mneme-kernel/src Cargo.lock
git commit -m "feat: persist operation ledger events in SQLite"
```

---

### Task 4: 添加 runs 投影模型与 deterministic replay

**Files:**
- Modify: `crates/mneme-kernel/src/events.rs`
- Create: `crates/mneme-kernel/src/projections.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Produces: `RunStatus`, `RunProjection`, `RunProjector::replay(events: &[OperationEvent]) -> Vec<RunProjection>`。
- Produces: `OperationEvent::new_run_completed(run_id: RunId, summary: impl Into<String>) -> Self` 和 `OperationEvent::new_run_failed(run_id: RunId, reason: impl Into<String>) -> Self`。

- [ ] **Step 1: Write failing projection tests**

创建 `crates/mneme-kernel/src/projections.rs`：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OperationEvent, RunId};

    #[test]
    fn replay_builds_completed_run_projection() {
        let run_id = RunId::new();
        let events = vec![
            OperationEvent::new_run_started(run_id, "生成第一份计划"),
            OperationEvent::new_run_completed(run_id, "计划已生成"),
        ];

        let runs = RunProjector::replay(&events);

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id, run_id);
        assert_eq!(runs[0].status, RunStatus::Completed);
        assert_eq!(runs[0].instruction, "生成第一份计划");
        assert_eq!(runs[0].summary.as_deref(), Some("计划已生成"));
    }

    #[test]
    fn replay_is_deterministic_for_same_event_stream() {
        let run_id = RunId::new();
        let events = vec![OperationEvent::new_run_started(run_id, "稳定 replay")];

        assert_eq!(RunProjector::replay(&events), RunProjector::replay(&events));
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-kernel replay_`

Expected: FAIL with unresolved `RunProjector` / `new_run_completed`。

- [ ] **Step 3: Add event constructors**

在 `crates/mneme-kernel/src/events.rs` 的 `impl OperationEvent` 中增加：

```rust
pub fn new_run_completed(run_id: RunId, summary: impl Into<String>) -> Self {
    Self {
        id: EventId::new(),
        run_id,
        occurred_at: OffsetDateTime::now_utc(),
        kind: OperationEventKind::RunCompleted { summary: summary.into() },
    }
}

pub fn new_run_failed(run_id: RunId, reason: impl Into<String>) -> Self {
    Self {
        id: EventId::new(),
        run_id,
        occurred_at: OffsetDateTime::now_utc(),
        kind: OperationEventKind::RunFailed { reason: reason.into() },
    }
}
```

- [ ] **Step 4: Implement projections**

`crates/mneme-kernel/src/projections.rs`：

```rust
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{OperationEvent, OperationEventKind, RunId};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunProjection {
    pub run_id: RunId,
    pub status: RunStatus,
    pub instruction: String,
    pub started_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
    pub failed_at: Option<OffsetDateTime>,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub updated_at: OffsetDateTime,
}

pub struct RunProjector;

impl RunProjector {
    pub fn replay(events: &[OperationEvent]) -> Vec<RunProjection> {
        let mut runs = Vec::<RunProjection>::new();

        for event in events {
            match &event.kind {
                OperationEventKind::RunStarted { instruction } => {
                    runs.push(RunProjection {
                        run_id: event.run_id,
                        status: RunStatus::Running,
                        instruction: instruction.clone(),
                        started_at: event.occurred_at,
                        completed_at: None,
                        failed_at: None,
                        summary: None,
                        error: None,
                        updated_at: event.occurred_at,
                    });
                }
                OperationEventKind::RunCompleted { summary } => {
                    if let Some(run) = runs.iter_mut().find(|run| run.run_id == event.run_id) {
                        run.status = RunStatus::Completed;
                        run.completed_at = Some(event.occurred_at);
                        run.summary = Some(summary.clone());
                        run.updated_at = event.occurred_at;
                    }
                }
                OperationEventKind::RunFailed { reason } => {
                    if let Some(run) = runs.iter_mut().find(|run| run.run_id == event.run_id) {
                        run.status = RunStatus::Failed;
                        run.failed_at = Some(event.occurred_at);
                        run.error = Some(reason.clone());
                        run.updated_at = event.occurred_at;
                    }
                }
            }
        }

        runs.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        runs
    }
}
```

- [ ] **Step 5: Export projections**

`crates/mneme-kernel/src/lib.rs` 增加：

```rust
pub mod projections;

pub use projections::{RunProjection, RunProjector, RunStatus};
```

- [ ] **Step 6: Run verification**

Run: `. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-kernel replay_ && cargo test -p mneme-kernel events`

Expected: PASS。

- [ ] **Step 7: Commit**

```bash
git add crates/mneme-kernel/src
git commit -m "feat: add run projection replay"
```

---

### Task 5: 实现 runs 投影表与 projection store

**Files:**
- Create: `crates/mneme-kernel/src/runs.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Produces: `pub type SharedRunProjectionStore = Arc<dyn RunProjectionStore>`。
- Produces: `#[async_trait] pub trait RunProjectionStore { async fn apply(&self, event: &OperationEvent) -> Result<(), RunProjectionError>; async fn rebuild(&self, events: &[OperationEvent]) -> Result<(), RunProjectionError>; async fn list_runs(&self) -> Result<Vec<RunProjection>, RunProjectionError>; }`。
- Produces: `InMemoryRunProjectionStore` and `SqliteRunProjectionStore`。

- [ ] **Step 1: Write failing run store tests**

创建 `crates/mneme-kernel/src/runs.rs` 测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OperationEvent, RunId};

    #[tokio::test]
    async fn in_memory_run_store_applies_started_event() {
        let store = InMemoryRunProjectionStore::new();
        let run_id = RunId::new();
        let event = OperationEvent::new_run_started(run_id, "记录一次 run");

        store.apply(&event).await.unwrap();

        let runs = store.list_runs().await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id, run_id);
        assert_eq!(runs[0].status, RunStatus::Running);
    }
}
```

在 `crates/mneme-kernel/src/storage/sqlite.rs` 测试模块增加：

```rust
#[tokio::test]
async fn sqlite_run_store_persists_projection() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let runs = SqliteRunProjectionStore::new(store);
    let run_id = RunId::new();
    let event = OperationEvent::new_run_started(run_id, "SQLite run 投影");

    runs.apply(&event).await.unwrap();

    let listed = runs.list_runs().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].run_id, run_id);
    assert_eq!(listed[0].instruction, "SQLite run 投影");
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-kernel run_store`

Expected: FAIL with unresolved run store types。

- [ ] **Step 3: Implement in-memory store**

`crates/mneme-kernel/src/runs.rs` 使用 `Arc<Mutex<Vec<RunProjection>>>`。`apply()` 的最小正确实现：取现有 runs，附加事件到一个临时事件列表不可行；因此直接按事件类型更新单个 run。规则必须和 `RunProjector::replay()` 一致：started 插入 running；completed/failed 更新已有 run；未知 run 的 completed/failed 不创建 projection。

- [ ] **Step 4: Implement SQLite store**

`SqliteRunProjectionStore::apply()` 对 `RunStarted` 使用 upsert：

```sql
insert into runs (run_id, status, instruction, started_at, completed_at, failed_at, summary, error, updated_at)
values (?1, 'running', ?2, ?3, null, null, null, null, ?3)
on conflict(run_id) do update set
  status = excluded.status,
  instruction = excluded.instruction,
  started_at = excluded.started_at,
  updated_at = excluded.updated_at
```

`RunCompleted` 更新 `status = 'completed'`、`completed_at`、`summary`、`updated_at`。`RunFailed` 更新 `status = 'failed'`、`failed_at`、`error`、`updated_at`。

- [ ] **Step 5: Implement rebuild**

`rebuild()` 先清空 `runs`，再用 `RunProjector::replay(events)` 的结果批量写入。测试增加：

```rust
#[tokio::test]
async fn sqlite_run_store_rebuild_matches_projector_replay() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let runs = SqliteRunProjectionStore::new(store);
    let run_id = RunId::new();
    let events = vec![
        OperationEvent::new_run_started(run_id, "重建投影"),
        OperationEvent::new_run_completed(run_id, "完成"),
    ];

    runs.rebuild(&events).await.unwrap();

    assert_eq!(runs.list_runs().await.unwrap(), RunProjector::replay(&events));
}
```

- [ ] **Step 6: Export run store types**

`crates/mneme-kernel/src/lib.rs` 增加：

```rust
pub mod runs;

pub use runs::{
    InMemoryRunProjectionStore, RunProjectionError, RunProjectionStore, SharedRunProjectionStore,
    SqliteRunProjectionStore,
};
```

- [ ] **Step 7: Run verification**

Run: `. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-kernel run_store && cargo test -p mneme-kernel sqlite_run_store`

Expected: PASS。

- [ ] **Step 8: Commit**

```bash
git add crates/mneme-kernel/src Cargo.lock
git commit -m "feat: add run projection stores"
```

---

### Task 6: API 增加 runs 读接口并维护投影

**Files:**
- Modify: `crates/mneme-api/src/types.rs`
- Modify: `crates/mneme-api/src/routes.rs`
- Modify: `crates/mneme-daemon/src/main.rs`

**Interfaces:**
- Produces: `GET /api/v1/runs`。
- Produces: `RunsResponse { runs: Vec<RunProjection> }`。
- Updates: `POST /api/v1/runs` append event 后必须调用 `run_store.apply(&event)`。

- [ ] **Step 1: Write failing API route tests**

在 `crates/mneme-api/src/routes.rs` 测试模块增加：

```rust
#[tokio::test]
async fn list_runs_returns_created_run_projection() {
    let ledger = InMemoryOperationLedger::new();
    let run_store = InMemoryRunProjectionStore::new();
    let app = build_router_with_state(ledger.shared(), run_store.shared());

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"instruction":"展示 runs"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let response = app
        .oneshot(Request::builder().uri("/api/v1/runs").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: RunsResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload.runs.len(), 1);
    assert_eq!(payload.runs[0].instruction, "展示 runs");
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-api list_runs_returns_created_run_projection`

Expected: FAIL with missing route/type/state。

- [ ] **Step 3: Add DTO**

`crates/mneme-api/src/types.rs`：

```rust
use mneme_kernel::{OperationEvent, RunProjection};

#[derive(Debug, Deserialize, Serialize)]
pub struct RunsResponse {
    pub runs: Vec<RunProjection>,
}
```

- [ ] **Step 4: Update API state and routes**

`crates/mneme-api/src/routes.rs` 的 state：

```rust
#[derive(Clone)]
pub struct ApiState {
    pub ledger: SharedOperationLedger,
    pub run_store: SharedRunProjectionStore,
}

pub fn build_router_with_state(
    ledger: SharedOperationLedger,
    run_store: SharedRunProjectionStore,
) -> Router {
    Router::new()
        .route("/api/v1/health", get(get_health))
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/runs", get(list_runs).post(create_run))
        .with_state(ApiState { ledger, run_store })
}
```

`create_run` 在 append 成功后调用：

```rust
state
    .run_store
    .apply(&event)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
```

`list_runs`：

```rust
async fn list_runs(State(state): State<ApiState>) -> Result<Json<RunsResponse>, StatusCode> {
    let runs = state
        .run_store
        .list_runs()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(RunsResponse { runs }))
}
```

- [ ] **Step 5: Preserve compatibility constructor**

保留 Phase 1 的 `build_router(ledger: InMemoryOperationLedger) -> Router`，内部创建 `InMemoryRunProjectionStore::new().shared()`，避免旧测试和调用点大幅变动。

- [ ] **Step 6: Run verification**

Run: `. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-api && cargo test --workspace`

Expected: PASS，现有 health/events/create-run 测试继续通过。

- [ ] **Step 7: Commit**

```bash
git add crates/mneme-api crates/mneme-daemon crates/mneme-kernel/src
git commit -m "feat: expose run projections through API"
```

---

### Task 7: daemon 启用 SQLite 持久化数据面

**Files:**
- Modify: `crates/mneme-daemon/src/main.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: `SqliteStore::connect(database_url)`, `SqliteOperationLedger::new(store.clone())`, `SqliteRunProjectionStore::new(store)`。
- Produces: env `MNEME_DB_URL`；默认值 `sqlite://.mneme/mneme.db?mode=rwc`。

- [ ] **Step 1: Write failing daemon configuration test**

如果当前 `main.rs` 没有可测试函数，先抽出：

```rust
fn database_url_from_env() -> String {
    std::env::var("MNEME_DB_URL").unwrap_or_else(|_| "sqlite://.mneme/mneme.db?mode=rwc".to_string())
}
```

测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_database_url_uses_local_mneme_directory() {
        std::env::remove_var("MNEME_DB_URL");
        assert_eq!(database_url_from_env(), "sqlite://.mneme/mneme.db?mode=rwc");
    }
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-daemon database_url_from_env`

Expected: FAIL if function does not exist or still uses memory-only setup。

- [ ] **Step 3: Update daemon startup**

`main()` 初始化：

```rust
let database_url = database_url_from_env();
ensure_sqlite_parent_directory(&database_url)?;
let store = SqliteStore::connect(&database_url).await?;
let ledger = SqliteOperationLedger::new(store.clone()).shared();
let run_store = SqliteRunProjectionStore::new(store).shared();
let app = mneme_api::build_router_with_state(ledger, run_store);
```

`ensure_sqlite_parent_directory()` 只处理默认相对路径 `.mneme/mneme.db`，创建 `.mneme` 目录；`sqlite::memory:` 不创建目录。

- [ ] **Step 4: Manual persistence verification**

Run:

```bash
. "$HOME/.cargo/env"
rm -rf .mneme-test
MNEME_DB_URL='sqlite://.mneme-test/mneme.db?mode=rwc' cargo run -p mneme-daemon &
DAEMON_PID=$!
sleep 2
curl -s -X POST http://127.0.0.1:3766/api/v1/runs \
  -H 'content-type: application/json' \
  -d '{"instruction":"验证持久化 run"}'
curl -s http://127.0.0.1:3766/api/v1/runs
kill "$DAEMON_PID"
```

Expected: 第二个 curl 返回 `runs` 数组，其中包含 `"instruction":"验证持久化 run"`。

- [ ] **Step 5: Update README**

`README.md` 增加：

````markdown
### 本地 SQLite 数据库

默认 daemon 使用 `sqlite://.mneme/mneme.db?mode=rwc`。如需隔离测试数据：

```bash
MNEME_DB_URL='sqlite://.mneme-dev/mneme.db?mode=rwc' cargo run -p mneme-daemon
```

Operation Ledger 的 `operation_events` 表是事实源；`runs` 表是可重建投影。
````

- [ ] **Step 6: Run verification**

Run: `. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-daemon && cargo test --workspace`

Expected: PASS。

- [ ] **Step 7: Commit**

```bash
git add crates/mneme-daemon README.md Cargo.lock
git commit -m "feat: persist daemon data in SQLite"
```

---

### Task 8: TypeScript client 与 Workbench 展示 runs 投影

**Files:**
- Modify: `apps/desktop/src/api/client.ts`
- Modify: `apps/desktop/src/api/client.test.ts`
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Modify: `apps/desktop/src/styles.css`

**Interfaces:**
- Produces: `RunStatus`, `RunProjection`, `RunsResponse` TypeScript types。
- Produces: `MnemeApiClient.listRuns(): Promise<RunsResponse>`。

- [ ] **Step 1: Write failing client test**

`apps/desktop/src/api/client.test.ts` 增加：

```ts
it('lists run projections', async () => {
  const fetchImpl = vi.fn(async () =>
    new Response(
      JSON.stringify({
        runs: [
          {
            run_id: 'run-1',
            status: 'running',
            instruction: '展示 runs 投影',
            started_at: '2026-11-09T00:00:00Z',
            completed_at: null,
            failed_at: null,
            summary: null,
            error: null,
            updated_at: '2026-11-09T00:00:00Z',
          },
        ],
      }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    ),
  );

  const client = createMnemeApiClient('http://127.0.0.1:3766/', fetchImpl);

  await expect(client.listRuns()).resolves.toEqual({
    runs: [expect.objectContaining({ instruction: '展示 runs 投影', status: 'running' })],
  });
  expect(fetchImpl).toHaveBeenCalledWith('http://127.0.0.1:3766/api/v1/runs', expect.any(Object));
});
```

- [ ] **Step 2: Run test to verify failure**

Run: `pnpm --dir apps/desktop test -- src/api/client.test.ts`

Expected: FAIL with `client.listRuns is not a function`。

- [ ] **Step 3: Implement client types and method**

`apps/desktop/src/api/client.ts` 增加：

```ts
export type RunStatus = 'running' | 'completed' | 'failed';

export interface RunProjection {
  run_id: string;
  status: RunStatus;
  instruction: string;
  started_at: string;
  completed_at: string | null;
  failed_at: string | null;
  summary: string | null;
  error: string | null;
  updated_at: string;
}

export interface RunsResponse {
  runs: RunProjection[];
}
```

`MnemeApiClient` 增加：

```ts
listRuns(): Promise<RunsResponse>;
```

实现：

```ts
async listRuns() {
  return requestJson<RunsResponse>(`${normalizedBaseUrl}/api/v1/runs`, fetchImpl);
}
```

- [ ] **Step 4: Write failing App test**

`apps/desktop/src/app/App.test.tsx` 更新 fake client，断言页面展示 instruction：

```tsx
expect(await screen.findByText('展示 runs 投影')).toBeInTheDocument();
expect(screen.getByText('running')).toBeInTheDocument();
```

- [ ] **Step 5: Implement UI**

`App.tsx` 从 `Promise.all([client.getHealth(), client.listEvents()])` 改为 `Promise.all([client.getHealth(), client.listEvents(), client.listRuns()])`。页面保留 event count，同时新增：

```tsx
<section className="panel">
  <h2>Runs</h2>
  {runs.length === 0 ? (
    <p className="muted">还没有持久化 run。通过 API 创建 run 后会显示在这里。</p>
  ) : (
    <ul className="run-list">
      {runs.map((run) => (
        <li key={run.run_id} className="run-card">
          <span className="run-status">{run.status}</span>
          <strong>{run.instruction}</strong>
          <small>{run.updated_at}</small>
        </li>
      ))}
    </ul>
  )}
</section>
```

- [ ] **Step 6: Run verification**

Run: `pnpm --dir apps/desktop check && pnpm --dir apps/desktop test && pnpm check`

Expected: PASS，2 个 test files 全部通过。

- [ ] **Step 7: Commit**

```bash
git add apps/desktop package.json pnpm-lock.yaml
git commit -m "feat: show run projections in workbench"
```

---

### Task 9: 文档、架构说明与最终验证

**Files:**
- Create: `docs/architecture/milestone-1-persistent-ledger.md`
- Modify: `docs/architecture/phase-1-foundation.md`
- Modify: `README.md`

**Interfaces:**
- Produces: documented validation and architecture boundary for Milestone 1。

- [ ] **Step 1: Write architecture document**

创建 `docs/architecture/milestone-1-persistent-ledger.md`：

````markdown
# Milestone 1：持久化 Operation Ledger 与 runs 投影

## 边界

Milestone 1 把 Phase 1 的内存账本升级为 SQLite 本地数据面。它不执行模型、不调用工具、不实现审批。

## 数据流

React Workbench → TypeScript API Client → HTTP `/api/v1/*` → `mneme-api` → `mneme-kernel`

Kernel 内部：

1. `OperationEvent` 追加写入 `operation_events`。
2. `RunProjectionStore` 根据同一事件更新 `runs`。
3. `RunProjector::replay()` 可以从事件流重建 `runs`，用于迁移、修复和测试。

## 表职责

- `operation_events`：事实源，append-only。
- `runs`：读模型，可删除后由 replay 重建。

## 验证

```bash
pnpm check
git diff --check
```
````

- [ ] **Step 2: Update README runbook**

确认 README 包含：

````markdown
## 常用验证

```bash
pnpm check
git diff --check
```

## 创建一条 run

```bash
curl -s -X POST http://127.0.0.1:3766/api/v1/runs \
  -H 'content-type: application/json' \
  -d '{"instruction":"整理今天的工作"}'

curl -s http://127.0.0.1:3766/api/v1/runs
```
````

- [ ] **Step 3: Update Phase 1 architecture bridge**

在 `docs/architecture/phase-1-foundation.md` 增加一节：

```markdown
## Phase 1 之后的数据面演进

Milestone 1 在不改变外部 API 基本形状的前提下，把内存 ledger 替换为 SQLite ledger，并加入 runs 投影。Phase 1 的 API route、daemon、TypeScript client 和 Workbench 会继续存在，只是数据来源从内存对象变成可持久化 Kernel 数据面。
```

- [ ] **Step 4: Run final verification**

Run:

```bash
. "$HOME/.cargo/env"
pnpm check
git diff --check
git status --short --branch
```

Expected:

```text
pnpm check exits 0
git diff --check exits 0
git status shows only the current branch after commit
```

- [ ] **Step 5: Commit**

```bash
git add README.md docs/architecture
git commit -m "docs: document persistent ledger milestone"
```

---

## Self-Review

**Spec coverage:** 本计划覆盖 Phase 1 路线图中的 Milestone 1：SQLite 追加式事件表、SQLx migrations、runs 投影表、确定性 replay 测试。它也明确排除了 agent run loop、工具审批、Tauri 和自托管服务器，避免范围膨胀。

**Type consistency:** `SharedOperationLedger`、`SharedRunProjectionStore`、`RunProjection`、`RunStatus`、`RunsResponse` 在 Kernel、API、TS client、Workbench 之间命名一致。API 保留旧 `build_router(ledger: InMemoryOperationLedger) -> Router`，新增 `build_router_with_state(...)` 给 SQLite daemon 使用。

**Execution recommendation:** 使用 subagent-driven-development。每个 Task 都能独立 TDD、验证、提交、审查；Task 1–3 是存储基础，Task 4–6 是投影/API，Task 7–9 是 daemon/UI/docs 收口。
