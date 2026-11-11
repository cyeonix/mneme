# Mneme Milestone 9 Self-hosted Multi-device Sync Server Design

## 1. Decision

Milestone 9 implements **Real multi-device sync first** for Mneme.

The user explicitly selected the stronger M9 direction over a foundation-only server and approved this design direction:

> Self-hosted personal server with real multi-device event/object synchronization, backup/restore, and a minimal remote Web Console foundation.

The selected conflict policy is:

> **Append-only ledger + LWW projections**.

This means Mneme keeps every synchronized fact as an immutable event/object record, while user-facing read models resolve visible conflicts deterministically using last-write-wins projection rules.

## 2. Product Goal

M9 turns the local-first Mneme desktop stack into a self-hostable personal AI kernel network:

- each desktop/device can register with a user-owned Mneme server;
- each device can push local Operation Ledger events and object metadata;
- each device can pull events and object metadata it has not seen;
- the server keeps sync cursors per device;
- backup export/import can preserve server-side sync state;
- a minimal remote Web Console/overview can inspect server health, registered devices, cursor status, event counts, object counts, and backup metadata.

M9 must not turn Mneme into an official cloud service. The server is self-hosted and user-owned. Local desktop remains a first-class authority for local runs, tools, provider config, daemon lifecycle, and secrets.

## 3. Current Context

M1 through M8 established:

- Rust-first Kernel and append-only Operation Ledger.
- SQLite read models for runs, tools, artifacts, context bundles, knowledge sources, chunks, memories, retrieval traces, and worker jobs.
- Local daemon/API as Kernel authority.
- Tauri desktop shell with browser-safe bridge, write-only provider config UI, Stronghold-ready provider secret storage boundary, and daemon sidecar supervisor boundary.
- Real OpenAI-compatible non-streaming provider worker behind local daemon.

Relevant prior decisions:

- Technical selection selected **personal cloud server / self-hosted Mneme server** for multi-device sync, remote access, backup/restore, device registration, automation endpoints, and future remote execution.
- M8 explicitly deferred self-hosted server, cloud sync, remote accounts, multi-device sync, and remote console to M9.
- Operation Ledger remains source of truth; SQLite projections remain rebuildable.

## 4. M9 Scope

M9 includes:

1. Device identity and registration records.
2. Sync metadata records and per-device cursors.
3. Server-side SQLite schema for synchronized event/object storage.
4. A new `mneme-server` crate that exposes sync APIs.
5. A minimal sync client/service boundary reusable by local daemon or future desktop bridge.
6. Event push/pull APIs for Operation Ledger events.
7. Object metadata push/pull APIs for artifact/context/knowledge/memory/worker metadata required for cross-device browsing.
8. Deterministic append-only deduplication.
9. LWW projection conflict semantics documented and tested.
10. Backup export/import of server-side sync state.
11. Minimal remote Web Console/overview API.
12. Documentation and final verification.

M9 does not include:

- official hosted cloud service;
- user accounts on an official Mneme backend;
- OAuth/social login;
- multi-tenant organization permissions;
- mobile app;
- end-to-end encrypted sync protocol;
- production deployment scripts for every cloud provider;
- CRDT data structures;
- streaming remote agent execution;
- remote tool execution;
- remote provider-key storage;
- weakening M8 desktop secret-storage boundaries.

## 5. Architecture

M9 adds one main crate:

```text
crates/mneme-server
```

The M9 architecture is:

```text
Desktop/Tauri Workbench
  -> local mneme-daemon
       -> mneme-kernel Operation Ledger + SQLite projections
       -> optional sync client boundary
            -> self-hosted mneme-server
                 -> server SQLite sync store
                 -> device registry
                 -> event/object sync APIs
                 -> backup/restore APIs
                 -> remote overview APIs
```

`mneme-server` is a coordinator, not the owner of all local facts. It stores synchronized copies, deduplicates by stable IDs, tracks cursors, and serves pull requests. It does not rewrite local secrets, local provider config, or local desktop runtime state.

## 6. Core IDs and Records

M9 adds strong IDs:

```rust
pub struct DeviceId(Uuid);
pub struct SyncBatchId(Uuid);
pub struct BackupId(Uuid);
```

M9 domain records:

```rust
pub struct DeviceRecord {
    pub device_id: DeviceId,
    pub display_name: String,
    pub public_key_fingerprint: Option<String>,
    pub registered_at: OffsetDateTime,
    pub last_seen_at: OffsetDateTime,
}

pub struct SyncCursorRecord {
    pub device_id: DeviceId,
    pub last_seen_server_sequence: i64,
    pub updated_at: OffsetDateTime,
}

pub struct SyncedEventRecord {
    pub server_sequence: i64,
    pub event_id: EventId,
    pub origin_device_id: DeviceId,
    pub run_id: RunId,
    pub occurred_at: OffsetDateTime,
    pub kind_type: String,
    pub payload_json: serde_json::Value,
    pub received_at: OffsetDateTime,
}

pub enum SyncObjectKind {
    Artifact,
    ContextBundle,
    KnowledgeSource,
    DocumentChunk,
    Memory,
    RetrievalTrace,
    WorkerJob,
}

pub struct SyncedObjectRecord {
    pub object_id: String,
    pub object_kind: SyncObjectKind,
    pub origin_device_id: DeviceId,
    pub run_id: RunId,
    pub updated_at: OffsetDateTime,
    pub payload_json: serde_json::Value,
    pub received_at: OffsetDateTime,
}

pub struct BackupRecord {
    pub backup_id: BackupId,
    pub created_at: OffsetDateTime,
    pub device_count: usize,
    pub event_count: usize,
    pub object_count: usize,
}
```

## 7. Server SQLite Schema

`mneme-server` uses its own SQLite database.

Proposed tables:

```sql
create table if not exists devices (
  device_id text primary key,
  display_name text not null,
  public_key_fingerprint text,
  registered_at text not null,
  last_seen_at text not null
);

create table if not exists sync_cursors (
  device_id text primary key,
  last_seen_server_sequence integer not null,
  updated_at text not null
);

create table if not exists synced_events (
  server_sequence integer primary key autoincrement,
  event_id text not null unique,
  origin_device_id text not null,
  run_id text not null,
  occurred_at text not null,
  kind_type text not null,
  payload_json text not null,
  received_at text not null
);

create index if not exists idx_synced_events_origin_device_sequence
  on synced_events(origin_device_id, server_sequence);

create table if not exists synced_objects (
  object_id text not null,
  object_kind text not null,
  origin_device_id text not null,
  run_id text not null,
  updated_at text not null,
  payload_json text not null,
  received_at text not null,
  primary key (object_kind, object_id)
);

create index if not exists idx_synced_objects_run_kind_updated
  on synced_objects(run_id, object_kind, updated_at);

create table if not exists backups (
  backup_id text primary key,
  created_at text not null,
  device_count integer not null,
  event_count integer not null,
  object_count integer not null,
  payload_json text not null
);
```

These tables are server-side sync records, not replacements for local daemon projections.

## 8. Sync Semantics

### 8.1 Event Deduplication

Events are deduplicated by `event_id`.

If a device pushes an event already present on the server, the server treats it as accepted and does not insert a duplicate row.

### 8.2 Object Deduplication

Objects are keyed by `(object_kind, object_id)`.

If the same object is pushed multiple times:

- same payload and same `updated_at`: no visible change;
- newer `updated_at`: server replaces the stored object payload;
- same `updated_at` but different origin device: tie-break by lexical `origin_device_id` to keep deterministic behavior;
- older `updated_at`: server keeps the current object payload but may record the push in audit metrics if M9 includes audit counters.

### 8.3 LWW Projection Rule

M9 LWW rule:

```text
winner = max(updated_at, origin_device_id)
```

For event-derived projections, `occurred_at` is used as the timestamp. For object metadata, `updated_at` is used.

Important: LWW applies only to read-model visibility. It never deletes source events. Auditability comes from retaining every accepted event and deterministic object replacement history where the server schema allows it.

### 8.4 Cursor Rule

Each device has a cursor:

```text
last_seen_server_sequence
```

Pull requests return server events with:

```text
server_sequence > last_seen_server_sequence
```

The client acknowledges progress by updating its cursor after successful local append/rebuild. Server cursor updates must be explicit so interrupted sync does not mark unseen events as seen.

## 9. API Design

M9 server routes:

### 9.1 Health and Overview

```text
GET /api/v1/server/health
GET /api/v1/server/overview
```

Overview returns counts and recent timestamps:

```rust
pub struct ServerOverviewResponse {
    pub devices: usize,
    pub synced_events: usize,
    pub synced_objects: usize,
    pub backups: usize,
    pub latest_event_at: Option<String>,
    pub latest_backup_at: Option<String>,
}
```

### 9.2 Device Registration

```text
POST /api/v1/devices/register
GET  /api/v1/devices
```

Request:

```rust
pub struct RegisterDeviceRequest {
    pub display_name: String,
    pub public_key_fingerprint: Option<String>,
}
```

Response:

```rust
pub struct RegisterDeviceResponse {
    pub device_id: String,
    pub device_token: String,
}
```

`device_token` is returned once and not stored in plaintext when token hashing is implemented. M9 tests may use deterministic fake token generation. The token is not written into Operation Ledger.

### 9.3 Event Sync

```text
POST /api/v1/sync/push-events
POST /api/v1/sync/pull-events
POST /api/v1/sync/ack-events
GET  /api/v1/sync/cursors
```

Push request:

```rust
pub struct PushEventsRequest {
    pub device_id: String,
    pub events: Vec<SyncedEventInput>,
}
```

Pull request:

```rust
pub struct PullEventsRequest {
    pub device_id: String,
    pub after_server_sequence: i64,
    pub limit: Option<usize>,
}
```

Ack request:

```rust
pub struct AckEventsRequest {
    pub device_id: String,
    pub last_seen_server_sequence: i64,
}
```

### 9.4 Object Metadata Sync

```text
POST /api/v1/sync/push-objects
POST /api/v1/sync/pull-objects
```

Objects are metadata and JSON payloads. Binary artifact bytes remain a future packaging and storage transport concern unless M9 plan explicitly adds a small content transfer task.

M9 object sync must include at least:

- artifact metadata;
- context bundle metadata;
- knowledge source metadata;
- document chunk metadata;
- memory metadata;
- retrieval trace metadata;
- worker job metadata.

### 9.5 Backup and Restore

```text
POST /api/v1/backups/export
POST /api/v1/backups/import
GET  /api/v1/backups
```

Backup export creates a JSON payload containing:

- devices;
- cursors;
- synced events;
- synced objects;
- backup metadata.

Import validates shape, deduplicates by IDs, and preserves existing newer LWW object records.

## 10. Authentication Model

M9 uses a small self-hosted authentication model:

- device registration returns a device token;
- sync APIs require device ID and token proof;
- tests use fake tokens;
- docs never include real tokens;
- token values are not stored in Operation Ledger, SQLite projections of local daemon, frontend localStorage, or examples.

The design leaves room to replace token storage with hashed tokens and public-key signatures. Full account management and OAuth are outside M9.

## 11. Local Daemon Sync Boundary

M9 should add a reusable local sync client/service boundary, but it must not make the desktop depend on a server to function.

Suggested trait:

```rust
#[async_trait]
pub trait SyncClient: Send + Sync {
    async fn register_device(&self, display_name: String) -> Result<DeviceRegistration, SyncError>;
    async fn push_events(&self, device_id: DeviceId, events: Vec<OperationEvent>) -> Result<PushEventsOutcome, SyncError>;
    async fn pull_events(&self, device_id: DeviceId, after_server_sequence: i64, limit: usize) -> Result<PullEventsOutcome, SyncError>;
    async fn ack_events(&self, device_id: DeviceId, last_seen_server_sequence: i64) -> Result<SyncCursorRecord, SyncError>;
}
```

M9 may implement this as an in-process client for tests and an HTTP client if scope remains manageable. The core server sync flow must be tested even if desktop UI only exposes status/docs in M9.

## 12. Remote Web Console Foundation

M9 remote Web Console is intentionally minimal:

- server health;
- overview counts;
- registered devices table;
- sync cursor table;
- recent backup list.

It is acceptable for M9 to expose this as JSON API plus documentation rather than a polished standalone web UI, if the implementation plan needs to protect scope. The remote console must not expose provider API keys, desktop Stronghold state, or local daemon secrets.

## 13. Error Handling

Expected status mapping:

- invalid device ID, invalid cursor, invalid limit: `400 BAD_REQUEST`;
- unknown device: `404 NOT_FOUND`;
- token/auth failure: `401 UNAUTHORIZED`;
- malformed event/object payload: `400 BAD_REQUEST`;
- duplicate event push: `200 OK` with duplicate count;
- backup import validation failure: `400 BAD_REQUEST`;
- storage failure: `500 INTERNAL_SERVER_ERROR`.

Server error bodies must not include device tokens or secrets.

## 14. Testing Strategy

M9 tests must run offline.

Required test categories:

1. Device registration creates a device and initial cursor.
2. Duplicate event push is idempotent.
3. Pull returns only events after a cursor.
4. Ack updates the cursor explicitly.
5. Two simulated devices can push/pull each other's events.
6. LWW object conflict is deterministic.
7. Backup export/import roundtrips devices, cursors, events, and objects.
8. Overview counts reflect sync state.
9. Token/auth failures do not leak token values.
10. Full workspace verification passes.

## 15. Documentation Requirements

M9 docs must explain:

- self-hosted server role;
- local-first authority remains intact;
- append-only event sync;
- LWW projection semantics;
- device registration and cursor flow;
- backup/restore flow;
- remote overview boundary;
- token secrecy rules;
- deferred CRDT/E2EE/OAuth/mobile/production deployment hardening.

M9 docs should also close the Phase 1 narrative from local daemon to desktop shell to self-hosted sync server.

## 16. Success Criteria

M9 is complete when:

- `mneme-server` crate exists and is part of the workspace;
- device registration works with offline tests;
- event push/pull/ack works with offline two-device tests;
- object metadata push/pull works with LWW conflict tests;
- backup export/import works with roundtrip tests;
- overview APIs return device/event/object/backup/cursor summaries;
- secrets and tokens are redacted from debug/log/test output;
- docs clearly separate M9 server sync from M8 desktop secret storage;
- `pnpm check` passes;
- `git diff --check` passes;
- final broad review approves the milestone.

## 17. Final Phase 1 Handoff

After M9, Mneme Phase 1 should demonstrate a coherent foundation:

```text
Rust Kernel + Operation Ledger
Local daemon/API
React Workbench
Artifacts/context/search/memory/worker runtime
Tauri desktop shell
Self-hosted multi-device sync server
```

Future phases can deepen:

- real Stronghold adapter;
- production sidecar packaging;
- CRDT or domain-specific merge rules;
- encrypted sync;
- mobile controller;
- remote execution;
- production deployment hardening.

M9 should not hide these future needs. It should make the first real sync loop explicit, testable, and auditable.
