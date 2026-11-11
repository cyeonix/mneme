# Mneme Milestone 9 Self-hosted Multi-device Sync Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a self-hosted Mneme server with real multi-device event/object synchronization, device registration, explicit sync cursors, backup/restore, and a minimal remote overview.

**Architecture:** Add sync domain types to `mneme-kernel`, then add a new `mneme-server` crate with SQLite-backed device registry, append-only event sync, LWW object metadata sync, backup/restore, and Axum routes. Add a small reusable `mneme-sync` HTTP client crate so local daemon/desktop can later integrate without coupling directly to server internals.

**Tech Stack:** Rust 2021, Tokio, async-trait, serde/serde_json, SQLx SQLite, Axum, reqwest rustls, time, uuid, thiserror, existing pnpm workspace verification.

## Global Constraints

- M9 scope is **Real multi-device sync first**.
- Conflict policy is **Append-only ledger + LWW projections**.
- Operation Ledger events remain immutable and are deduplicated by `event_id`.
- Object metadata is keyed by `(object_kind, object_id)` and visible conflicts resolve with `winner = max(updated_at, origin_device_id)`.
- Local desktop and `mneme-daemon` remain local-first authorities; `mneme-server` is self-hosted sync coordinator, not an official cloud.
- M8 desktop secret storage boundaries must not be weakened; provider API keys and device tokens are not written to Operation Ledger, local SQLite projections, frontend storage, docs examples, or logs.
- Tests must run offline and use fake tokens / in-memory or local SQLite test stores.
- M9 does not implement official hosted accounts, OAuth, multi-tenant permissions, mobile app, end-to-end encrypted sync protocol, CRDTs, remote tool execution, remote provider-key storage, or production deployment hardening.
- Source Rust toolchain before Rust commands: `. "$HOME/.cargo/env"`.
- Each task produces one focused commit and must leave the worktree clean.

---

## File Structure

```text
crates/mneme-kernel/src/ids.rs
  Add DeviceId, SyncBatchId, BackupId UUID newtypes.

crates/mneme-kernel/src/sync.rs
  M9 sync domain records, request/response DTO-shaped records, LWW helpers, token redaction helpers.

crates/mneme-kernel/src/lib.rs
  Export M9 sync IDs and domain records.

crates/mneme-server/Cargo.toml
crates/mneme-server/src/lib.rs
crates/mneme-server/src/storage.rs
crates/mneme-server/src/service.rs
crates/mneme-server/src/routes.rs
crates/mneme-server/src/main.rs
crates/mneme-server/migrations/0001_sync_server.sql
  Self-hosted server crate: SQLite store, sync service, Axum router, binary entrypoint.

crates/mneme-sync/Cargo.toml
crates/mneme-sync/src/lib.rs
  Reusable client boundary and reqwest HTTP implementation for later daemon/desktop integration.

Cargo.toml
Cargo.lock
  Add `crates/mneme-server` and `crates/mneme-sync` workspace members and dependency lockfile updates.

README.md
docs/architecture/milestone-9-self-hosted-sync-server.md
docs/architecture/phase-1-foundation.md
  M9 docs and Phase 1 closeout.
```

---

### Task 1: Kernel sync IDs and domain records

**Files:**
- Modify: `crates/mneme-kernel/src/ids.rs`
- Create: `crates/mneme-kernel/src/sync.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: existing `EventId`, `RunId`, and UUID newtype pattern in `crates/mneme-kernel/src/ids.rs`.
- Produces: `DeviceId`, `SyncBatchId`, `BackupId`, `DeviceRecord`, `SyncCursorRecord`, `SyncedEventRecord`, `SyncObjectKind`, `SyncedObjectRecord`, `BackupRecord`, `RegisterDeviceRequest`, `RegisterDeviceResponse`, `PushEventsRequest`, `PushEventsResponse`, `PullEventsRequest`, `PullEventsResponse`, `AckEventsRequest`, `PushObjectsRequest`, `PushObjectsResponse`, `PullObjectsRequest`, `PullObjectsResponse`, `ServerOverviewResponse`, `lww_object_winner(left, right) -> SyncedObjectRecord`.

- [ ] **Step 1: Add failing ID tests**

Add tests to `crates/mneme-kernel/src/ids.rs`:

```rust
#[test]
fn generated_sync_ids_are_displayable_and_parseable() {
    let device_id = DeviceId::new();
    let sync_batch_id = SyncBatchId::new();
    let backup_id = BackupId::new();

    assert_eq!(DeviceId::from_str(&device_id.to_string()).unwrap(), device_id);
    assert_eq!(SyncBatchId::from_str(&sync_batch_id.to_string()).unwrap(), sync_batch_id);
    assert_eq!(BackupId::from_str(&backup_id.to_string()).unwrap(), backup_id);
}
```

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-kernel generated_sync_ids_are_displayable_and_parseable`

Expected: FAIL with missing `DeviceId`, `SyncBatchId`, or `BackupId`.

- [ ] **Step 2: Add failing sync record tests**

Create `crates/mneme-kernel/src/sync.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EventId, RunId};
    use serde_json::json;
    use time::macros::datetime;

    #[test]
    fn lww_object_winner_uses_updated_at_then_origin_device_id() {
        let run_id = RunId::new();
        let left_device = DeviceId::from_str("00000000-0000-4000-8000-000000000001").unwrap();
        let right_device = DeviceId::from_str("00000000-0000-4000-8000-000000000002").unwrap();
        let older = SyncedObjectRecord {
            object_id: "artifact-1".to_string(),
            object_kind: SyncObjectKind::Artifact,
            origin_device_id: left_device,
            run_id,
            updated_at: datetime!(2026-01-01 00:00 UTC),
            payload_json: json!({ "title": "older" }),
            received_at: datetime!(2026-01-01 00:01 UTC),
        };
        let newer = SyncedObjectRecord {
            updated_at: datetime!(2026-01-02 00:00 UTC),
            payload_json: json!({ "title": "newer" }),
            ..older.clone()
        };

        assert_eq!(lww_object_winner(older.clone(), newer.clone()), newer);

        let same_time_left = SyncedObjectRecord {
            origin_device_id: left_device,
            updated_at: datetime!(2026-01-03 00:00 UTC),
            payload_json: json!({ "title": "left" }),
            ..older.clone()
        };
        let same_time_right = SyncedObjectRecord {
            origin_device_id: right_device,
            updated_at: datetime!(2026-01-03 00:00 UTC),
            payload_json: json!({ "title": "right" }),
            ..older
        };

        assert_eq!(lww_object_winner(same_time_left, same_time_right.clone()), same_time_right);
    }

    #[test]
    fn synced_event_record_preserves_event_identity_and_origin_device() {
        let device_id = DeviceId::new();
        let event_id = EventId::new();
        let record = SyncedEventRecord {
            server_sequence: 42,
            event_id,
            origin_device_id: device_id,
            run_id: RunId::new(),
            occurred_at: time::OffsetDateTime::now_utc(),
            kind_type: "run_started".to_string(),
            payload_json: json!({ "instruction": "hello" }),
            received_at: time::OffsetDateTime::now_utc(),
        };

        assert_eq!(record.event_id, event_id);
        assert_eq!(record.origin_device_id, device_id);
    }
}
```

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-kernel sync`

Expected: FAIL with missing sync types/functions.

- [ ] **Step 3: Implement ID newtypes**

In `crates/mneme-kernel/src/ids.rs`, follow the existing UUID newtype pattern and add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SyncBatchId(Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BackupId(Uuid);
```

Add `new()`, `Default`, `Display`, and `FromStr` implementations matching existing ID types.

- [ ] **Step 4: Implement sync records and LWW helper**

In `crates/mneme-kernel/src/sync.rs`, implement all records from the Interfaces block. Use `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]` for records and `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]` for `SyncObjectKind`.

Implement:

```rust
pub fn lww_object_winner(left: SyncedObjectRecord, right: SyncedObjectRecord) -> SyncedObjectRecord {
    if right.updated_at > left.updated_at {
        return right;
    }
    if left.updated_at > right.updated_at {
        return left;
    }
    if right.origin_device_id.to_string() > left.origin_device_id.to_string() {
        right
    } else {
        left
    }
}
```

Add DTO-style request/response structs exactly named in the Interfaces block.

- [ ] **Step 5: Export sync module**

In `crates/mneme-kernel/src/lib.rs`, add:

```rust
pub mod sync;

pub use sync::{
    lww_object_winner, AckEventsRequest, BackupRecord, DeviceRecord, PullEventsRequest,
    PullEventsResponse, PullObjectsRequest, PullObjectsResponse, PushEventsRequest,
    PushEventsResponse, PushObjectsRequest, PushObjectsResponse, RegisterDeviceRequest,
    RegisterDeviceResponse, ServerOverviewResponse, SyncCursorRecord, SyncObjectKind,
    SyncedEventRecord, SyncedObjectRecord,
};
```

Also export `DeviceId`, `SyncBatchId`, and `BackupId` from the existing ID export list.

- [ ] **Step 6: Verify and commit**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check
. "$HOME/.cargo/env" && cargo test -p mneme-kernel sync
. "$HOME/.cargo/env" && cargo test -p mneme-kernel generated_sync_ids_are_displayable_and_parseable
git diff --check
git status --short
```

Expected: all commands exit 0, status shows only intended files before commit.

Commit:

```bash
git add crates/mneme-kernel/src/ids.rs crates/mneme-kernel/src/sync.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add sync domain records"
```

---

### Task 2: Server crate, SQLite schema, device registration, and cursors

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `crates/mneme-server/Cargo.toml`
- Create: `crates/mneme-server/migrations/0001_sync_server.sql`
- Create: `crates/mneme-server/src/lib.rs`
- Create: `crates/mneme-server/src/storage.rs`
- Create: `crates/mneme-server/src/service.rs`
- Create: `crates/mneme-server/src/main.rs`

**Interfaces:**
- Consumes: Task 1 sync domain records.
- Produces: `SyncServerStore`, `SqliteSyncServerStore`, `SyncServerService`, `SyncServerError`, `DeviceRegistrationOutcome`, `register_device`, `list_devices`, `list_cursors`, `ack_events`.

- [ ] **Step 1: Add workspace member and crate skeleton**

Update root `Cargo.toml` members with:

```toml
  "crates/mneme-server",
```

Create `crates/mneme-server/Cargo.toml`:

```toml
[package]
name = "mneme-server"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
async-trait.workspace = true
axum.workspace = true
mneme-kernel = { path = "../mneme-kernel" }
serde.workspace = true
serde_json.workspace = true
sqlx.workspace = true
thiserror.workspace = true
time.workspace = true
tokio.workspace = true
tracing.workspace = true
uuid.workspace = true
```

Create `src/lib.rs` with module declarations:

```rust
pub mod service;
pub mod storage;
```

Create minimal `src/main.rs`:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
```

- [ ] **Step 2: Write failing device registration tests**

In `crates/mneme-server/src/service.rs`, add tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::SqliteSyncServerStore;

    #[tokio::test]
    async fn service_registers_device_and_initial_cursor() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:").await.unwrap();
        let service = SyncServerService::new(store);

        let registered = service.register_device("Laptop".to_string(), Some("fp-1".to_string())).await.unwrap();
        assert_eq!(registered.device.display_name, "Laptop");
        assert_eq!(registered.cursor.last_seen_server_sequence, 0);
        assert!(!registered.device_token.is_empty());

        let devices = service.list_devices().await.unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_id, registered.device.device_id);
    }

    #[tokio::test]
    async fn service_rejects_blank_device_name() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:").await.unwrap();
        let service = SyncServerService::new(store);

        assert!(matches!(
            service.register_device("   ".to_string(), None).await.unwrap_err(),
            SyncServerError::InvalidInput
        ));
    }
}
```

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-server register_device`

Expected: FAIL with missing store/service types.

- [ ] **Step 3: Add SQLite migration**

Create `crates/mneme-server/migrations/0001_sync_server.sql` with `devices`, `sync_cursors`, `synced_events`, `synced_objects`, and `backups` tables from the spec. Include indexes `idx_synced_events_origin_device_sequence` and `idx_synced_objects_run_kind_updated`.

- [ ] **Step 4: Implement store and service**

In `storage.rs`, implement:

```rust
#[derive(Clone)]
pub struct SqliteSyncServerStore {
    pool: sqlx::SqlitePool,
}

impl SqliteSyncServerStore {
    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = sqlx::SqlitePool::connect(database_url).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }
}
```

In `service.rs`, implement:

```rust
#[derive(Debug, thiserror::Error)]
pub enum SyncServerError {
    #[error("invalid input")]
    InvalidInput,
    #[error("unknown device")]
    UnknownDevice,
    #[error("storage error: {0}")]
    Storage(String),
}

pub struct DeviceRegistrationOutcome {
    pub device: mneme_kernel::DeviceRecord,
    pub cursor: mneme_kernel::SyncCursorRecord,
    pub device_token: String,
}

#[derive(Clone)]
pub struct SyncServerService {
    store: SqliteSyncServerStore,
}
```

Add methods:

```rust
impl SyncServerService {
    pub fn new(store: SqliteSyncServerStore) -> Self;
    pub async fn register_device(&self, display_name: String, public_key_fingerprint: Option<String>) -> Result<DeviceRegistrationOutcome, SyncServerError>;
    pub async fn list_devices(&self) -> Result<Vec<DeviceRecord>, SyncServerError>;
    pub async fn list_cursors(&self) -> Result<Vec<SyncCursorRecord>, SyncServerError>;
    pub async fn ack_events(&self, device_id: DeviceId, last_seen_server_sequence: i64) -> Result<SyncCursorRecord, SyncServerError>;
}
```

Use fake token format for M9 tests:

```rust
let device_token = format!("mneme-dev-token-{}", device_id);
```

Do not store the token in plaintext in the schema in this task.

- [ ] **Step 5: Verify and commit**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check
. "$HOME/.cargo/env" && cargo test -p mneme-server register_device
. "$HOME/.cargo/env" && cargo test -p mneme-server
. "$HOME/.cargo/env" && cargo check --workspace
git diff --check
git status --short
```

Commit:

```bash
git add Cargo.toml Cargo.lock crates/mneme-server
git commit -m "feat: add sync server device registry"
```

---

### Task 3: Event push, pull, ack, and two-device sync

**Files:**
- Modify: `crates/mneme-server/src/service.rs`
- Modify: `crates/mneme-server/src/storage.rs`

**Interfaces:**
- Consumes: `SyncServerService`, `SyncedEventRecord`, `PushEventsRequest`, `PullEventsRequest`, `AckEventsRequest`.
- Produces: `push_events`, `pull_events`, idempotent event dedupe, explicit cursor ack.

- [ ] **Step 1: Write failing event sync tests**

In `crates/mneme-server/src/service.rs`, add:

```rust
#[tokio::test]
async fn event_push_is_idempotent_and_pull_respects_cursor() {
    let store = SqliteSyncServerStore::connect("sqlite::memory:").await.unwrap();
    let service = SyncServerService::new(store);
    let device_a = service.register_device("A".to_string(), None).await.unwrap().device.device_id;
    let device_b = service.register_device("B".to_string(), None).await.unwrap().device.device_id;
    let event = sample_synced_event(device_a, "run_started");

    let first = service.push_events(device_a, vec![event.clone()]).await.unwrap();
    assert_eq!(first.accepted, 1);
    assert_eq!(first.duplicates, 0);

    let second = service.push_events(device_a, vec![event]).await.unwrap();
    assert_eq!(second.accepted, 0);
    assert_eq!(second.duplicates, 1);

    let pulled = service.pull_events(device_b, 0, 50).await.unwrap();
    assert_eq!(pulled.events.len(), 1);
    assert_eq!(pulled.events[0].origin_device_id, device_a);

    service.ack_events(device_b, pulled.events[0].server_sequence).await.unwrap();
    let after_ack = service.pull_events(device_b, pulled.events[0].server_sequence, 50).await.unwrap();
    assert!(after_ack.events.is_empty());
}
```

Add helper in tests:

```rust
fn sample_synced_event(origin_device_id: DeviceId, kind_type: &str) -> SyncedEventRecord {
    SyncedEventRecord {
        server_sequence: 0,
        event_id: EventId::new(),
        origin_device_id,
        run_id: RunId::new(),
        occurred_at: time::OffsetDateTime::now_utc(),
        kind_type: kind_type.to_string(),
        payload_json: serde_json::json!({ "sample": true }),
        received_at: time::OffsetDateTime::now_utc(),
    }
}
```

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-server event_push_is_idempotent_and_pull_respects_cursor`

Expected: FAIL with missing `push_events`/`pull_events`.

- [ ] **Step 2: Implement push/pull outcomes**

Add to `service.rs`:

```rust
pub struct PushEventsOutcome {
    pub accepted: usize,
    pub duplicates: usize,
    pub latest_server_sequence: i64,
}

pub struct PullEventsOutcome {
    pub events: Vec<SyncedEventRecord>,
    pub latest_server_sequence: i64,
}
```

- [ ] **Step 3: Implement event push**

Add method:

```rust
pub async fn push_events(&self, device_id: DeviceId, events: Vec<SyncedEventRecord>) -> Result<PushEventsOutcome, SyncServerError>
```

Validation:

- known device required;
- event `origin_device_id` must equal `device_id`;
- empty vector is accepted with `accepted = 0`;
- duplicate `event_id` increments `duplicates` and does not insert;
- inserted rows set `received_at = OffsetDateTime::now_utc()`.

- [ ] **Step 4: Implement event pull**

Add method:

```rust
pub async fn pull_events(&self, device_id: DeviceId, after_server_sequence: i64, limit: usize) -> Result<PullEventsOutcome, SyncServerError>
```

Validation:

- known device required;
- `after_server_sequence >= 0`;
- `limit` clamped to `1..=500`;
- return rows where `server_sequence > after_server_sequence` ordered ascending.

- [ ] **Step 5: Verify and commit**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check
. "$HOME/.cargo/env" && cargo test -p mneme-server event_push
. "$HOME/.cargo/env" && cargo test -p mneme-server pull_events
. "$HOME/.cargo/env" && cargo test -p mneme-server
git diff --check
git status --short
```

Commit:

```bash
git add crates/mneme-server/src/service.rs crates/mneme-server/src/storage.rs
git commit -m "feat: add server event sync"
```

---

### Task 4: Object metadata sync with deterministic LWW

**Files:**
- Modify: `crates/mneme-server/src/service.rs`
- Modify: `crates/mneme-server/src/storage.rs`

**Interfaces:**
- Consumes: `SyncedObjectRecord`, `SyncObjectKind`, `lww_object_winner`.
- Produces: `push_objects`, `pull_objects`, object LWW conflict handling.

- [ ] **Step 1: Write failing object sync tests**

Add tests:

```rust
#[tokio::test]
async fn object_push_uses_lww_conflict_rule() {
    let store = SqliteSyncServerStore::connect("sqlite::memory:").await.unwrap();
    let service = SyncServerService::new(store);
    let device_a = service.register_device("A".to_string(), None).await.unwrap().device.device_id;
    let device_b = service.register_device("B".to_string(), None).await.unwrap().device.device_id;
    let run_id = RunId::new();

    let older = sample_synced_object(device_a, run_id, "artifact-1", "older", 1);
    let newer = sample_synced_object(device_b, run_id, "artifact-1", "newer", 2);

    service.push_objects(device_a, vec![older]).await.unwrap();
    service.push_objects(device_b, vec![newer]).await.unwrap();

    let pulled = service.pull_objects(device_a, None, 50).await.unwrap();
    assert_eq!(pulled.objects.len(), 1);
    assert_eq!(pulled.objects[0].payload_json["title"], "newer");
}
```

Helper:

```rust
fn sample_synced_object(origin_device_id: DeviceId, run_id: RunId, object_id: &str, title: &str, day: u8) -> SyncedObjectRecord {
    SyncedObjectRecord {
        object_id: object_id.to_string(),
        object_kind: SyncObjectKind::Artifact,
        origin_device_id,
        run_id,
        updated_at: time::OffsetDateTime::from_unix_timestamp(1_700_000_000 + i64::from(day)).unwrap(),
        payload_json: serde_json::json!({ "title": title }),
        received_at: time::OffsetDateTime::now_utc(),
    }
}
```

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-server object_push_uses_lww_conflict_rule`

Expected: FAIL with missing methods.

- [ ] **Step 2: Implement object outcomes**

Add:

```rust
pub struct PushObjectsOutcome {
    pub accepted: usize,
    pub kept_existing: usize,
}

pub struct PullObjectsOutcome {
    pub objects: Vec<SyncedObjectRecord>,
}
```

- [ ] **Step 3: Implement push_objects**

Method:

```rust
pub async fn push_objects(&self, device_id: DeviceId, objects: Vec<SyncedObjectRecord>) -> Result<PushObjectsOutcome, SyncServerError>
```

Rules:

- known device required;
- object `origin_device_id` must equal `device_id`;
- lookup current row by `(object_kind, object_id)`;
- if absent: insert;
- if present: choose winner via `lww_object_winner(current, incoming)`;
- update only when incoming wins;
- return `accepted` for inserts/updates, `kept_existing` for older losing objects.

- [ ] **Step 4: Implement pull_objects**

Method:

```rust
pub async fn pull_objects(&self, device_id: DeviceId, object_kind: Option<SyncObjectKind>, limit: usize) -> Result<PullObjectsOutcome, SyncServerError>
```

Rules:

- known device required;
- limit clamped to `1..=500`;
- optional kind filter;
- order by `updated_at desc, object_id asc`.

- [ ] **Step 5: Verify and commit**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check
. "$HOME/.cargo/env" && cargo test -p mneme-server object_push
. "$HOME/.cargo/env" && cargo test -p mneme-server pull_objects
. "$HOME/.cargo/env" && cargo test -p mneme-server
git diff --check
git status --short
```

Commit:

```bash
git add crates/mneme-server/src/service.rs crates/mneme-server/src/storage.rs
git commit -m "feat: add server object sync"
```

---

### Task 5: Backup export/import and server overview

**Files:**
- Modify: `crates/mneme-server/src/service.rs`
- Modify: `crates/mneme-server/src/storage.rs`

**Interfaces:**
- Consumes: devices, cursors, synced events, synced objects.
- Produces: `export_backup`, `import_backup`, `server_overview`, `BackupPayload`.

- [ ] **Step 1: Write failing backup and overview tests**

Add tests:

```rust
#[tokio::test]
async fn backup_export_import_roundtrips_sync_state() {
    let source = SyncServerService::new(SqliteSyncServerStore::connect("sqlite::memory:").await.unwrap());
    let device = source.register_device("A".to_string(), None).await.unwrap().device.device_id;
    source.push_events(device, vec![sample_synced_event(device, "run_started")]).await.unwrap();
    source.push_objects(device, vec![sample_synced_object(device, RunId::new(), "artifact-1", "A", 1)]).await.unwrap();

    let backup = source.export_backup().await.unwrap();

    let target = SyncServerService::new(SqliteSyncServerStore::connect("sqlite::memory:").await.unwrap());
    target.import_backup(backup.payload_json.clone()).await.unwrap();

    let overview = target.server_overview().await.unwrap();
    assert_eq!(overview.devices, 1);
    assert_eq!(overview.synced_events, 1);
    assert_eq!(overview.synced_objects, 1);
}
```

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-server backup_export_import_roundtrips_sync_state`

Expected: FAIL with missing backup methods.

- [ ] **Step 2: Implement backup payload**

Add:

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BackupPayload {
    pub devices: Vec<DeviceRecord>,
    pub cursors: Vec<SyncCursorRecord>,
    pub events: Vec<SyncedEventRecord>,
    pub objects: Vec<SyncedObjectRecord>,
}

pub struct BackupExportOutcome {
    pub backup: BackupRecord,
    pub payload_json: serde_json::Value,
}
```

- [ ] **Step 3: Implement export/import**

Methods:

```rust
pub async fn export_backup(&self) -> Result<BackupExportOutcome, SyncServerError>;
pub async fn import_backup(&self, payload_json: serde_json::Value) -> Result<BackupRecord, SyncServerError>;
```

Rules:

- export captures current devices/cursors/events/objects;
- export inserts one `backups` row with JSON payload;
- import validates JSON shape by deserializing `BackupPayload`;
- import upserts devices/cursors;
- import inserts events idempotently;
- import applies object LWW rules.

- [ ] **Step 4: Implement overview**

Method:

```rust
pub async fn server_overview(&self) -> Result<ServerOverviewResponse, SyncServerError>;
```

Return counts and latest event/backup timestamps as strings.

- [ ] **Step 5: Verify and commit**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check
. "$HOME/.cargo/env" && cargo test -p mneme-server backup
. "$HOME/.cargo/env" && cargo test -p mneme-server overview
. "$HOME/.cargo/env" && cargo test -p mneme-server
git diff --check
git status --short
```

Commit:

```bash
git add crates/mneme-server/src/service.rs crates/mneme-server/src/storage.rs
git commit -m "feat: add sync backup overview"
```

---

### Task 6: Server HTTP API and token checks

**Files:**
- Create: `crates/mneme-server/src/routes.rs`
- Modify: `crates/mneme-server/src/lib.rs`
- Modify: `crates/mneme-server/src/main.rs`
- Modify: `crates/mneme-server/src/service.rs`

**Interfaces:**
- Consumes: `SyncServerService` and all service methods.
- Produces: `build_router(service: SyncServerService) -> axum::Router`, HTTP routes from M9 spec, token/auth rejection tests.

- [ ] **Step 1: Write failing route tests**

In `routes.rs`, write tests:

```rust
#[tokio::test]
async fn api_registers_device_pushes_and_pulls_events() {
    let app = test_router().await;
    let registered: RegisterDeviceResponse = post_json(&app, "/api/v1/devices/register", serde_json::json!({
        "display_name": "Laptop",
        "public_key_fingerprint": "fp"
    })).await;

    let push: serde_json::Value = post_json(&app, "/api/v1/sync/push-events", serde_json::json!({
        "device_id": registered.device_id,
        "device_token": registered.device_token,
        "events": [sample_event_json(&registered.device_id)]
    })).await;
    assert_eq!(push["accepted"], 1);

    let pulled: serde_json::Value = post_json(&app, "/api/v1/sync/pull-events", serde_json::json!({
        "device_id": registered.device_id,
        "device_token": registered.device_token,
        "after_server_sequence": 0,
        "limit": 10
    })).await;
    assert_eq!(pulled["events"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn api_rejects_bad_device_token_without_leaking_it() {
    let app = test_router().await;
    let response = raw_post_json(&app, "/api/v1/sync/pull-events", serde_json::json!({
        "device_id": DeviceId::new().to_string(),
        "device_token": "secret-token",
        "after_server_sequence": 0,
        "limit": 10
    })).await;
    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
    let body = response_body_text(response).await;
    assert!(!body.contains("secret-token"));
}
```

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-server api_`

Expected: FAIL with missing routes.

- [ ] **Step 2: Implement token verification boundary**

Add to service:

```rust
pub async fn verify_device_token(&self, device_id: DeviceId, device_token: &str) -> Result<(), SyncServerError>
```

For M9 fake tokens, accept exactly:

```rust
format!("mneme-dev-token-{}", device_id)
```

Return `SyncServerError::Unauthorized` otherwise. Do not log token value.

- [ ] **Step 3: Implement route DTOs and router**

In `routes.rs`, define route request structs with `device_token` where needed. Add:

```rust
pub fn build_router(service: SyncServerService) -> axum::Router
```

Routes:

```text
GET  /api/v1/server/health
GET  /api/v1/server/overview
POST /api/v1/devices/register
GET  /api/v1/devices
GET  /api/v1/sync/cursors
POST /api/v1/sync/push-events
POST /api/v1/sync/pull-events
POST /api/v1/sync/ack-events
POST /api/v1/sync/push-objects
POST /api/v1/sync/pull-objects
POST /api/v1/backups/export
POST /api/v1/backups/import
GET  /api/v1/backups
```

Status mapping:

- `InvalidInput` -> 400;
- `UnknownDevice` -> 404;
- `Unauthorized` -> 401;
- `Storage(_)` -> 500.

- [ ] **Step 4: Implement main binary**

In `main.rs`, read:

```text
MNEME_SERVER_DB_URL default sqlite://.mneme/server.db?mode=rwc
MNEME_SERVER_BIND_ADDR default 127.0.0.1:3866
```

Create parent directory like daemon does, connect `SqliteSyncServerStore`, create service/router, bind listener.

Do not log device tokens.

- [ ] **Step 5: Verify and commit**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check
. "$HOME/.cargo/env" && cargo test -p mneme-server api_
. "$HOME/.cargo/env" && cargo test -p mneme-server
. "$HOME/.cargo/env" && cargo check --workspace
git diff --check
git status --short
```

Commit:

```bash
git add crates/mneme-server/src/routes.rs crates/mneme-server/src/lib.rs crates/mneme-server/src/main.rs crates/mneme-server/src/service.rs
git commit -m "feat: expose sync server API"
```

---

### Task 7: Reusable sync HTTP client crate

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `crates/mneme-sync/Cargo.toml`
- Create: `crates/mneme-sync/src/lib.rs`

**Interfaces:**
- Consumes: M9 server HTTP API DTOs from `mneme-kernel` sync records.
- Produces: `SyncClient`, `HttpSyncClient`, `SyncClientError`, offline mock transport tests.

- [ ] **Step 1: Add crate skeleton and failing client tests**

Add workspace member:

```toml
  "crates/mneme-sync",
```

Create `crates/mneme-sync/Cargo.toml`:

```toml
[package]
name = "mneme-sync"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
async-trait.workspace = true
mneme-kernel = { path = "../mneme-kernel" }
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
```

In `src/lib.rs`, write tests first with a mock transport:

```rust
#[tokio::test]
async fn http_sync_client_posts_register_device_request() {
    let transport = MockSyncTransport::new(serde_json::json!({
        "device_id": DeviceId::new().to_string(),
        "device_token": "mneme-dev-token-test"
    }));
    let client = HttpSyncClient::with_transport("http://127.0.0.1:3866", transport.clone());

    let response = client.register_device("Laptop".to_string()).await.unwrap();
    assert!(!response.device_token.is_empty());
    assert_eq!(transport.last_path().unwrap(), "/api/v1/devices/register");
    assert!(!format!("{:?}", response).contains("secret"));
}
```

Run: `. "$HOME/.cargo/env" && cargo test -p mneme-sync`

Expected: FAIL with missing client types.

- [ ] **Step 2: Implement client traits**

Implement:

```rust
#[derive(Debug, thiserror::Error)]
pub enum SyncClientError {
    #[error("invalid input")]
    InvalidInput,
    #[error("http error: {0}")]
    Http(String),
    #[error("server returned status {status}")]
    Status { status: u16 },
    #[error("malformed response: {0}")]
    MalformedResponse(String),
}

#[async_trait]
pub trait SyncClient: Send + Sync {
    async fn register_device(&self, display_name: String) -> Result<RegisterDeviceResponse, SyncClientError>;
    async fn push_events(&self, request: PushEventsRequest) -> Result<PushEventsResponse, SyncClientError>;
    async fn pull_events(&self, request: PullEventsRequest) -> Result<PullEventsResponse, SyncClientError>;
    async fn ack_events(&self, request: AckEventsRequest) -> Result<SyncCursorRecord, SyncClientError>;
}
```

Implement `HttpSyncClient` with reqwest. Tests may use a `SyncTransport` trait to avoid real network.

- [ ] **Step 3: Ensure secrets are not formatted by helper summaries**

If any response struct includes a token, add a client-side redacted debug wrapper rather than changing kernel DTO serialization. Test that wrapper output does not contain token values.

- [ ] **Step 4: Verify and commit**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check
. "$HOME/.cargo/env" && cargo test -p mneme-sync
. "$HOME/.cargo/env" && cargo check --workspace
git diff --check
git status --short
```

Commit:

```bash
git add Cargo.toml Cargo.lock crates/mneme-sync
git commit -m "feat: add sync HTTP client"
```

---

### Task 8: M9 documentation and M8 stale copy refresh

**Files:**
- Modify: `README.md`
- Create: `docs/architecture/milestone-9-self-hosted-sync-server.md`
- Modify: `docs/architecture/phase-1-foundation.md`
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`

**Interfaces:**
- Consumes: all M9 implemented boundaries.
- Produces: docs and one minor M8 broad-review copy fix.

- [ ] **Step 1: Update stale M8 Worker Runtime copy test**

In `apps/desktop/src/app/App.test.tsx`, update the worker runtime copy assertion to expect M8 wording:

```ts
expect(
  screen.getByText('Provider keys are configured in Desktop Runtime. No streaming token UI or tool execution is available in Milestone 8.')
).toBeInTheDocument();
```

Run: `pnpm --dir apps/desktop test -- src/app/App.test.tsx`

Expected: FAIL until production copy is updated.

- [ ] **Step 2: Update M8 copy**

In `apps/desktop/src/app/App.tsx`, replace the stale M7 copy with:

```text
Provider keys are configured in Desktop Runtime. No streaming token UI or tool execution is available in Milestone 8.
```

Do not add new UI behavior.

- [ ] **Step 3: Write M9 architecture doc**

Create `docs/architecture/milestone-9-self-hosted-sync-server.md` with sections:

```markdown
# Milestone 9: Self-hosted Multi-device Sync Server

## Scope
## Device Registration
## Append-only Event Sync
## Object Metadata Sync and LWW Projections
## Backup and Restore
## Remote Overview
## Authentication Boundary
## Non-goals
## Verification
## Phase 1 Closeout
```

Required statements:

- `Operation Ledger sync is append-only.`
- `Visible object conflicts use LWW projections with updated_at and origin_device_id.`
- `Device tokens are not written to Operation Ledger, local SQLite projections, frontend storage, or docs examples.`
- `M9 does not implement CRDTs, OAuth, mobile apps, official cloud hosting, or end-to-end encrypted sync.`

- [ ] **Step 4: Update README and phase doc**

README must include a short M9 run/test section with commands:

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-server
. "$HOME/.cargo/env" && cargo test -p mneme-sync
. "$HOME/.cargo/env" && pnpm check
```

Update `docs/architecture/phase-1-foundation.md` to say Phase 1 now spans local Kernel, daemon/API, Workbench, Tauri desktop shell, and self-hosted sync server.

- [ ] **Step 5: Docs scan and verify**

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
pnpm --dir apps/desktop test -- src/app/App.test.tsx
. "$HOME/.cargo/env" && pnpm check
git status --short
```

Commit:

```bash
git add README.md docs/architecture/milestone-9-self-hosted-sync-server.md docs/architecture/phase-1-foundation.md apps/desktop/src/app/App.tsx apps/desktop/src/app/App.test.tsx
git commit -m "docs: document self hosted sync server milestone"
```

---

### Task 9: Final verification and broad review

**Files:**
- No source changes expected.
- Uses review artifacts under `.superpowers/sdd/`.

**Interfaces:**
- Consumes: M9 base commit `63ab688` and all M9 task commits.
- Produces: final verification evidence, broad review package, progress ledger line.

- [ ] **Step 1: Run final verification**

Run:

```bash
. "$HOME/.cargo/env" && git status --short --branch && git log --oneline 63ab688..HEAD && pnpm check && cargo test -p mneme-server && cargo test -p mneme-sync && git diff --check && git diff --stat 63ab688..HEAD && git status --short --branch
```

Expected:

- branch clean before and after;
- `pnpm check` passes;
- server and sync client tests pass;
- `git diff --check` passes.

- [ ] **Step 2: Generate review package**

Run:

```bash
bash /Users/baiye/.agents/skills/subagent-driven-development/scripts/review-package 63ab688 HEAD .superpowers/sdd/review-63ab688..HEAD.diff
```

- [ ] **Step 3: Dispatch broad reviewer**

Ask a reviewer to inspect:

- real multi-device sync loop;
- event dedupe;
- cursor semantics;
- LWW object conflict rule;
- backup export/import;
- overview API;
- token secrecy;
- docs and non-goals;
- workspace verification.

- [ ] **Step 4: Append final ledger after approval**

After broad review approval, append:

```text
Milestone 9: complete (commits 63ab688..HEAD, final verification passed, overall review approved; Phase 1 foundation complete: local kernel, daemon/API, Workbench, Tauri desktop shell, worker runtime, and self-hosted multi-device sync server are in place)
```

- [ ] **Step 5: Goal completion check**

After ledger is appended and the worktree is clean, mark the active goal complete.
