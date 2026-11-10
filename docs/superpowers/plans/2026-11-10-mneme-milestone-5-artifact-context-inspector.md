# Mneme Milestone 5 Artifact and Context Inspector Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 run 输出、上下文和附件提升为可浏览、可引用、可审计的 Artifact 与 ContextBundle 对象，并通过 Kernel、SQLite、HTTP API、TypeScript client 和 Workbench 形成只读闭环。

**Architecture:** Operation Ledger 继续作为事实源，新增 artifact/context events；SQLite `artifacts` 与 `context_bundles` 是可重建读模型；artifact 内容由本地 content-addressed store 以 `sha256` 保存。Artifact/Context 写入必须通过明确服务边界，API 与 UI 只调用服务和 HTTP client，不直接写 ledger、projection 或 content store。

**Tech Stack:** Rust `mneme-kernel` / Axum `mneme-api` / SQLite SQLx / Tokio async filesystem / TypeScript API client / React Workbench / pnpm check。

## Global Constraints

- 当前工作区是 `/Users/baiye/code/ai/mneme/.worktrees/phase-1-foundation`，分支是 `phase-1-foundation`。
- 设计规格是 `docs/superpowers/specs/2026-11-10-mneme-milestone-5-artifact-context-inspector-design.md`。
- Milestone 5 范围是“本地 content-addressed Artifact Store + ContextBundle 只读 Inspector”。
- Mneme 是 Personal AI Kernel（个人 AI 内核），不是通用云端 Agent 平台。
- Rust Kernel 是 authoritative control plane；TypeScript UI 不能绕过 Kernel/API 写数据。
- Operation Ledger 是 append-only fact source；SQLite projection 必须能由 ledger event 重建。
- HTTP API 必须 additive，不能破坏现有 health/events/runs/lifecycle/tool endpoints。
- M5 不做搜索、向量索引、记忆抽取、LLM prompt compilation、真实工具执行、MCP/OpenAPI/CLI/local script、Tauri、云同步或富媒体二进制预览。
- Artifact/Context 创建允许发生在 terminal run 之后，但必须校验 run 存在。
- unknown run/artifact/context 在 HTTP API 中统一映射为 `404 NOT_FOUND`；invalid UUID 和 blank required text 映射为 `400 BAD_REQUEST`。
- 每个实现任务必须先写失败测试，再实现最小代码，再运行验证并提交。
- 运行 Rust 命令前必须执行 `. "$HOME/.cargo/env"`。

---

## Current Baseline

- 当前 HEAD 是 `bc7dcbb docs: add milestone 5 artifact context design`。
- Kernel 当前导出 events/health/ids/ledger/projections/runs/service/storage/tools。
- `ids.rs` 当前有 `EventId`、`RunId`、`ToolCallId`。
- `events.rs` 当前有 run lifecycle 与 tool lifecycle variants，没有 artifact/context variants。
- `service.rs` 当前只有 `RunService`，包含 run lifecycle 与 tool lifecycle methods。
- SQLite migration 当前只有 `operation_events` 与 `runs` tables。
- API `ApiState` 当前只有 `run_service: SharedRunService`。
- Workbench 当前展示 Runs 与 Tool Activity。

## File Structure

- `Cargo.toml`：新增 workspace dependency `sha2 = "0.10"`。
- `crates/mneme-kernel/Cargo.toml`：新增 `sha2.workspace = true`。
- `crates/mneme-kernel/src/ids.rs`：新增 `ArtifactId`、`ContextBundleId`。
- `crates/mneme-kernel/src/artifacts.rs`：新增 Artifact/Context domain types、content store、projection store、service traits 与 in-memory implementations。
- `crates/mneme-kernel/src/events.rs`：新增 `ArtifactCreated`、`ContextBundleCreated` variants 与 constructors。
- `crates/mneme-kernel/src/storage/sqlite.rs`：新增 SQLite projection tables、artifact/context service implementation、ledger serde roundtrip support。
- `crates/mneme-kernel/src/lib.rs`：导出 artifact/context types 与 services。
- `crates/mneme-api/src/types.rs`：新增 Artifact/Context DTOs。
- `crates/mneme-api/src/routes.rs`：新增 artifact/context routes，扩展 `ApiState`。
- `crates/mneme-daemon/src/main.rs`：为 daemon wiring artifact/context SQLite services 与 local artifact store 路径。
- `apps/desktop/src/api/client.ts`、`apps/desktop/src/api/client.test.ts`：新增 Artifact/Context client 类型与方法。
- `apps/desktop/src/app/App.tsx`、`apps/desktop/src/app/App.test.tsx`、`apps/desktop/src/styles.css`：新增只读 Artifact Workspace 与 Context Inspector。
- `README.md`、`docs/architecture/milestone-5-artifact-context-inspector.md`、`docs/architecture/phase-1-foundation.md`：记录 M5 边界、API 与验证命令。

---

### Task 1: Kernel IDs、Artifact/Context domain types 与 ledger events

**Files:**
- Modify: `crates/mneme-kernel/src/ids.rs`
- Create: `crates/mneme-kernel/src/artifacts.rs`
- Modify: `crates/mneme-kernel/src/events.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`

**Interfaces:**
- Consumes: `EventId`, `RunId`, `OperationEvent`, `OperationEventKind`。
- Produces:
  - `ArtifactId::new() -> ArtifactId`
  - `ContextBundleId::new() -> ContextBundleId`
  - `ArtifactKind::{Text, Markdown, Json, Binary}` serialized as `text`, `markdown`, `json`, `binary`
  - `ArtifactRecord`
  - `ArtifactDetail { pub artifact: ArtifactRecord, pub content: Option<String> }`
  - `ContextBundleRecord`
  - `OperationEvent::new_artifact_created(run_id: RunId, artifact_id: ArtifactId, title: impl Into<String>, kind: ArtifactKind, mime_type: impl Into<String>, sha256: impl Into<String>, byte_len: u64) -> OperationEvent`
  - `OperationEvent::new_context_bundle_created(run_id: RunId, context_bundle_id: ContextBundleId, title: impl Into<String>, summary: impl Into<String>, artifact_ids: Vec<ArtifactId>, event_ids: Vec<EventId>) -> OperationEvent`

- [ ] **Step 1: 写失败测试：新增 ID 与 domain serde**

Add to `crates/mneme-kernel/src/ids.rs` tests:

```rust
#[test]
fn generated_artifact_and_context_bundle_ids_are_displayable() {
    let artifact_id = ArtifactId::new().to_string();
    let context_bundle_id = ContextBundleId::new().to_string();

    assert_eq!(artifact_id.len(), 36);
    assert!(artifact_id.contains('-'));
    assert_eq!(context_bundle_id.len(), 36);
    assert!(context_bundle_id.contains('-'));
}
```

Create `crates/mneme-kernel/src/artifacts.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArtifactId, ContextBundleId, EventId, RunId};

    #[test]
    fn artifact_kind_serializes_as_snake_case() {
        assert_eq!(serde_json::to_string(&ArtifactKind::Markdown).unwrap(), "\"markdown\"");
        assert_eq!(serde_json::to_string(&ArtifactKind::Json).unwrap(), "\"json\"");
    }

    #[test]
    fn context_bundle_record_roundtrips_reference_ids() {
        let record = ContextBundleRecord {
            context_bundle_id: ContextBundleId::new(),
            run_id: RunId::new(),
            title: "Debug context".to_string(),
            summary: "User report and generated artifact".to_string(),
            artifact_ids: vec![ArtifactId::new()],
            event_ids: vec![EventId::new()],
            created_at: time::OffsetDateTime::now_utc(),
        };

        let json = serde_json::to_string(&record).unwrap();
        let decoded: ContextBundleRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.context_bundle_id, record.context_bundle_id);
        assert_eq!(decoded.artifact_ids, record.artifact_ids);
        assert_eq!(decoded.event_ids, record.event_ids);
    }
}
```

- [ ] **Step 2: 运行失败测试**

Run:

```bash
. "$HOME/.cargo/env"
cargo test -p mneme-kernel generated_artifact_and_context_bundle_ids_are_displayable
cargo test -p mneme-kernel artifact_kind_serializes_as_snake_case
```

Expected: FAIL，包含 `cannot find type ArtifactId` 或 `cannot find type ArtifactKind`。

- [ ] **Step 3: 实现 ID、domain types 与 exports**

Implement ID newtypes in `ids.rs` following the existing `ToolCallId` pattern. Implement `artifacts.rs` with `ArtifactKind`, `ArtifactRecord`, `ArtifactDetail`, `ContextBundleRecord`. Add `pub mod artifacts;` and public exports in `lib.rs`:

```rust
pub use artifacts::{ArtifactDetail, ArtifactKind, ArtifactRecord, ContextBundleRecord};
pub use ids::{ArtifactId, ContextBundleId, EventId, RunId, ToolCallId};
```

- [ ] **Step 4: 写失败测试：ledger events serde roundtrip**

In `crates/mneme-kernel/src/events.rs` tests add:

```rust
#[test]
fn artifact_created_event_roundtrips_through_json() {
    let run_id = RunId::new();
    let artifact_id = ArtifactId::new();
    let event = OperationEvent::new_artifact_created(
        run_id,
        artifact_id,
        "Run summary",
        ArtifactKind::Markdown,
        "text/markdown",
        "abc123",
        42,
    );

    let json = serde_json::to_string(&event).unwrap();
    let decoded: OperationEvent = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.run_id, run_id);
    assert!(matches!(
        decoded.kind,
        OperationEventKind::ArtifactCreated { artifact_id: id, kind: ArtifactKind::Markdown, byte_len: 42, .. } if id == artifact_id
    ));
}

#[test]
fn context_bundle_created_event_roundtrips_through_json() {
    let run_id = RunId::new();
    let context_bundle_id = ContextBundleId::new();
    let artifact_id = ArtifactId::new();
    let event_id = EventId::new();
    let event = OperationEvent::new_context_bundle_created(
        run_id,
        context_bundle_id,
        "Debug context",
        "User report and artifact",
        vec![artifact_id],
        vec![event_id],
    );

    let json = serde_json::to_string(&event).unwrap();
    let decoded: OperationEvent = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.run_id, run_id);
    assert!(matches!(
        decoded.kind,
        OperationEventKind::ContextBundleCreated { context_bundle_id: id, artifact_ids, event_ids, .. }
            if id == context_bundle_id && artifact_ids == vec![artifact_id] && event_ids == vec![event_id]
    ));
}
```

- [ ] **Step 5: 运行失败测试**

Run:

```bash
. "$HOME/.cargo/env"
cargo test -p mneme-kernel artifact_created_event_roundtrips_through_json
cargo test -p mneme-kernel context_bundle_created_event_roundtrips_through_json
```

Expected: FAIL，包含 missing constructor 或 missing `OperationEventKind` variant。

- [ ] **Step 6: 实现 ledger events 与 SQLite kind mapping**

Add the two `OperationEventKind` variants and constructors in `events.rs`. Update any exhaustive matches in `storage/sqlite.rs` to serialize/deserialize `artifact_created` and `context_bundle_created` through the existing `kind_type` + `payload_json` mechanism.

- [ ] **Step 7: 验证并提交**

Run:

```bash
. "$HOME/.cargo/env"
cargo fmt --all --check
cargo test -p mneme-kernel artifact
cargo test -p mneme-kernel context_bundle
cargo test -p mneme-kernel event_roundtrips
cargo test -p mneme-kernel sqlite_ledger_roundtrips
git diff --check
git status --short
```

Expected: all commands exit 0 and status shows only intended files before commit.

Commit:

```bash
git add crates/mneme-kernel/src/ids.rs crates/mneme-kernel/src/artifacts.rs crates/mneme-kernel/src/events.rs crates/mneme-kernel/src/lib.rs crates/mneme-kernel/src/storage/sqlite.rs
git commit -m "feat: add artifact context event model"
```

---

### Task 2: Content-addressed ArtifactContentStore

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/mneme-kernel/Cargo.toml`
- Modify: `crates/mneme-kernel/src/artifacts.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: `ArtifactKind`, `ArtifactRecord`。
- Produces:
  - `StoredArtifactContent { pub sha256: String, pub byte_len: u64 }`
  - `ArtifactStoreError::{Io(String), NotFound(String)}`
  - `ArtifactContentStore` async trait with `put(content: Vec<u8>)` and `get(sha256: &str)`
  - `InMemoryArtifactContentStore::new() -> Self`
  - `LocalArtifactContentStore::new(root: impl Into<PathBuf>) -> Self`

- [ ] **Step 1: 写失败测试：content hash、dedup 与 get**

In `artifacts.rs` tests add:

```rust
#[tokio::test]
async fn in_memory_artifact_store_uses_sha256_content_address() {
    let store = InMemoryArtifactContentStore::new();

    let first = store.put(b"hello artifact".to_vec()).await.unwrap();
    let second = store.put(b"hello artifact".to_vec()).await.unwrap();
    let bytes = store.get(&first.sha256).await.unwrap();

    assert_eq!(first.sha256, second.sha256);
    assert_eq!(first.byte_len, 14);
    assert_eq!(bytes, b"hello artifact".to_vec());
}

#[tokio::test]
async fn local_artifact_store_writes_and_reads_by_sha256() {
    let root = std::env::temp_dir().join(format!("mneme-artifact-store-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let store = LocalArtifactContentStore::new(root.clone());

    let stored = store.put(b"local artifact".to_vec()).await.unwrap();
    let bytes = store.get(&stored.sha256).await.unwrap();

    assert_eq!(stored.byte_len, 14);
    assert_eq!(bytes, b"local artifact".to_vec());
    assert!(root.join("sha256").join(&stored.sha256).is_file());

    std::fs::remove_dir_all(&root).unwrap();
}
```

- [ ] **Step 2: 运行失败测试**

Run:

```bash
. "$HOME/.cargo/env"
cargo test -p mneme-kernel artifact_store
```

Expected: FAIL，包含 missing `InMemoryArtifactContentStore` 或 missing `sha2` dependency。

- [ ] **Step 3: 增加 dependency 并实现 store**

Add to root `Cargo.toml` workspace dependencies:

```toml
sha2 = "0.10"
```

Add to `crates/mneme-kernel/Cargo.toml` dependencies:

```toml
sha2.workspace = true
```

Implement SHA-256 hex using `sha2::{Digest, Sha256}`. `LocalArtifactContentStore` writes to `<root>/sha256/<hash>` with `tokio::fs::create_dir_all`, skips overwrite when file already exists, and returns `ArtifactStoreError::NotFound(hash.to_string())` for missing hash.

- [ ] **Step 4: 验证并提交**

Run:

```bash
. "$HOME/.cargo/env"
cargo fmt --all --check
cargo test -p mneme-kernel artifact_store
git diff --check
git status --short
```

Commit:

```bash
git add Cargo.toml Cargo.lock crates/mneme-kernel/Cargo.toml crates/mneme-kernel/src/artifacts.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add content addressed artifact store"
```

---

### Task 3: Artifact projection and service with SQLite

**Files:**
- Modify: `crates/mneme-kernel/migrations/0001_operation_ledger.sql`
- Modify: `crates/mneme-kernel/src/artifacts.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: `ArtifactContentStore`, `OperationEvent::new_artifact_created`, `RunProjectionStore` or existing run projection query in SQLite。
- Produces:
  - `CreateArtifactOutcome { pub artifact_id: ArtifactId }`
  - `ArtifactServiceError::{InvalidInput, UnknownRun, UnknownArtifact, ContentStore(String), Ledger(String), Projection(String)}`
  - `ArtifactProjectionStore` with `apply`, `list_artifacts`, `get_artifact`
  - `ComposedArtifactService`
  - `SqliteArtifactService`

- [ ] **Step 1: 写失败测试：in-memory artifact service**

In `artifacts.rs` tests add a service test that creates a run using existing in-memory run components, then creates an artifact:

```rust
#[tokio::test]
async fn composed_artifact_service_creates_lists_and_gets_artifact() {
    let ledger = crate::InMemoryOperationLedger::new().shared();
    let run_store = crate::InMemoryRunProjectionStore::new().shared();
    let run_service = crate::ComposedRunService::new(ledger.clone(), run_store.clone());
    let run = run_service.create_run("write a summary".to_string()).await.unwrap();
    let artifact_store = InMemoryArtifactContentStore::new().shared();
    let artifact_projection = InMemoryArtifactProjectionStore::new().shared();
    let service = ComposedArtifactService::new(ledger, run_store, artifact_projection, artifact_store);

    let outcome = service
        .create_text_artifact(
            run.run_id,
            "Run summary".to_string(),
            ArtifactKind::Markdown,
            "text/markdown".to_string(),
            "# Summary".to_string(),
        )
        .await
        .unwrap();

    let artifacts = service.list_artifacts().await.unwrap();
    let detail = service.get_artifact(outcome.artifact_id).await.unwrap();

    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].artifact_id, outcome.artifact_id);
    assert_eq!(detail.content.as_deref(), Some("# Summary"));
}
```

- [ ] **Step 2: 写失败测试：SQLite artifact service**

In `storage/sqlite.rs` tests add:

```rust
#[tokio::test]
async fn sqlite_artifact_service_creates_artifact_projection_and_ledger_event() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let run_service = SqliteRunService::new(store.clone());
    let run = run_service.create_run("collect artifact".to_string()).await.unwrap();
    let content_store = InMemoryArtifactContentStore::new().shared();
    let artifact_service = SqliteArtifactService::new(store.clone(), content_store);

    let created = artifact_service
        .create_text_artifact(
            run.run_id,
            "Collected note".to_string(),
            ArtifactKind::Text,
            "text/plain".to_string(),
            "note body".to_string(),
        )
        .await
        .unwrap();

    let artifacts = artifact_service.list_artifacts().await.unwrap();
    let events = store.list_events().await.unwrap();

    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].artifact_id, created.artifact_id);
    assert!(events.iter().any(|event| matches!(event.kind, OperationEventKind::ArtifactCreated { artifact_id, .. } if artifact_id == created.artifact_id)));
}
```

- [ ] **Step 3: 运行失败测试**

Run:

```bash
. "$HOME/.cargo/env"
cargo test -p mneme-kernel composed_artifact_service_creates_lists_and_gets_artifact
cargo test -p mneme-kernel sqlite_artifact_service_creates_artifact_projection_and_ledger_event
```

Expected: FAIL，包含 missing `ComposedArtifactService` 或 missing `artifacts` table。

- [ ] **Step 4: 实现 projection/service 与 SQLite table**

Add `artifacts` table and index to migration. Implement in-memory projection store and service in `artifacts.rs`. Implement `SqliteArtifactService` in `storage/sqlite.rs` with one transaction that verifies run exists, inserts `operation_events`, inserts `artifacts`, and stores content before ledger insert. Unknown run returns `ArtifactServiceError::UnknownRun`; blank title/content/mime returns `InvalidInput`.

- [ ] **Step 5: 验证并提交**

Run:

```bash
. "$HOME/.cargo/env"
cargo fmt --all --check
cargo test -p mneme-kernel artifact_service
cargo test -p mneme-kernel sqlite_artifact_service
cargo test -p mneme-kernel sqlite_ledger_roundtrips_artifact
git diff --check
git status --short
```

Commit:

```bash
git add crates/mneme-kernel/migrations/0001_operation_ledger.sql crates/mneme-kernel/src/artifacts.rs crates/mneme-kernel/src/storage/sqlite.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add artifact projection service"
```

---

### Task 4: ContextBundle projection and service with SQLite

**Files:**
- Modify: `crates/mneme-kernel/migrations/0001_operation_ledger.sql`
- Modify: `crates/mneme-kernel/src/artifacts.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: `ContextBundleRecord`, `OperationEvent::new_context_bundle_created`, `ArtifactProjectionStore` or SQLite artifacts table。
- Produces:
  - `CreateContextBundleOutcome { pub context_bundle_id: ContextBundleId }`
  - `ContextServiceError::{InvalidInput, UnknownRun, UnknownContextBundle, UnknownArtifact, UnknownEvent, Ledger(String), Projection(String)}`
  - `ContextBundleProjectionStore`
  - `ComposedContextService`
  - `SqliteContextService`

- [ ] **Step 1: 写失败测试：in-memory context service**

In `artifacts.rs` tests add:

```rust
#[tokio::test]
async fn composed_context_service_creates_lists_and_gets_context_bundle() {
    let ledger = crate::InMemoryOperationLedger::new().shared();
    let run_store = crate::InMemoryRunProjectionStore::new().shared();
    let run_service = crate::ComposedRunService::new(ledger.clone(), run_store.clone());
    let run = run_service.create_run("explain context".to_string()).await.unwrap();
    let artifact_projection = InMemoryArtifactProjectionStore::new().shared();
    let context_projection = InMemoryContextBundleProjectionStore::new().shared();
    let service = ComposedContextService::new(ledger, run_store, artifact_projection, context_projection);

    let outcome = service
        .create_context_bundle(
            run.run_id,
            "Debug context".to_string(),
            "User request and selected events".to_string(),
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();

    let bundles = service.list_context_bundles().await.unwrap();
    let bundle = service.get_context_bundle(outcome.context_bundle_id).await.unwrap();

    assert_eq!(bundles.len(), 1);
    assert_eq!(bundle.context_bundle_id, outcome.context_bundle_id);
    assert_eq!(bundle.summary, "User request and selected events");
}
```

- [ ] **Step 2: 写失败测试：SQLite context service**

In `storage/sqlite.rs` tests add:

```rust
#[tokio::test]
async fn sqlite_context_service_creates_context_bundle_projection_and_ledger_event() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    let run_service = SqliteRunService::new(store.clone());
    let run = run_service.create_run("bundle context".to_string()).await.unwrap();
    let context_service = SqliteContextService::new(store.clone());

    let created = context_service
        .create_context_bundle(
            run.run_id,
            "Context".to_string(),
            "Important inputs".to_string(),
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();

    let bundles = context_service.list_context_bundles().await.unwrap();
    let events = store.list_events().await.unwrap();

    assert_eq!(bundles.len(), 1);
    assert_eq!(bundles[0].context_bundle_id, created.context_bundle_id);
    assert!(events.iter().any(|event| matches!(event.kind, OperationEventKind::ContextBundleCreated { context_bundle_id, .. } if context_bundle_id == created.context_bundle_id)));
}
```

- [ ] **Step 3: 运行失败测试**

Run:

```bash
. "$HOME/.cargo/env"
cargo test -p mneme-kernel composed_context_service_creates_lists_and_gets_context_bundle
cargo test -p mneme-kernel sqlite_context_service_creates_context_bundle_projection_and_ledger_event
```

Expected: FAIL，包含 missing `ComposedContextService` 或 missing `context_bundles` table。

- [ ] **Step 4: 实现 context projection/service 与 SQLite table**

Add `context_bundles` table and index to migration. Store `artifact_ids` and `event_ids` as JSON arrays of strings. Validate run exists, title and summary are non-blank, referenced artifacts exist when artifact list is non-empty, and referenced events exist when event list is non-empty.

- [ ] **Step 5: 验证并提交**

Run:

```bash
. "$HOME/.cargo/env"
cargo fmt --all --check
cargo test -p mneme-kernel context_service
cargo test -p mneme-kernel sqlite_context_service
cargo test -p mneme-kernel sqlite_ledger_roundtrips_context
git diff --check
git status --short
```

Commit:

```bash
git add crates/mneme-kernel/migrations/0001_operation_ledger.sql crates/mneme-kernel/src/artifacts.rs crates/mneme-kernel/src/storage/sqlite.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add context bundle service"
```

---

### Task 5: HTTP API DTOs and routes

**Files:**
- Modify: `crates/mneme-api/src/types.rs`
- Modify: `crates/mneme-api/src/routes.rs`
- Modify: `crates/mneme-daemon/src/main.rs`

**Interfaces:**
- Consumes: `SharedArtifactService`, `SharedContextService`, `ArtifactServiceError`, `ContextServiceError`。
- Produces endpoints:
  - `GET /api/v1/artifacts`
  - `POST /api/v1/runs/:run_id/artifacts`
  - `GET /api/v1/artifacts/:artifact_id`
  - `GET /api/v1/context-bundles`
  - `POST /api/v1/runs/:run_id/context-bundles`
  - `GET /api/v1/context-bundles/:context_bundle_id`

- [ ] **Step 1: 写失败测试：artifact API happy path 与 validation**

In `routes.rs` tests add:

```rust
#[tokio::test]
async fn api_creates_lists_and_gets_artifact() {
    let app = build_router_with_in_memory_services();
    let run_id = create_run_for_test(&app, "collect artifact").await;

    let create_response = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/v1/runs/{run_id}/artifacts"),
            serde_json::json!({
                "title": "Run summary",
                "kind": "markdown",
                "mime_type": "text/markdown",
                "content": "# Summary"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = response_json(create_response).await;
    let artifact_id = body["artifact_id"].as_str().unwrap();

    let list_response = app.clone().oneshot(empty_request("GET", "/api/v1/artifacts")).await.unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);

    let get_response = app
        .oneshot(empty_request("GET", &format!("/api/v1/artifacts/{artifact_id}")))
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    let detail = response_json(get_response).await;
    assert_eq!(detail["artifact"]["title"], "Run summary");
    assert_eq!(detail["content"], "# Summary");
}
```

- [ ] **Step 2: 写失败测试：context API happy path**

Add:

```rust
#[tokio::test]
async fn api_creates_lists_and_gets_context_bundle() {
    let app = build_router_with_in_memory_services();
    let run_id = create_run_for_test(&app, "collect context").await;

    let create_response = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/v1/runs/{run_id}/context-bundles"),
            serde_json::json!({
                "title": "Debug context",
                "summary": "User report and selected events",
                "artifact_ids": [],
                "event_ids": []
            }),
        ))
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = response_json(create_response).await;
    let context_bundle_id = body["context_bundle_id"].as_str().unwrap();

    let get_response = app
        .oneshot(empty_request("GET", &format!("/api/v1/context-bundles/{context_bundle_id}")))
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    let detail = response_json(get_response).await;
    assert_eq!(detail["title"], "Debug context");
    assert_eq!(detail["artifact_ids"].as_array().unwrap().len(), 0);
}
```

- [ ] **Step 3: 运行失败测试**

Run:

```bash
. "$HOME/.cargo/env"
cargo test -p mneme-api artifact
cargo test -p mneme-api context_bundle
```

Expected: FAIL，包含 missing route 或 missing DTO。

- [ ] **Step 4: 实现 DTOs, state, routes, daemon wiring**

Add DTOs in `types.rs`: `CreateArtifactRequest`, `CreateArtifactResponse`, `ArtifactsResponse`, `ArtifactDetailResponse`, `CreateContextBundleRequest`, `CreateContextBundleResponse`, `ContextBundlesResponse`. Extend `ApiState` with artifact/context services. Keep existing `build_router_with_service` for tests by constructing in-memory artifact/context services from shared ledger/run store. Add explicit builder for daemon accepting all services.

- [ ] **Step 5: 验证并提交**

Run:

```bash
. "$HOME/.cargo/env"
cargo fmt --all --check
cargo test -p mneme-api artifact
cargo test -p mneme-api context_bundle
cargo test -p mneme-api
cargo test --workspace
git diff --check
git status --short
```

Commit:

```bash
git add crates/mneme-api/src/types.rs crates/mneme-api/src/routes.rs crates/mneme-daemon/src/main.rs
git commit -m "feat: expose artifact context API"
```

---

### Task 6: TypeScript client

**Files:**
- Modify: `apps/desktop/src/api/client.ts`
- Modify: `apps/desktop/src/api/client.test.ts`
- Modify: `apps/desktop/src/app/App.test.tsx`

**Interfaces:**
- Consumes HTTP DTOs from Task 5。
- Produces non-optional `MnemeApiClient` methods: `listArtifacts`, `createArtifact`, `getArtifact`, `listContextBundles`, `createContextBundle`, `getContextBundle`。

- [ ] **Step 1: 写失败测试**

In `client.test.ts` add tests that call `client.createArtifact('run-1', { title: 'Run summary', kind: 'markdown', mime_type: 'text/markdown', content: '# Summary' })` and assert path `/api/v1/runs/run-1/artifacts`, method `POST`, JSON body. Add a second test for `createContextBundle('run-1', { title: 'Debug context', summary: 'User report', artifact_ids: [], event_ids: [] })` and assert path `/api/v1/runs/run-1/context-bundles`.

- [ ] **Step 2: 运行失败测试**

Run:

```bash
pnpm --dir apps/desktop test -- src/api/client.test.ts
```

Expected: FAIL，包含 `client.createArtifact is not a function` 或 TypeScript property missing。

- [ ] **Step 3: 实现 client 类型与方法**

Add `ArtifactKind`, `ArtifactRecord`, `ArtifactDetail`, `ContextBundleRecord`, request/response interfaces, and non-optional methods to `MnemeApiClient`. Use existing `requestJson` helper and existing base URL normalization.

- [ ] **Step 4: 更新 App test double**

Update `apps/desktop/src/app/App.test.tsx` test double so every `MnemeApiClient` property is implemented. Return empty `artifacts` and `context_bundles` arrays for list methods.

- [ ] **Step 5: 验证并提交**

Run:

```bash
pnpm --dir apps/desktop check
pnpm --dir apps/desktop test -- src/api/client.test.ts
. "$HOME/.cargo/env" && pnpm check
git diff --check
git status --short
```

Commit:

```bash
git add apps/desktop/src/api/client.ts apps/desktop/src/api/client.test.ts apps/desktop/src/app/App.test.tsx
git commit -m "feat: add artifact context API client"
```

---

### Task 7: Workbench Artifact Workspace and Context Inspector

**Files:**
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Modify: `apps/desktop/src/styles.css`

**Interfaces:**
- Consumes `MnemeApiClient.listArtifacts()`, `getArtifact()`, `listContextBundles()`。
- Produces read-only UI sections `Artifact Workspace` and `Context Inspector`。

- [ ] **Step 1: 写失败测试**

In `App.test.tsx` add a test double returning one artifact detail and one context bundle. Assert rendered text includes `Artifact Workspace`, artifact title `Run summary`, `markdown`, sha prefix, preview `# Summary`, `Context Inspector`, context title `Debug context`, `1 artifact`, and `2 events`.

- [ ] **Step 2: 运行失败测试**

Run:

```bash
pnpm --dir apps/desktop test -- src/app/App.test.tsx
```

Expected: FAIL，包含 `Unable to find an element with the text: Artifact Workspace`。

- [ ] **Step 3: 实现只读 UI**

Load artifacts and context bundles alongside health/events/runs. Render two sections after Tool Activity. Artifact preview uses `getArtifact(artifact_id)` for text/markdown/json records and truncates content to 240 characters. Binary records render `Binary artifact preview is not available in Milestone 5.` Do not add edit buttons, execution buttons, search UI, drag/drop, or prompt compile UI.

- [ ] **Step 4: 验证并提交**

Run:

```bash
pnpm --dir apps/desktop check
pnpm --dir apps/desktop test -- src/app/App.test.tsx
. "$HOME/.cargo/env" && pnpm check
git diff --check
git status --short
```

Commit:

```bash
git add apps/desktop/src/app/App.tsx apps/desktop/src/app/App.test.tsx apps/desktop/src/styles.css
git commit -m "feat: show artifact context inspector"
```

---

### Task 8: Documentation

**Files:**
- Modify: `README.md`
- Create: `docs/architecture/milestone-5-artifact-context-inspector.md`
- Modify: `docs/architecture/phase-1-foundation.md`

**Interfaces:**
- Consumes implemented API endpoints and behavior from Tasks 1–7。
- Produces user/developer docs explaining M5 boundary and verification.

- [ ] **Step 1: 写 docs**

Create `docs/architecture/milestone-5-artifact-context-inspector.md` with sections: Scope, Event Flow, Content Addressed Store, Service Boundary, HTTP API, Workbench, Non-goals, Verification. Add README curl examples for create artifact and create context bundle. Update `phase-1-foundation.md` to say M5 adds artifact/context read models and read-only Workbench sections.

- [ ] **Step 2: 验证 docs**

Run:

```bash
git diff --check
python3 - <<'PY'
from pathlib import Path
paths=[Path('README.md'), *Path('docs/architecture').glob('*.md')]
needles=['TB'+'D','TO'+'DO','place'+'holder','FIX'+'ME','?'+'?']
hits=[]
for p in paths:
    text=p.read_text()
    for needle in needles:
        if needle in text:
            hits.append(f'{p}:{needle}')
print('docs scan hits:', hits)
raise SystemExit(1 if hits else 0)
PY
. "$HOME/.cargo/env" && pnpm check
git status --short
```

Expected: all commands exit 0.

- [ ] **Step 3: 提交 docs**

Commit:

```bash
git add README.md docs/architecture/milestone-5-artifact-context-inspector.md docs/architecture/phase-1-foundation.md
git commit -m "docs: document artifact context inspector milestone"
```

---

### Task 9: Final verification and broad review

**Files:**
- Read only: entire branch diff from Milestone 5 base `bc7dcbb46f5564ae195003ff37db77867023dda2` to current HEAD.
- Modify only if reviewer finds Critical/Important issues.

**Interfaces:**
- Consumes all M5 tasks。
- Produces final M5 approval and progress ledger line.

- [ ] **Step 1: Run full verification**

Run:

```bash
. "$HOME/.cargo/env"
git status --short --branch
git log --oneline bc7dcbb46f5564ae195003ff37db77867023dda2..HEAD
pnpm check
git diff --check
git diff --stat bc7dcbb46f5564ae195003ff37db77867023dda2..HEAD
git status --short --branch
```

Expected: `pnpm check` exits 0, `git diff --check` exits 0, and status is clean.

- [ ] **Step 2: Create review package**

Run:

```bash
mkdir -p .superpowers/sdd
git diff bc7dcbb46f5564ae195003ff37db77867023dda2..HEAD > .superpowers/sdd/review-bc7dcbb..HEAD-m5.diff
```

- [ ] **Step 3: Broad review checklist**

Reviewer must check:

- Artifact/Context ID newtypes are distinct and serde/display/from_str work.
- Operation Ledger serializes artifact/context events and SQLite roundtrips them.
- Artifact content store writes and reads by sha256.
- Artifact and Context services validate run existence and blank inputs.
- SQLite projections are written through service boundary and queryable.
- API routes are additive and error mapping matches Global Constraints.
- TypeScript client methods are non-optional and path/body tests cover them.
- Workbench is read-only and does not imply search, memory, prompt compilation, real tool execution, Tauri, cloud, or rich binary preview.
- Docs describe M5/M6/M7 boundaries.

- [ ] **Step 4: Record progress if review approves**

Append to `.superpowers/sdd/progress.md`:

```text
Milestone 5: complete (commits bc7dcbb..HEAD, final verification passed, overall review approved; follow-ups for Milestone 6: build search/memory/knowledge indexing from artifact/context objects without changing M5 ledger facts)
```

No commit is required for `.superpowers/sdd/progress.md` because it is gitignored.
