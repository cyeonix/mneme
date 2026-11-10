# Mneme Milestone 6 Search Memory Knowledge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a SQLite-first, explainable Search / Memory / Knowledge foundation around existing ledger, artifact, and context objects.

**Architecture:** Operation Ledger and SQLite read models remain the authority. M6 adds knowledge source, document chunk, memory, retrieval trace, deterministic search, and vector-index abstraction as rebuildable projections/services; API, TypeScript client, and Workbench expose a narrow read/write vertical slice without real embeddings, Tantivy, worker runtime, or agent prompt injection.

**Tech Stack:** Rust workspace, serde, time, uuid, async-trait, SQLx SQLite, axum, React + TypeScript + Vite/Vitest, existing `pnpm check` workspace verification.

## Global Constraints

- Milestone 6 scope is **SQLite-first 可解释检索 + 索引抽象**.
- Do not generate real embeddings.
- Do not integrate Tantivy as a production full-text sidecar.
- Do not bind LanceDB, Qdrant, sqlite-vec, or any other vector database.
- Do not call Worker Runtime; Milestone 7 defines worker protocol and execution plane.
- Do not implement OCR, PDF parsing, web crawling, or rich document ingestion.
- Do not inject search results into agent prompts or implement prompt compilation.
- Do not let search sidecars become the only fact source; Operation Ledger, SQLite read models, and artifact content store remain authoritative.
- Current `OperationEvent` requires `run_id: RunId`; all M6 knowledge, memory, and retrieval operations must associate with a run.
- UI and docs must not claim semantic embedding, Tantivy production index, automatic long-term memory, or automatic agent recall.
- Every task must run its listed verification commands with `. "$HOME/.cargo/env"` for Rust commands.
- Keep commits small and one task per commit.

---

## File Structure

- `crates/mneme-kernel/src/ids.rs`: add `KnowledgeSourceId`, `DocumentChunkId`, `MemoryId`, `RetrievalTraceId` strong IDs.
- `crates/mneme-kernel/src/events.rs`: add M6 event variants and constructors.
- `crates/mneme-kernel/src/knowledge.rs`: new focused Kernel module for knowledge sources, chunks, memories, retrieval traces, projection traits, deterministic chunking, index traits, and composed services.
- `crates/mneme-kernel/src/storage/sqlite.rs`: add SQLite M6 services/projection stores and ledger rebuild support following existing artifact/context style.
- `crates/mneme-kernel/migrations/0001_operation_ledger.sql`: add M6 read-model tables and indexes.
- `crates/mneme-kernel/src/lib.rs`: export M6 types and SQLite services.
- `crates/mneme-api/src/types.rs`: add M6 DTOs.
- `crates/mneme-api/src/routes.rs`: add M6 routes while preserving existing builders.
- `crates/mneme-daemon/src/main.rs`: wire production SQLite M6 services.
- `apps/desktop/src/api/client.ts`: add M6 TS types and non-optional client methods.
- `apps/desktop/src/api/client.test.ts`: verify M6 client paths and request bodies.
- `apps/desktop/src/app/App.tsx`: add Workbench Knowledge Search, Memory Inspector, Retrieval Traces read-only views.
- `apps/desktop/src/app/App.test.tsx`: update test double and UI assertions.
- `apps/desktop/src/styles.css`: add focused styles for new views.
- `README.md`: add M6 curl examples.
- `docs/architecture/milestone-6-search-memory-knowledge.md`: new architecture doc.
- `docs/architecture/phase-1-foundation.md`: update Phase 1 status.

---

### Task 1: Kernel IDs, domain records, and ledger events

**Files:**
- Modify: `crates/mneme-kernel/src/ids.rs`
- Modify: `crates/mneme-kernel/src/events.rs`
- Create: `crates/mneme-kernel/src/knowledge.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`

**Interfaces:**
- Consumes: existing `RunId`, `EventId`, `ArtifactId`, `ContextBundleId`, `OperationEvent`, `OperationEventKind`.
- Produces:
  - `pub struct KnowledgeSourceId(Uuid);`
  - `pub struct DocumentChunkId(Uuid);`
  - `pub struct MemoryId(Uuid);`
  - `pub struct RetrievalTraceId(Uuid);`
  - `pub enum KnowledgeSourceKind { Artifact, ContextBundle, Manual }`
  - `pub enum MemoryKind { Fact, Decision, Preference, Insight }`
  - `pub struct KnowledgeSourceRecord { pub knowledge_source_id: KnowledgeSourceId, pub run_id: RunId, pub source_kind: KnowledgeSourceKind, pub artifact_id: Option<ArtifactId>, pub context_bundle_id: Option<ContextBundleId>, pub title: String, pub summary: String, pub created_at: OffsetDateTime }`
  - `pub struct DocumentChunkRecord { pub document_chunk_id: DocumentChunkId, pub knowledge_source_id: KnowledgeSourceId, pub run_id: RunId, pub sequence: i64, pub text: String, pub char_start: i64, pub char_end: i64, pub created_at: OffsetDateTime }`
  - `pub struct MemoryRecord { pub memory_id: MemoryId, pub knowledge_source_id: Option<KnowledgeSourceId>, pub run_id: RunId, pub kind: MemoryKind, pub title: String, pub body: String, pub confidence: f32, pub created_at: OffsetDateTime }`
  - `pub struct RetrievalTraceRecord { pub retrieval_trace_id: RetrievalTraceId, pub run_id: RunId, pub query: String, pub result_chunk_ids: Vec<DocumentChunkId>, pub result_memory_ids: Vec<MemoryId>, pub explanation: String, pub created_at: OffsetDateTime }`
  - `OperationEventKind::{KnowledgeSourceCreated, DocumentChunkCreated, MemoryRecorded, RetrievalTraceRecorded}`.
  - Constructors `OperationEvent::new_knowledge_source_created`, `new_document_chunk_created`, `new_memory_recorded`, `new_retrieval_trace_recorded`.

- [ ] **Step 1: Write failing ID tests**

Add tests to `crates/mneme-kernel/src/ids.rs`:

```rust
#[test]
fn generated_knowledge_memory_and_retrieval_ids_are_displayable() {
    let source_id = KnowledgeSourceId::new().to_string();
    let chunk_id = DocumentChunkId::new().to_string();
    let memory_id = MemoryId::new().to_string();
    let trace_id = RetrievalTraceId::new().to_string();

    for id in [source_id, chunk_id, memory_id, trace_id] {
        assert_eq!(id.len(), 36);
        assert!(id.contains('-'));
    }
}

#[test]
fn knowledge_memory_and_retrieval_ids_parse_from_display() {
    let source_id = KnowledgeSourceId::new();
    let chunk_id = DocumentChunkId::new();
    let memory_id = MemoryId::new();
    let trace_id = RetrievalTraceId::new();

    assert_eq!(source_id.to_string().parse::<KnowledgeSourceId>().unwrap(), source_id);
    assert_eq!(chunk_id.to_string().parse::<DocumentChunkId>().unwrap(), chunk_id);
    assert_eq!(memory_id.to_string().parse::<MemoryId>().unwrap(), memory_id);
    assert_eq!(trace_id.to_string().parse::<RetrievalTraceId>().unwrap(), trace_id);
}
```

- [ ] **Step 2: Run ID tests and verify RED**

Run:

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-kernel generated_knowledge_memory_and_retrieval_ids_are_displayable knowledge_memory_and_retrieval_ids_parse_from_display
```

Expected: FAIL with missing `KnowledgeSourceId`, `DocumentChunkId`, `MemoryId`, or `RetrievalTraceId` types.

- [ ] **Step 3: Add strong IDs**

In `crates/mneme-kernel/src/ids.rs`, add four newtypes near existing IDs and implement `new`, `Default`, `Display`, and `FromStr` using the same pattern as `ArtifactId` and `ContextBundleId`.

```rust
/// 知识来源的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KnowledgeSourceId(Uuid);

/// 可检索文本块的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentChunkId(Uuid);

/// 可解释记忆的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryId(Uuid);

/// 一次检索审计记录的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RetrievalTraceId(Uuid);
```

- [ ] **Step 4: Create domain record tests**

Create `crates/mneme-kernel/src/knowledge.rs` with the domain records and tests. Start with tests at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArtifactId, ContextBundleId, RunId};
    use time::OffsetDateTime;

    #[test]
    fn knowledge_source_record_serializes_with_snake_case_kind() {
        let record = KnowledgeSourceRecord {
            knowledge_source_id: KnowledgeSourceId::new(),
            run_id: RunId::new(),
            source_kind: KnowledgeSourceKind::Artifact,
            artifact_id: Some(ArtifactId::new()),
            context_bundle_id: None,
            title: "Run summary".to_string(),
            summary: "Source summary".to_string(),
            created_at: OffsetDateTime::now_utc(),
        };

        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("artifact"));
        let decoded: KnowledgeSourceRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.title, "Run summary");
        assert_eq!(decoded.source_kind, KnowledgeSourceKind::Artifact);
    }

    #[test]
    fn memory_record_requires_confidence_shape() {
        let record = MemoryRecord {
            memory_id: MemoryId::new(),
            knowledge_source_id: None,
            run_id: RunId::new(),
            kind: MemoryKind::Decision,
            title: "Architecture decision".to_string(),
            body: "Use SQLite-first retrieval.".to_string(),
            confidence: 0.8,
            created_at: OffsetDateTime::now_utc(),
        };

        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("decision"));
        let decoded: MemoryRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.confidence, 0.8);
    }
}
```

- [ ] **Step 5: Run domain tests and verify RED**

Run:

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-kernel knowledge_source_record_serializes_with_snake_case_kind memory_record_requires_confidence_shape
```

Expected: FAIL because `knowledge` module and domain types are not exported yet.

- [ ] **Step 6: Implement domain records**

In `crates/mneme-kernel/src/knowledge.rs`, define the exact records from the Interfaces block. Use:

```rust
use crate::{
    ArtifactId, ContextBundleId, DocumentChunkId, KnowledgeSourceId, MemoryId, RetrievalTraceId,
    RunId,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSourceKind {
    Artifact,
    ContextBundle,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Fact,
    Decision,
    Preference,
    Insight,
}
```

Continue with the four record structs from the Interfaces block and derive `Debug, Clone, PartialEq, Serialize, Deserialize`. Use `Eq` only when all fields support it; `MemoryRecord` contains `f32`, so do not derive `Eq` for it.

- [ ] **Step 7: Add event constructor tests**

In `crates/mneme-kernel/src/events.rs`, add tests:

```rust
#[test]
fn knowledge_source_event_records_source_metadata() {
    let run_id = RunId::new();
    let source_id = KnowledgeSourceId::new();
    let artifact_id = ArtifactId::new();

    let event = OperationEvent::new_knowledge_source_created(
        run_id,
        source_id,
        KnowledgeSourceKind::Artifact,
        Some(artifact_id),
        None,
        "Run summary",
        "Source summary",
    );

    assert_eq!(event.run_id, run_id);
    assert!(matches!(event.kind, OperationEventKind::KnowledgeSourceCreated { knowledge_source_id, source_kind: KnowledgeSourceKind::Artifact, artifact_id: Some(id), .. } if knowledge_source_id == source_id && id == artifact_id));
}

#[test]
fn retrieval_trace_event_records_result_ids() {
    let run_id = RunId::new();
    let trace_id = RetrievalTraceId::new();
    let chunk_id = DocumentChunkId::new();
    let memory_id = MemoryId::new();

    let event = OperationEvent::new_retrieval_trace_recorded(
        run_id,
        trace_id,
        "sqlite search",
        vec![chunk_id],
        vec![memory_id],
        "deterministic keyword retrieval matched one chunk",
    );

    assert!(matches!(event.kind, OperationEventKind::RetrievalTraceRecorded { retrieval_trace_id, result_chunk_ids, result_memory_ids, .. } if retrieval_trace_id == trace_id && result_chunk_ids == vec![chunk_id] && result_memory_ids == vec![memory_id]));
}
```

- [ ] **Step 8: Run event tests and verify RED**

Run:

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-kernel knowledge_source_event_records_source_metadata retrieval_trace_event_records_result_ids
```

Expected: FAIL because event variants and constructors are missing.

- [ ] **Step 9: Implement event variants and exports**

In `crates/mneme-kernel/src/events.rs`, import the new IDs and domain enums, add four `OperationEventKind` variants from the spec, and add four constructors. Each constructor must set `id: EventId::new()`, the supplied `run_id`, `occurred_at: OffsetDateTime::now_utc()`, and the appropriate kind.

In `crates/mneme-kernel/src/lib.rs`, add:

```rust
pub mod knowledge;

pub use ids::{
    ArtifactId, ContextBundleId, DocumentChunkId, EventId, KnowledgeSourceId, MemoryId, RetrievalTraceId,
    RunId, ToolCallId,
};
pub use knowledge::{
    DocumentChunkRecord, KnowledgeSourceKind, KnowledgeSourceRecord, MemoryKind, MemoryRecord,
    RetrievalTraceRecord,
};
```

In `crates/mneme-kernel/src/storage/sqlite.rs`, update all `match event.kind` blocks for SQLite ledger serialization/deserialization so M6 events roundtrip through `operation_events.kind_type` and `payload_json`. Use kind type strings:

- `knowledge_source_created`
- `document_chunk_created`
- `memory_recorded`
- `retrieval_trace_recorded`

- [ ] **Step 10: Add SQLite ledger roundtrip tests**

In `crates/mneme-kernel/src/storage/sqlite.rs`, add a test named `sqlite_ledger_roundtrips_knowledge_memory_retrieval_events`:

```rust
#[tokio::test]
async fn sqlite_ledger_roundtrips_knowledge_memory_retrieval_events() {
    let store = temp_sqlite_store("sqlite_ledger_roundtrips_knowledge_memory_retrieval_events").await;
    let ledger = SqliteOperationLedger::new(store);
    let run_id = RunId::new();
    let source_id = KnowledgeSourceId::new();
    let chunk_id = DocumentChunkId::new();
    let memory_id = MemoryId::new();
    let trace_id = RetrievalTraceId::new();

    ledger.append(OperationEvent::new_knowledge_source_created(
        run_id,
        source_id,
        KnowledgeSourceKind::Manual,
        None,
        None,
        "Manual source",
        "Source summary",
    )).await.unwrap();
    ledger.append(OperationEvent::new_document_chunk_created(
        run_id,
        chunk_id,
        source_id,
        0,
        "searchable text",
        0,
        15,
    )).await.unwrap();
    ledger.append(OperationEvent::new_memory_recorded(
        run_id,
        memory_id,
        Some(source_id),
        MemoryKind::Fact,
        "Fact title",
        "Fact body",
        0.9,
    )).await.unwrap();
    ledger.append(OperationEvent::new_retrieval_trace_recorded(
        run_id,
        trace_id,
        "searchable",
        vec![chunk_id],
        vec![memory_id],
        "deterministic keyword retrieval matched searchable text",
    )).await.unwrap();

    let events = ledger.list().await.unwrap();
    assert_eq!(events.len(), 4);
    assert!(matches!(events[0].kind, OperationEventKind::KnowledgeSourceCreated { .. }));
    assert!(matches!(events[1].kind, OperationEventKind::DocumentChunkCreated { .. }));
    assert!(matches!(events[2].kind, OperationEventKind::MemoryRecorded { .. }));
    assert!(matches!(events[3].kind, OperationEventKind::RetrievalTraceRecorded { .. }));
}
```

- [ ] **Step 11: Verify Task 1**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-kernel knowledge && cargo test -p mneme-kernel knowledge_source_event_records_source_metadata && cargo test -p mneme-kernel retrieval_trace_event_records_result_ids && cargo test -p mneme-kernel sqlite_ledger_roundtrips_knowledge_memory_retrieval_events && git diff --check && git status --short --branch
```

Expected: all tests PASS; status shows only Task 1 files modified before commit.

- [ ] **Step 12: Commit**

```bash
git add crates/mneme-kernel/src/ids.rs crates/mneme-kernel/src/events.rs crates/mneme-kernel/src/knowledge.rs crates/mneme-kernel/src/lib.rs crates/mneme-kernel/src/storage/sqlite.rs
git commit -m "feat: add search memory knowledge event model"
```

---

### Task 2: Knowledge source projection and service

**Files:**
- Modify: `crates/mneme-kernel/migrations/0001_operation_ledger.sql`
- Modify: `crates/mneme-kernel/src/knowledge.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 IDs, records, `OperationEvent::new_knowledge_source_created`.
- Produces:
  - `pub struct CreateKnowledgeSourceOutcome { pub knowledge_source_id: KnowledgeSourceId }`
  - `pub enum KnowledgeServiceError { InvalidInput, UnknownRun, UnknownArtifact, UnknownContextBundle, UnknownKnowledgeSource, Ledger(String), Projection(String) }`
  - `pub trait KnowledgeSourceProjectionStore`
  - `pub struct InMemoryKnowledgeSourceProjectionStore`
  - `pub struct ComposedKnowledgeService`
  - `pub struct SqliteKnowledgeSourceProjectionStore`
  - `pub struct SqliteKnowledgeService`

- [ ] **Step 1: Write failing in-memory service test**

In `crates/mneme-kernel/src/knowledge.rs`, add:

```rust
#[tokio::test]
async fn composed_knowledge_service_creates_artifact_source() {
    let ledger = crate::InMemoryOperationLedger::new().shared();
    let run_store = crate::InMemoryRunProjectionStore::new().shared();
    let artifact_projection = crate::InMemoryArtifactProjectionStore::new().shared();
    let context_projection = crate::InMemoryContextBundleProjectionStore::new().shared();
    let source_projection = InMemoryKnowledgeSourceProjectionStore::new().shared();

    let run_service = crate::ComposedRunService::new(ledger.clone(), run_store.clone());
    let run = run_service.create_run("Create a source".to_string()).await.unwrap();
    let artifact_id = ArtifactId::new();
    artifact_projection
        .apply(&OperationEvent::new_artifact_created(
            run.run_id,
            artifact_id,
            "Run summary",
            crate::ArtifactKind::Markdown,
            "text/markdown",
            "abc123",
            12,
        ))
        .await
        .unwrap();

    let service = ComposedKnowledgeService::new(
        ledger.clone(),
        run_store,
        artifact_projection,
        context_projection,
        source_projection,
    );

    let outcome = service
        .create_source_from_artifact(artifact_id, "Run summary".to_string(), "Summary source".to_string())
        .await
        .unwrap();

    let sources = service.list_sources().await.unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].knowledge_source_id, outcome.knowledge_source_id);
    assert_eq!(sources[0].run_id, run.run_id);
    assert_eq!(sources[0].source_kind, KnowledgeSourceKind::Artifact);
    assert_eq!(sources[0].artifact_id, Some(artifact_id));
}
```

- [ ] **Step 2: Run in-memory test and verify RED**

Run:

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-kernel composed_knowledge_service_creates_artifact_source
```

Expected: FAIL with missing `ComposedKnowledgeService` or `InMemoryKnowledgeSourceProjectionStore`.

- [ ] **Step 3: Implement in-memory projection and service**

In `crates/mneme-kernel/src/knowledge.rs`, implement:

```rust
pub type SharedKnowledgeSourceProjectionStore = Arc<dyn KnowledgeSourceProjectionStore>;

#[async_trait]
pub trait KnowledgeSourceProjectionStore: Send + Sync {
    async fn apply(&self, event: &OperationEvent) -> Result<(), KnowledgeServiceError>;
    async fn list_sources(&self) -> Result<Vec<KnowledgeSourceRecord>, KnowledgeServiceError>;
    async fn get_source(&self, knowledge_source_id: KnowledgeSourceId) -> Result<KnowledgeSourceRecord, KnowledgeServiceError>;
}
```

`InMemoryKnowledgeSourceProjectionStore` stores `Vec<KnowledgeSourceRecord>` behind `Arc<RwLock<_>>`, applies only `OperationEventKind::KnowledgeSourceCreated`, and returns `UnknownKnowledgeSource` for missing get.

`ComposedKnowledgeService` must validate:

- title is non-blank.
- artifact source references an existing artifact via `ArtifactProjectionStore::get_artifact`.
- context bundle source references an existing bundle via `ContextBundleProjectionStore::get_context_bundle`.
- manual source uses the supplied `RunId` and verifies run exists via `RunProjectionStore::list()`.

Creation appends the ledger event, then applies projection.

- [ ] **Step 4: Add migration table**

In `crates/mneme-kernel/migrations/0001_operation_ledger.sql`, add:

```sql
create table if not exists knowledge_sources (
    knowledge_source_id text primary key,
    run_id text not null,
    source_kind text not null,
    artifact_id text,
    context_bundle_id text,
    title text not null,
    summary text not null,
    created_at text not null
);

create index if not exists idx_knowledge_sources_run_id_created_at
    on knowledge_sources(run_id, created_at);
```

- [ ] **Step 5: Write SQLite service and rebuild tests**

In `crates/mneme-kernel/src/storage/sqlite.rs`, add tests:

```rust
#[tokio::test]
async fn sqlite_knowledge_service_creates_source_projection_and_ledger_event() {
    let store = temp_sqlite_store("sqlite_knowledge_service_creates_source_projection_and_ledger_event").await;
    let run_service = SqliteRunService::new(store.clone());
    let run = run_service.create_run("Create source".to_string()).await.unwrap();
    let content_store = crate::InMemoryArtifactContentStore::new().shared();
    let artifact_service = SqliteArtifactService::new(store.clone(), content_store);
    let artifact = artifact_service
        .create_text_artifact(run.run_id, "Run summary".to_string(), crate::ArtifactKind::Markdown, "text/markdown".to_string(), "Searchable content".to_string())
        .await
        .unwrap();

    let service = SqliteKnowledgeService::new(store.clone());
    let outcome = service
        .create_source_from_artifact(artifact.artifact_id, "Run summary".to_string(), "Summary source".to_string())
        .await
        .unwrap();

    let sources = service.list_sources().await.unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].knowledge_source_id, outcome.knowledge_source_id);

    let events = SqliteOperationLedger::new(store).list().await.unwrap();
    assert!(events.iter().any(|event| matches!(event.kind, OperationEventKind::KnowledgeSourceCreated { knowledge_source_id, .. } if knowledge_source_id == outcome.knowledge_source_id)));
}

#[tokio::test]
async fn sqlite_ledger_roundtrips_knowledge_source_events_can_rebuild_projection() {
    let store = temp_sqlite_store("sqlite_ledger_roundtrips_knowledge_source_events_can_rebuild_projection").await;
    let ledger = SqliteOperationLedger::new(store.clone());
    let projection = SqliteKnowledgeSourceProjectionStore::new(store);
    let run_id = RunId::new();
    let source_id = KnowledgeSourceId::new();

    let event = OperationEvent::new_knowledge_source_created(
        run_id,
        source_id,
        KnowledgeSourceKind::Manual,
        None,
        None,
        "Manual source",
        "Manual summary",
    );
    ledger.append(event.clone()).await.unwrap();
    projection.rebuild(&[event]).await.unwrap();

    let sources = projection.list_sources().await.unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].knowledge_source_id, source_id);
}
```

- [ ] **Step 6: Run SQLite tests and verify RED**

Run:

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-kernel sqlite_knowledge_service_creates_source_projection_and_ledger_event sqlite_ledger_roundtrips_knowledge_source_events_can_rebuild_projection
```

Expected: FAIL with missing `SqliteKnowledgeService` or `SqliteKnowledgeSourceProjectionStore`.

- [ ] **Step 7: Implement SQLite projection and service**

In `crates/mneme-kernel/src/storage/sqlite.rs`, implement:

- `pub struct SqliteKnowledgeSourceProjectionStore { store: SqliteStore }`
- `pub struct SqliteKnowledgeService { store: SqliteStore }`
- `apply_knowledge_source_created_to_sqlite_projection(executor, event)` helper matching existing artifact/context helpers.
- `SqliteKnowledgeSourceProjectionStore::rebuild(&self, events: &[OperationEvent])` deletes `knowledge_sources`, then applies `KnowledgeSourceCreated` events.
- `SqliteKnowledgeService::create_source_from_artifact` verifies artifact exists, derives run id from artifact record, inserts ledger event, applies projection in the same transaction.
- `create_source_from_context_bundle` mirrors artifact path.
- `create_manual_source` verifies run exists in `runs`.

Map storage failures to `KnowledgeServiceError::Projection` or `Ledger` consistently.

- [ ] **Step 8: Export Task 2 interfaces**

In `crates/mneme-kernel/src/lib.rs`, export the new services, errors, stores, and outcomes.

- [ ] **Step 9: Verify Task 2**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-kernel knowledge_service && cargo test -p mneme-kernel sqlite_knowledge_service && cargo test -p mneme-kernel sqlite_ledger_roundtrips_knowledge_source && cargo test -p mneme-kernel && git diff --check && git status --short --branch
```

Expected: all PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/mneme-kernel/migrations/0001_operation_ledger.sql crates/mneme-kernel/src/knowledge.rs crates/mneme-kernel/src/storage/sqlite.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add knowledge source service"
```

---

### Task 3: Document chunking projection and service

**Files:**
- Modify: `crates/mneme-kernel/migrations/0001_operation_ledger.sql`
- Modify: `crates/mneme-kernel/src/knowledge.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: Task 2 `KnowledgeSourceRecord`, `KnowledgeSourceProjectionStore`, `SqliteKnowledgeService`.
- Produces:
  - `pub trait DocumentChunkProjectionStore`
  - `pub struct InMemoryDocumentChunkProjectionStore`
  - `pub struct SqliteDocumentChunkProjectionStore`
  - `pub struct ChunkTextSplitter { pub target_chars: usize, pub overlap_chars: usize }`
  - `pub struct ComposedChunkService`
  - `pub struct SqliteChunkService`
  - Methods `chunk_source(knowledge_source_id) -> Result<Vec<DocumentChunkRecord>, KnowledgeServiceError>` and `list_chunks(Option<KnowledgeSourceId>)`.

- [ ] **Step 1: Add deterministic splitter tests**

In `crates/mneme-kernel/src/knowledge.rs`:

```rust
#[test]
fn chunk_text_splitter_creates_ordered_overlapping_chunks() {
    let splitter = ChunkTextSplitter::new(12, 3);
    let chunks = splitter.split("abcdefghijklmnopqrstuv");

    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].sequence, 0);
    assert_eq!(chunks[0].text, "abcdefghijkl");
    assert_eq!(chunks[0].char_start, 0);
    assert_eq!(chunks[0].char_end, 12);
    assert_eq!(chunks[1].text, "jklmnopqrstu");
    assert_eq!(chunks[1].char_start, 9);
}
```

- [ ] **Step 2: Run splitter test and verify RED**

Run:

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-kernel chunk_text_splitter_creates_ordered_overlapping_chunks
```

Expected: FAIL because `ChunkTextSplitter` is missing.

- [ ] **Step 3: Implement splitter**

Define:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkDraft {
    pub sequence: i64,
    pub text: String,
    pub char_start: i64,
    pub char_end: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkTextSplitter {
    pub target_chars: usize,
    pub overlap_chars: usize,
}
```

`ChunkTextSplitter::new(target_chars, overlap_chars)` clamps `target_chars` to at least 1 and `overlap_chars` to less than target. `split(&self, text: &str)` uses `text.chars().collect::<Vec<_>>()`, slices by char index, and advances by `target_chars - overlap_chars`. Empty input returns an empty Vec.

- [ ] **Step 4: Add in-memory chunk service test**

```rust
#[tokio::test]
async fn composed_chunk_service_chunks_manual_source() {
    let ledger = crate::InMemoryOperationLedger::new().shared();
    let run_store = crate::InMemoryRunProjectionStore::new().shared();
    let artifact_projection = crate::InMemoryArtifactProjectionStore::new().shared();
    let context_projection = crate::InMemoryContextBundleProjectionStore::new().shared();
    let source_projection = InMemoryKnowledgeSourceProjectionStore::new().shared();
    let chunk_projection = InMemoryDocumentChunkProjectionStore::new().shared();

    let run_service = crate::ComposedRunService::new(ledger.clone(), run_store.clone());
    let run = run_service.create_run("Chunk source".to_string()).await.unwrap();
    let knowledge_service = ComposedKnowledgeService::new(
        ledger.clone(),
        run_store,
        artifact_projection,
        context_projection,
        source_projection.clone(),
    );
    let source = knowledge_service
        .create_manual_source(run.run_id, "Manual source".to_string(), "abcdefghijklmnopqrstuvwxyz".to_string())
        .await
        .unwrap();

    let chunk_service = ComposedChunkService::new(
        ledger.clone(),
        source_projection,
        chunk_projection,
        ChunkTextSplitter::new(10, 2),
    );
    let chunks = chunk_service.chunk_source(source.knowledge_source_id).await.unwrap();

    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].sequence, 0);
    assert_eq!(chunks[0].knowledge_source_id, source.knowledge_source_id);
    assert_eq!(chunks[0].run_id, run.run_id);
    assert_eq!(chunks[0].text, "abcdefghij");
}
```

- [ ] **Step 5: Run chunk service test and verify RED**

Run:

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-kernel composed_chunk_service_chunks_manual_source
```

Expected: FAIL with missing chunk projection/service.

- [ ] **Step 6: Implement in-memory chunk projection and service**

Add `SharedDocumentChunkProjectionStore`, `DocumentChunkProjectionStore`, `InMemoryDocumentChunkProjectionStore`, and `ComposedChunkService` to `knowledge.rs`.

`ComposedChunkService::chunk_source` must:

1. load source by ID;
2. use `source.summary` as deterministic text for Manual source;
3. use the same `source.summary` for Artifact and ContextBundle in M6 MVP because full artifact content retrieval is already owned by artifact services and Task 3 stays focused;
4. create one ledger event per chunk;
5. apply each chunk event to projection;
6. return created chunks.

- [ ] **Step 7: Add migration table**

Add:

```sql
create table if not exists document_chunks (
    document_chunk_id text primary key,
    knowledge_source_id text not null,
    run_id text not null,
    sequence integer not null,
    text text not null,
    char_start integer not null,
    char_end integer not null,
    created_at text not null
);

create index if not exists idx_document_chunks_source_sequence
    on document_chunks(knowledge_source_id, sequence);
```

- [ ] **Step 8: Add SQLite chunk tests**

In `storage/sqlite.rs`:

```rust
#[tokio::test]
async fn sqlite_chunk_service_chunks_source_and_records_ledger_events() {
    let store = temp_sqlite_store("sqlite_chunk_service_chunks_source_and_records_ledger_events").await;
    let run_service = SqliteRunService::new(store.clone());
    let run = run_service.create_run("Chunk source".to_string()).await.unwrap();
    let knowledge_service = SqliteKnowledgeService::new(store.clone());
    let source = knowledge_service
        .create_manual_source(run.run_id, "Manual source".to_string(), "abcdefghijklmnopqrstuvwxyz".to_string())
        .await
        .unwrap();

    let service = SqliteChunkService::new(store.clone(), ChunkTextSplitter::new(10, 2));
    let chunks = service.chunk_source(source.knowledge_source_id).await.unwrap();

    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].text, "abcdefghij");
    let events = SqliteOperationLedger::new(store).list().await.unwrap();
    assert!(events.iter().any(|event| matches!(event.kind, OperationEventKind::DocumentChunkCreated { document_chunk_id, .. } if document_chunk_id == chunks[0].document_chunk_id)));
}

#[tokio::test]
async fn sqlite_ledger_roundtrips_document_chunk_events_can_rebuild_projection() {
    let store = temp_sqlite_store("sqlite_ledger_roundtrips_document_chunk_events_can_rebuild_projection").await;
    let ledger = SqliteOperationLedger::new(store.clone());
    let projection = SqliteDocumentChunkProjectionStore::new(store);
    let run_id = RunId::new();
    let source_id = KnowledgeSourceId::new();
    let chunk_id = DocumentChunkId::new();
    let event = OperationEvent::new_document_chunk_created(run_id, chunk_id, source_id, 0, "searchable", 0, 10);

    ledger.append(event.clone()).await.unwrap();
    projection.rebuild(&[event]).await.unwrap();

    let chunks = projection.list_chunks(None).await.unwrap();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].document_chunk_id, chunk_id);
}
```

- [ ] **Step 9: Implement SQLite chunk projection and service**

Follow Task 2 patterns. `SqliteChunkService::chunk_source` loads source from `knowledge_sources`, splits `summary`, inserts one event per chunk, applies each chunk projection in the same transaction, and returns chunks.

- [ ] **Step 10: Verify Task 3**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-kernel chunk_text_splitter && cargo test -p mneme-kernel chunk_service && cargo test -p mneme-kernel sqlite_chunk_service && cargo test -p mneme-kernel sqlite_ledger_roundtrips_document_chunk && cargo test -p mneme-kernel && git diff --check && git status --short --branch
```

- [ ] **Step 11: Commit**

```bash
git add crates/mneme-kernel/migrations/0001_operation_ledger.sql crates/mneme-kernel/src/knowledge.rs crates/mneme-kernel/src/storage/sqlite.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add document chunk service"
```

---

### Task 4: Memory record projection and service

**Files:**
- Modify: `crates/mneme-kernel/migrations/0001_operation_ledger.sql`
- Modify: `crates/mneme-kernel/src/knowledge.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: Task 2 knowledge sources and Task 1 `MemoryRecord`.
- Produces:
  - `pub struct RecordMemoryOutcome { pub memory_id: MemoryId }`
  - `pub enum MemoryServiceError { InvalidInput, UnknownRun, UnknownKnowledgeSource, UnknownMemory, Ledger(String), Projection(String) }`
  - `pub trait MemoryProjectionStore`
  - `pub struct InMemoryMemoryProjectionStore`
  - `pub struct ComposedMemoryService`
  - `pub struct SqliteMemoryProjectionStore`
  - `pub struct SqliteMemoryService`

- [ ] **Step 1: Write in-memory memory service test**

```rust
#[tokio::test]
async fn composed_memory_service_records_and_lists_memory() {
    let ledger = crate::InMemoryOperationLedger::new().shared();
    let run_store = crate::InMemoryRunProjectionStore::new().shared();
    let source_projection = InMemoryKnowledgeSourceProjectionStore::new().shared();
    let memory_projection = InMemoryMemoryProjectionStore::new().shared();

    let run_service = crate::ComposedRunService::new(ledger.clone(), run_store.clone());
    let run = run_service.create_run("Record memory".to_string()).await.unwrap();

    let service = ComposedMemoryService::new(ledger, run_store, source_projection, memory_projection);
    let outcome = service
        .record_memory(run.run_id, None, MemoryKind::Decision, "Decision".to_string(), "Use SQLite-first search.".to_string(), 0.9)
        .await
        .unwrap();

    let memories = service.list_memories().await.unwrap();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].memory_id, outcome.memory_id);
    assert_eq!(memories[0].confidence, 0.9);
}
```

- [ ] **Step 2: Run test and verify RED**

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-kernel composed_memory_service_records_and_lists_memory
```

Expected: FAIL with missing memory service/projection.

- [ ] **Step 3: Implement memory projection and service**

In `knowledge.rs`, implement `MemoryProjectionStore`, in-memory store, and `ComposedMemoryService`.

Validation:

- title non-blank;
- body non-blank;
- confidence in `0.0..=1.0`;
- run exists;
- if `knowledge_source_id` is present, source exists.

Creation appends `MemoryRecorded`, applies projection, and returns `RecordMemoryOutcome`.

- [ ] **Step 4: Add migration table**

```sql
create table if not exists memory_records (
    memory_id text primary key,
    knowledge_source_id text,
    run_id text not null,
    kind text not null,
    title text not null,
    body text not null,
    confidence real not null,
    created_at text not null
);

create index if not exists idx_memory_records_source_created_at
    on memory_records(knowledge_source_id, created_at);
```

- [ ] **Step 5: Write SQLite tests**

```rust
#[tokio::test]
async fn sqlite_memory_service_records_memory_and_ledger_event() {
    let store = temp_sqlite_store("sqlite_memory_service_records_memory_and_ledger_event").await;
    let run_service = SqliteRunService::new(store.clone());
    let run = run_service.create_run("Record memory".to_string()).await.unwrap();
    let service = SqliteMemoryService::new(store.clone());

    let outcome = service
        .record_memory(run.run_id, None, MemoryKind::Fact, "Fact".to_string(), "Mneme uses operation ledger.".to_string(), 1.0)
        .await
        .unwrap();

    let memories = service.list_memories().await.unwrap();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].memory_id, outcome.memory_id);

    let events = SqliteOperationLedger::new(store).list().await.unwrap();
    assert!(events.iter().any(|event| matches!(event.kind, OperationEventKind::MemoryRecorded { memory_id, .. } if memory_id == outcome.memory_id)));
}

#[tokio::test]
async fn sqlite_memory_service_rejects_invalid_confidence() {
    let store = temp_sqlite_store("sqlite_memory_service_rejects_invalid_confidence").await;
    let run_service = SqliteRunService::new(store.clone());
    let run = run_service.create_run("Record memory".to_string()).await.unwrap();
    let service = SqliteMemoryService::new(store);

    let error = service
        .record_memory(run.run_id, None, MemoryKind::Fact, "Fact".to_string(), "Body".to_string(), 1.5)
        .await
        .unwrap_err();

    assert_eq!(error, MemoryServiceError::InvalidInput);
}
```

- [ ] **Step 6: Implement SQLite memory service**

Add `SqliteMemoryProjectionStore`, `SqliteMemoryService`, projection rebuild, and exports. Use JSON-free columns for scalar fields. Store `MemoryKind` as snake_case text.

- [ ] **Step 7: Verify Task 4**

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-kernel memory_service && cargo test -p mneme-kernel sqlite_memory_service && cargo test -p mneme-kernel sqlite_ledger_roundtrips_memory && cargo test -p mneme-kernel && git diff --check && git status --short --branch
```

- [ ] **Step 8: Commit**

```bash
git add crates/mneme-kernel/migrations/0001_operation_ledger.sql crates/mneme-kernel/src/knowledge.rs crates/mneme-kernel/src/storage/sqlite.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add memory record service"
```

---

### Task 5: Search index abstraction and retrieval service

**Files:**
- Modify: `crates/mneme-kernel/migrations/0001_operation_ledger.sql`
- Modify: `crates/mneme-kernel/src/knowledge.rs`
- Modify: `crates/mneme-kernel/src/storage/sqlite.rs`
- Modify: `crates/mneme-kernel/src/lib.rs`

**Interfaces:**
- Consumes: Task 3 chunks and Task 4 memories.
- Produces:
  - `pub struct SearchHit { pub document_chunk_id: DocumentChunkId, pub score: f32, pub reason: String }`
  - `pub struct VectorHit { pub document_chunk_id: DocumentChunkId, pub score: f32 }`
  - `pub trait SearchIndex`
  - `pub trait VectorIndex`
  - `pub struct InMemorySearchIndex`
  - `pub struct NoopVectorIndex`
  - `pub struct SearchResponse { pub query: String, pub hits: Vec<SearchResult>, pub retrieval_trace_id: RetrievalTraceId }`
  - `pub struct SearchResult { pub chunk: DocumentChunkRecord, pub source: KnowledgeSourceRecord, pub score: f32, pub reason: String }`
  - `pub enum RetrievalServiceError { InvalidInput, UnknownRun, Index(String), Ledger(String), Projection(String) }`
  - `pub struct ComposedRetrievalService`
  - `pub struct SqliteRetrievalService`

- [ ] **Step 1: Write in-memory search index test**

```rust
#[tokio::test]
async fn in_memory_search_index_returns_keyword_hits_with_reason() {
    let run_id = RunId::new();
    let source_id = KnowledgeSourceId::new();
    let chunk = DocumentChunkRecord {
        document_chunk_id: DocumentChunkId::new(),
        knowledge_source_id: source_id,
        run_id,
        sequence: 0,
        text: "SQLite-first deterministic retrieval".to_string(),
        char_start: 0,
        char_end: 36,
        created_at: OffsetDateTime::now_utc(),
    };
    let index = InMemorySearchIndex::new();
    index.rebuild(vec![chunk.clone()]).await.unwrap();

    let hits = index.search("deterministic", 10).await.unwrap();

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].document_chunk_id, chunk.document_chunk_id);
    assert!(hits[0].reason.contains("deterministic"));
}
```

- [ ] **Step 2: Run index test and verify RED**

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-kernel in_memory_search_index_returns_keyword_hits_with_reason
```

Expected: FAIL with missing `InMemorySearchIndex`.

- [ ] **Step 3: Implement index traits and in-memory search**

In `knowledge.rs`, use `async_trait` and implement:

- `SearchIndex::rebuild` replaces in-memory chunks.
- `SearchIndex::search` lowercases query and chunk text, returns chunks containing query, score `1.0`, reason `keyword '<query>' matched chunk text`, limited by `limit`.
- `NoopVectorIndex` returns an empty Vec for search and accepts upsert without storing anything.

- [ ] **Step 4: Add retrieval trace migration table**

```sql
create table if not exists retrieval_traces (
    retrieval_trace_id text primary key,
    run_id text not null,
    query text not null,
    result_chunk_ids_json text not null,
    result_memory_ids_json text not null,
    explanation text not null,
    created_at text not null
);

create index if not exists idx_retrieval_traces_run_id_created_at
    on retrieval_traces(run_id, created_at);
```

- [ ] **Step 5: Write retrieval service tests**

```rust
#[tokio::test]
async fn composed_retrieval_service_searches_chunks_and_records_trace() {
    let ledger = crate::InMemoryOperationLedger::new().shared();
    let run_store = crate::InMemoryRunProjectionStore::new().shared();
    let source_projection = InMemoryKnowledgeSourceProjectionStore::new().shared();
    let chunk_projection = InMemoryDocumentChunkProjectionStore::new().shared();
    let memory_projection = InMemoryMemoryProjectionStore::new().shared();
    let trace_projection = InMemoryRetrievalTraceProjectionStore::new().shared();

    let run_service = crate::ComposedRunService::new(ledger.clone(), run_store.clone());
    let run = run_service.create_run("Search".to_string()).await.unwrap();
    let source_id = KnowledgeSourceId::new();
    source_projection.apply(&OperationEvent::new_knowledge_source_created(run.run_id, source_id, KnowledgeSourceKind::Manual, None, None, "Manual", "SQLite deterministic retrieval")).await.unwrap();
    let chunk_id = DocumentChunkId::new();
    chunk_projection.apply(&OperationEvent::new_document_chunk_created(run.run_id, chunk_id, source_id, 0, "SQLite deterministic retrieval", 0, 30)).await.unwrap();

    let service = ComposedRetrievalService::new(
        ledger,
        run_store,
        source_projection,
        chunk_projection,
        memory_projection,
        trace_projection,
        InMemorySearchIndex::new().shared(),
    );

    let response = service.search(run.run_id, "deterministic".to_string(), 5).await.unwrap();

    assert_eq!(response.query, "deterministic");
    assert_eq!(response.hits.len(), 1);
    assert_eq!(response.hits[0].chunk.document_chunk_id, chunk_id);
    let traces = service.list_retrieval_traces().await.unwrap();
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].retrieval_trace_id, response.retrieval_trace_id);
}
```

- [ ] **Step 6: Implement retrieval trace projection and composed service**

Add `RetrievalTraceProjectionStore`, `InMemoryRetrievalTraceProjectionStore`, `ComposedRetrievalService`. Search validation:

- query trimmed non-empty;
- limit in `1..=50`; if caller passes 0, return `InvalidInput`;
- run exists;
- explanation exactly starts with `M6 deterministic keyword retrieval`.

- [ ] **Step 7: Write SQLite retrieval tests**

```rust
#[tokio::test]
async fn sqlite_retrieval_service_searches_chunks_and_records_trace() {
    let store = temp_sqlite_store("sqlite_retrieval_service_searches_chunks_and_records_trace").await;
    let run_service = SqliteRunService::new(store.clone());
    let run = run_service.create_run("Search".to_string()).await.unwrap();
    let knowledge_service = SqliteKnowledgeService::new(store.clone());
    let source = knowledge_service.create_manual_source(run.run_id, "Manual".to_string(), "SQLite deterministic retrieval".to_string()).await.unwrap();
    let chunk_service = SqliteChunkService::new(store.clone(), ChunkTextSplitter::new(1000, 0));
    chunk_service.chunk_source(source.knowledge_source_id).await.unwrap();

    let service = SqliteRetrievalService::new(store.clone());
    let response = service.search(run.run_id, "deterministic".to_string(), 10).await.unwrap();

    assert_eq!(response.hits.len(), 1);
    assert!(response.hits[0].reason.contains("deterministic"));
    let traces = service.list_retrieval_traces().await.unwrap();
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].retrieval_trace_id, response.retrieval_trace_id);
}
```

- [ ] **Step 8: Implement SQLite retrieval service**

`SqliteRetrievalService::search` queries `document_chunks` with `lower(text) like '%' || lower(?1) || '%'`, joins sources by `knowledge_source_id`, limits results, inserts `RetrievalTraceRecorded` event and retrieval trace projection in one transaction, and returns a `SearchResponse`.

- [ ] **Step 9: Verify Task 5**

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-kernel search_index && cargo test -p mneme-kernel retrieval_service && cargo test -p mneme-kernel sqlite_retrieval_service && cargo test -p mneme-kernel sqlite_ledger_roundtrips_retrieval && cargo test -p mneme-kernel && git diff --check && git status --short --branch
```

- [ ] **Step 10: Commit**

```bash
git add crates/mneme-kernel/migrations/0001_operation_ledger.sql crates/mneme-kernel/src/knowledge.rs crates/mneme-kernel/src/storage/sqlite.rs crates/mneme-kernel/src/lib.rs
git commit -m "feat: add deterministic retrieval service"
```

---

### Task 6: HTTP API and daemon wiring

**Files:**
- Modify: `crates/mneme-api/src/types.rs`
- Modify: `crates/mneme-api/src/routes.rs`
- Modify: `crates/mneme-daemon/src/main.rs`

**Interfaces:**
- Consumes: Kernel services from Tasks 2–5.
- Produces API routes:
  - `GET /api/v1/knowledge-sources`
  - `POST /api/v1/artifacts/:artifact_id/knowledge-source`
  - `POST /api/v1/context-bundles/:context_bundle_id/knowledge-source`
  - `POST /api/v1/knowledge-sources/manual`
  - `GET /api/v1/document-chunks`
  - `POST /api/v1/knowledge-sources/:knowledge_source_id/chunks`
  - `GET /api/v1/memories`
  - `POST /api/v1/memories`
  - `POST /api/v1/search`
  - `GET /api/v1/retrieval-traces`

- [ ] **Step 1: Add DTOs to `types.rs`**

Define:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateKnowledgeSourceRequest {
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateManualKnowledgeSourceRequest {
    pub run_id: String,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateKnowledgeSourceResponse {
    pub knowledge_source_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeSourcesResponse {
    pub knowledge_sources: Vec<mneme_kernel::KnowledgeSourceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentChunksResponse {
    pub document_chunks: Vec<mneme_kernel::DocumentChunkRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordMemoryRequest {
    pub run_id: String,
    pub knowledge_source_id: Option<String>,
    pub kind: mneme_kernel::MemoryKind,
    pub title: String,
    pub body: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordMemoryResponse {
    pub memory_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoriesResponse {
    pub memories: Vec<mneme_kernel::MemoryRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRequest {
    pub run_id: String,
    pub query: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResponseDto {
    pub query: String,
    pub hits: Vec<mneme_kernel::SearchResult>,
    pub retrieval_trace_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetrievalTracesResponse {
    pub retrieval_traces: Vec<mneme_kernel::RetrievalTraceRecord>,
}
```

- [ ] **Step 2: Write API tests before routes**

In `routes.rs`, add tests:

```rust
#[tokio::test]
async fn api_creates_chunks_searches_and_lists_retrieval_traces() {
    let app = build_router_with_in_memory_services();
    let run_id = create_run_for_test(&app, "Search run").await;
    let artifact_id = create_artifact_for_test(&app, &run_id, "Searchable artifact", "SQLite deterministic retrieval").await;

    let source_response: CreateKnowledgeSourceResponse = response_json(
        app.clone()
            .oneshot(json_request(
                Method::POST,
                &format!("/api/v1/artifacts/{artifact_id}/knowledge-source"),
                serde_json::json!({"title":"Searchable artifact","summary":"SQLite deterministic retrieval"}),
            ))
            .await
            .unwrap(),
    ).await;

    let chunks_response: DocumentChunksResponse = response_json(
        app.clone()
            .oneshot(empty_request(Method::POST, &format!("/api/v1/knowledge-sources/{}/chunks", source_response.knowledge_source_id)))
            .await
            .unwrap(),
    ).await;
    assert_eq!(chunks_response.document_chunks.len(), 1);

    let search_response: SearchResponseDto = response_json(
        app.clone()
            .oneshot(json_request(
                Method::POST,
                "/api/v1/search",
                serde_json::json!({"run_id":run_id,"query":"deterministic","limit":5}),
            ))
            .await
            .unwrap(),
    ).await;
    assert_eq!(search_response.hits.len(), 1);

    let traces: RetrievalTracesResponse = response_json(
        app.oneshot(empty_request(Method::GET, "/api/v1/retrieval-traces")).await.unwrap(),
    ).await;
    assert_eq!(traces.retrieval_traces.len(), 1);
}

#[tokio::test]
async fn api_rejects_blank_search_query() {
    let app = build_router_with_in_memory_services();
    let run_id = create_run_for_test(&app, "Search run").await;
    let response = app
        .oneshot(json_request(Method::POST, "/api/v1/search", serde_json::json!({"run_id":run_id,"query":"   ","limit":5})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 3: Run API tests and verify RED**

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-api knowledge search retrieval_traces
```

Expected: FAIL because M6 routes are missing.

- [ ] **Step 4: Add route-level shared service enums**

Follow existing artifact/context pattern in `routes.rs`. If Kernel aliases do not exist, add route-local enums:

```rust
pub enum SharedKnowledgeService {
    Composed(ComposedKnowledgeService),
    Sqlite(SqliteKnowledgeService),
}
```

Repeat for chunk, memory, retrieval services. Add these services to `ApiState` and update `build_router_with_all_services` to accept them.

- [ ] **Step 5: Implement routes and error mapping**

Implement handlers that parse IDs with `FromStr`, map invalid parse to `400 BAD_REQUEST`, map unknown resources to `404 NOT_FOUND`, and map ledger/projection/index errors to `500 INTERNAL_SERVER_ERROR`.

Search route must default missing limit to `10` and pass through explicit limit.

- [ ] **Step 6: Wire daemon**

In `crates/mneme-daemon/src/main.rs`, instantiate SQLite M6 services using the same `SqliteStore` and pass them to `build_router_with_all_services`. Do not introduce new environment variables.

- [ ] **Step 7: Verify Task 6**

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check && cargo test -p mneme-api knowledge && cargo test -p mneme-api search && cargo test -p mneme-api retrieval && cargo test -p mneme-api && cargo test --workspace && git diff --check && git status --short --branch
```

- [ ] **Step 8: Commit**

```bash
git add crates/mneme-api/src/types.rs crates/mneme-api/src/routes.rs crates/mneme-daemon/src/main.rs
git commit -m "feat: expose search memory knowledge API"
```

---

### Task 7: TypeScript API client

**Files:**
- Modify: `apps/desktop/src/api/client.ts`
- Modify: `apps/desktop/src/api/client.test.ts`
- Modify: `apps/desktop/src/app/App.test.tsx`

**Interfaces:**
- Consumes: Task 6 API.
- Produces non-optional `MnemeApiClient` methods:
  - `listKnowledgeSources()`
  - `createKnowledgeSourceFromArtifact(artifactId, request)`
  - `createKnowledgeSourceFromContextBundle(contextBundleId, request)`
  - `createManualKnowledgeSource(request)`
  - `listDocumentChunks()`
  - `chunkKnowledgeSource(knowledgeSourceId)`
  - `listMemories()`
  - `recordMemory(request)`
  - `search(request)`
  - `listRetrievalTraces()`

- [ ] **Step 1: Add failing client tests**

In `apps/desktop/src/api/client.test.ts`, add tests:

```ts
it('calls knowledge source and chunk endpoints', async () => {
  const fetchMock = vi.fn(async () => jsonResponse({ knowledge_source_id: 'source-1' }));
  const client = createMnemeApiClient('http://localhost:3000/', fetchMock as unknown as typeof fetch);

  await client.createKnowledgeSourceFromArtifact('artifact-1', {
    title: 'Run summary',
    summary: 'SQLite deterministic retrieval',
  });

  expect(fetchMock).toHaveBeenCalledWith('http://localhost:3000/api/v1/artifacts/artifact-1/knowledge-source', {
    method: 'POST',
    headers: { accept: 'application/json', 'content-type': 'application/json' },
    body: JSON.stringify({ title: 'Run summary', summary: 'SQLite deterministic retrieval' }),
  });
});

it('calls search and retrieval trace endpoints', async () => {
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(jsonResponse({ query: 'sqlite', hits: [], retrieval_trace_id: 'trace-1' }))
    .mockResolvedValueOnce(jsonResponse({ retrieval_traces: [] }));
  const client = createMnemeApiClient('http://localhost:3000', fetchMock as unknown as typeof fetch);

  await client.search({ run_id: 'run-1', query: 'sqlite', limit: 5 });
  await client.listRetrievalTraces();

  expect(fetchMock).toHaveBeenNthCalledWith(1, 'http://localhost:3000/api/v1/search', {
    method: 'POST',
    headers: { accept: 'application/json', 'content-type': 'application/json' },
    body: JSON.stringify({ run_id: 'run-1', query: 'sqlite', limit: 5 }),
  });
  expect(fetchMock).toHaveBeenNthCalledWith(2, 'http://localhost:3000/api/v1/retrieval-traces', {
    headers: { accept: 'application/json' },
  });
});
```

- [ ] **Step 2: Run client tests and verify RED**

```bash
pnpm --dir apps/desktop test -- src/api/client.test.ts
```

Expected: FAIL with methods not found.

- [ ] **Step 3: Add TS types and methods**

In `client.ts`, add exact type names from the spec and extend `MnemeApiClient`. Implement methods with existing `parseJsonResponse` helper and normalized base URL.

Paths must exactly match Task 6 routes.

- [ ] **Step 4: Update App test double**

In `apps/desktop/src/app/App.test.tsx`, add all new non-optional methods to the fake `MnemeApiClient`. Use resolved empty responses:

```ts
listKnowledgeSources: vi.fn(async () => ({ knowledge_sources: [] })),
createKnowledgeSourceFromArtifact: vi.fn(async () => ({ knowledge_source_id: 'source-1' })),
createKnowledgeSourceFromContextBundle: vi.fn(async () => ({ knowledge_source_id: 'source-1' })),
createManualKnowledgeSource: vi.fn(async () => ({ knowledge_source_id: 'source-1' })),
listDocumentChunks: vi.fn(async () => ({ document_chunks: [] })),
chunkKnowledgeSource: vi.fn(async () => ({ document_chunks: [] })),
listMemories: vi.fn(async () => ({ memories: [] })),
recordMemory: vi.fn(async () => ({ memory_id: 'memory-1' })),
search: vi.fn(async () => ({ query: 'sqlite', hits: [], retrieval_trace_id: 'trace-1' })),
listRetrievalTraces: vi.fn(async () => ({ retrieval_traces: [] })),
```

- [ ] **Step 5: Verify Task 7**

```bash
pnpm --dir apps/desktop check && pnpm --dir apps/desktop test -- src/api/client.test.ts && . "$HOME/.cargo/env" && pnpm check && git diff --check && git status --short --branch
```

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/api/client.ts apps/desktop/src/api/client.test.ts apps/desktop/src/app/App.test.tsx
git commit -m "feat: add search memory knowledge API client"
```

---

### Task 8: Workbench Knowledge Search, Memory Inspector, and Retrieval Traces

**Files:**
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Modify: `apps/desktop/src/styles.css`

**Interfaces:**
- Consumes: Task 7 client methods.
- Produces UI sections:
  - `Knowledge Search`
  - `Memory Inspector`
  - `Retrieval Traces`

- [ ] **Step 1: Write failing UI test**

In `App.test.tsx`, add fixture data and assertions:

```ts
it('renders knowledge search memory and retrieval trace sections', async () => {
  const client = createTestClient({
    knowledgeSources: [
      {
        knowledge_source_id: 'source-1',
        run_id: 'run-123',
        source_kind: 'artifact',
        artifact_id: 'artifact-1',
        context_bundle_id: null,
        title: 'Run summary source',
        summary: 'SQLite deterministic retrieval source',
        created_at: '2026-11-10T00:00:00Z',
      },
    ],
    documentChunks: [
      {
        document_chunk_id: 'chunk-1',
        knowledge_source_id: 'source-1',
        run_id: 'run-123',
        sequence: 0,
        text: 'SQLite deterministic retrieval chunk',
        char_start: 0,
        char_end: 36,
        created_at: '2026-11-10T00:00:00Z',
      },
    ],
    memories: [
      {
        memory_id: 'memory-1',
        knowledge_source_id: 'source-1',
        run_id: 'run-123',
        kind: 'decision',
        title: 'Use SQLite-first search',
        body: 'Search sidecars are rebuildable.',
        confidence: 0.9,
        created_at: '2026-11-10T00:00:00Z',
      },
    ],
    retrievalTraces: [
      {
        retrieval_trace_id: 'trace-1',
        run_id: 'run-123',
        query: 'sqlite',
        result_chunk_ids: ['chunk-1'],
        result_memory_ids: ['memory-1'],
        explanation: 'M6 deterministic keyword retrieval matched sqlite',
        created_at: '2026-11-10T00:00:00Z',
      },
    ],
  });

  render(<App apiClient={client} />);

  expect(await screen.findByText('Knowledge Search')).toBeInTheDocument();
  expect(screen.getByText('Run summary source')).toBeInTheDocument();
  expect(screen.getByText('SQLite deterministic retrieval chunk')).toBeInTheDocument();
  expect(screen.getByText('Memory Inspector')).toBeInTheDocument();
  expect(screen.getByText('Use SQLite-first search')).toBeInTheDocument();
  expect(screen.getByText('Retrieval Traces')).toBeInTheDocument();
  expect(screen.getByText('M6 deterministic keyword retrieval matched sqlite')).toBeInTheDocument();
  expect(screen.getByText(/deterministic keyword retrieval/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run UI test and verify RED**

```bash
pnpm --dir apps/desktop test -- src/app/App.test.tsx
```

Expected: FAIL because new sections are missing.

- [ ] **Step 3: Load data in App**

In `App.tsx`, extend state loading to call:

- `listKnowledgeSources()`
- `listDocumentChunks()`
- `listMemories()`
- `listRetrievalTraces()`

Do not call `search()` automatically on load. Display existing retrieval traces and chunks.

- [ ] **Step 4: Render read-only sections**

Add sections after Context Inspector:

- `Knowledge Search`: input disabled or static explanatory state plus existing chunks/sources. Include copy: `Milestone 6 search uses deterministic keyword retrieval, not semantic embeddings.`
- `Memory Inspector`: list memory title, kind, confidence, body preview.
- `Retrieval Traces`: list query, result counts, explanation.

Keep UI read-only; no create memory form in M6 UI task.

- [ ] **Step 5: Add styles**

In `styles.css`, add classes for `.knowledge-grid`, `.knowledge-card`, `.trace-card`, `.memory-card`, following existing card spacing and typography.

- [ ] **Step 6: Verify Task 8**

```bash
pnpm --dir apps/desktop check && pnpm --dir apps/desktop test -- src/app/App.test.tsx && . "$HOME/.cargo/env" && pnpm check && git diff --check && git status --short --branch
```

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src/app/App.tsx apps/desktop/src/app/App.test.tsx apps/desktop/src/styles.css
git commit -m "feat: show search memory knowledge inspector"
```

---

### Task 9: Documentation

**Files:**
- Modify: `README.md`
- Create: `docs/architecture/milestone-6-search-memory-knowledge.md`
- Modify: `docs/architecture/phase-1-foundation.md`

**Interfaces:**
- Consumes: completed M6 implementation.
- Produces M6 docs with scope, data flow, API examples, non-goals, and M7 handoff.

- [ ] **Step 1: Write architecture doc**

Create `docs/architecture/milestone-6-search-memory-knowledge.md` with sections:

```markdown
# Milestone 6：Search / Memory / Knowledge

## Scope

Milestone 6 adds SQLite-first explainable retrieval and knowledge metadata around existing Operation Ledger, Artifact, and ContextBundle objects.

## Event Flow

Run -> Artifact/ContextBundle -> KnowledgeSourceCreated -> DocumentChunkCreated -> MemoryRecorded -> RetrievalTraceRecorded.

## SQLite-first Retrieval

Search uses deterministic keyword retrieval over `document_chunks`. Search and vector indexes are sidecars and can be rebuilt from ledger events and SQLite read models.

## Non-goals

Milestone 6 does not provide real embeddings, Tantivy production indexing, vector database integration, worker runtime, automatic long-term memory, or agent prompt injection.

## Verification

Run `. "$HOME/.cargo/env" && pnpm check` and `git diff --check`.
```

Expand each section with the actual endpoint names and table names from this implementation.

- [ ] **Step 2: Update README with curl examples**

Add a Milestone 6 section with examples for:

```bash
curl -s http://127.0.0.1:3000/api/v1/knowledge-sources
curl -s -X POST http://127.0.0.1:3000/api/v1/knowledge-sources/manual \
  -H 'content-type: application/json' \
  -d '{"run_id":"<run-id>","title":"Manual source","summary":"SQLite deterministic retrieval"}'
curl -s -X POST http://127.0.0.1:3000/api/v1/search \
  -H 'content-type: application/json' \
  -d '{"run_id":"<run-id>","query":"sqlite","limit":5}'
curl -s http://127.0.0.1:3000/api/v1/retrieval-traces
```

Use `<run-id>` literally in docs examples.

- [ ] **Step 3: Update phase doc**

In `docs/architecture/phase-1-foundation.md`, add a line that Milestone 6 adds SQLite-first knowledge sources, document chunks, memory metadata, deterministic retrieval, and retrieval traces, while M7 owns worker/embedding execution.

- [ ] **Step 4: Verify docs**

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
git status --short --branch
```

- [ ] **Step 5: Commit**

```bash
git add README.md docs/architecture/milestone-6-search-memory-knowledge.md docs/architecture/phase-1-foundation.md
git commit -m "docs: document search memory knowledge milestone"
```

---

### Task 10: Final verification and broad review

**Files:**
- No code changes expected.
- Read: `.superpowers/sdd/progress.md`
- Create review package under `.superpowers/sdd/` using the SDD review-package script.

**Interfaces:**
- Consumes: Tasks 1–9.
- Produces final Milestone 6 verification evidence and review approval.

- [ ] **Step 1: Run final verification**

Use M6 base commit `a89fc16`.

```bash
. "$HOME/.cargo/env" && git status --short --branch && git log --oneline a89fc16..HEAD && pnpm check && git diff --check && git diff --stat a89fc16..HEAD && git status --short --branch
```

Expected:

- status clean before and after;
- `pnpm check` passes Rust workspace and desktop tests;
- `git diff --check` passes;
- diff stat includes kernel knowledge/search/memory work, API routes/types, desktop client/UI, docs.

- [ ] **Step 2: Build review package**

Run the SDD review package script from the `subagent-driven-development` skill:

```bash
bash /Users/baiye/.agents/skills/subagent-driven-development/scripts/review-package a89fc16 HEAD .superpowers/sdd/review-a89fc16..HEAD.diff
```

Expected: package file exists and contains the full Milestone 6 diff.

- [ ] **Step 3: Dispatch broad review**

Ask reviewer to inspect all M6 success criteria:

- Kernel exports knowledge/search/memory/retrieval types and services.
- Operation Ledger serializes, persists, and replays M6 events.
- SQLite read models rebuild from ledger events.
- Artifact/context objects can be promoted to knowledge sources and chunked.
- Search API returns explainable hits and records retrieval traces.
- Memory metadata can be explicitly created, listed, and linked to source.
- SearchIndex / VectorIndex traits exist, with deterministic implementation and no real embedding claim.
- HTTP API, TS client, Workbench form an end-to-end vertical slice.
- UI/docs do not claim semantic embedding, Tantivy production index, automatic long-term memory, or agent recall.
- `. "$HOME/.cargo/env" && pnpm check` and `git diff --check` pass.

- [ ] **Step 4: Append final progress ledger line after approval**

Only after broad review approves with no Critical or Important findings, append to `.superpowers/sdd/progress.md`:

```markdown
Milestone 6: complete (commits a89fc16..HEAD, final verification passed, overall review approved; follow-ups for Milestone 7: connect Worker Runtime to M6 service boundaries for embeddings, document parsing, and asynchronous reindexing without changing M6 ledger facts)
```

- [ ] **Step 5: Leave branch clean**

```bash
git status --short --branch
```

Expected: clean except `.superpowers` remains gitignored.
