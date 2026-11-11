use crate::{
    ArtifactId, ArtifactKind, ArtifactProjectionStore, ArtifactRecord, ArtifactServiceError,
    ChunkTextSplitter, ContextBundleId, ContextBundleProjectionStore, ContextBundleRecord,
    ContextServiceError, CreateArtifactOutcome, CreateContextBundleOutcome,
    CreateKnowledgeSourceOutcome, CreateRunOutcome, DocumentChunkProjectionStore,
    DocumentChunkRecord, EventId, KnowledgeServiceError, KnowledgeSourceId, KnowledgeSourceKind,
    KnowledgeSourceProjectionStore, KnowledgeSourceRecord, LedgerError, MemoryId, MemoryKind,
    MemoryProjectionStore, MemoryRecord, MemoryServiceError, OperationEvent, OperationEventKind,
    OperationLedger, RecordMemoryOutcome, RetrievalServiceError, RetrievalTraceProjectionStore,
    RetrievalTraceRecord, RunId, RunLifecycleOutcome, RunProjection, RunProjectionError,
    RunProjectionStore, RunProjector, RunService, RunServiceError, RunStatus, SearchResponse,
    SearchResult, SharedArtifactContentStore, SharedOperationLedger, SharedRunProjectionStore,
    SharedRunService, ToolApprovalDecision, ToolApprovalPolicy, ToolCallId, ToolCallOutcome,
    ToolEffect, ToolLifecycleOutcome, WorkerJobKind, WorkerJobProjectionStore, WorkerJobRecord,
    WorkerJobStatus, WorkerRuntimeError,
};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::{sqlite::SqlitePoolOptions, Sqlite, SqlitePool, Transaction};
use std::{str::FromStr, sync::Arc};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

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

    pub async fn list_events(&self) -> Result<Vec<OperationEvent>, LedgerError> {
        SqliteOperationLedger::new(self.clone()).list().await
    }
}

#[derive(Clone)]
pub struct SqliteArtifactService {
    store: SqliteStore,
    content_store: SharedArtifactContentStore,
}

impl SqliteArtifactService {
    pub fn new(store: SqliteStore, content_store: SharedArtifactContentStore) -> Self {
        Self {
            store,
            content_store,
        }
    }

    pub async fn create_text_artifact(
        &self,
        run_id: RunId,
        title: String,
        kind: ArtifactKind,
        mime_type: String,
        content: String,
    ) -> Result<CreateArtifactOutcome, ArtifactServiceError> {
        let title = non_blank_artifact_input(title)?;
        let mime_type = non_blank_artifact_input(mime_type)?;
        if content.trim().is_empty() {
            return Err(ArtifactServiceError::InvalidInput);
        }

        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| ArtifactServiceError::Ledger(error.to_string()))?;

        let run_exists: Option<String> =
            sqlx::query_scalar("select run_id from runs where run_id = ?1")
                .bind(run_id.to_string())
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?;
        if run_exists.is_none() {
            return Err(ArtifactServiceError::UnknownRun);
        }

        let stored = self
            .content_store
            .put(content.into_bytes())
            .await
            .map_err(|error| ArtifactServiceError::ContentStore(error.to_string()))?;
        let artifact_id = ArtifactId::new();
        let event = OperationEvent::new_artifact_created(
            run_id,
            artifact_id,
            title.clone(),
            kind,
            mime_type.clone(),
            stored.sha256.clone(),
            stored.byte_len,
        );
        let occurred_at = event
            .occurred_at
            .format(&Rfc3339)
            .map_err(|error| ArtifactServiceError::Ledger(error.to_string()))?;
        let payload_json = serde_json::to_string(&event.kind)
            .map_err(|error| ArtifactServiceError::Ledger(error.to_string()))?;

        sqlx::query(
            "insert into operation_events (event_id, run_id, occurred_at, kind_type, payload_json) values (?1, ?2, ?3, 'artifact_created', ?4)",
        )
        .bind(event.id.to_string())
        .bind(run_id.to_string())
        .bind(&occurred_at)
        .bind(payload_json)
        .execute(&mut *tx)
        .await
        .map_err(|error| ArtifactServiceError::Ledger(error.to_string()))?;

        apply_artifact_created_to_sqlite_projection(&mut tx, &event).await?;
        update_running_run_for_artifact_created(&mut tx, &event).await?;

        tx.commit()
            .await
            .map_err(|error| ArtifactServiceError::Ledger(error.to_string()))?;

        Ok(CreateArtifactOutcome { artifact_id })
    }

    pub async fn rebuild_artifact_projections(
        &self,
    ) -> Result<Vec<ArtifactRecord>, ArtifactServiceError> {
        let events = self
            .store
            .list_events()
            .await
            .map_err(|error| ArtifactServiceError::Ledger(error.to_string()))?;
        SqliteArtifactProjectionStore::new(self.store.clone())
            .rebuild(&events)
            .await?;
        self.list_artifacts().await
    }

    pub async fn list_artifacts(&self) -> Result<Vec<ArtifactRecord>, ArtifactServiceError> {
        let rows = sqlx::query_as::<_, StoredArtifactRecord>(
            "select artifact_id, run_id, title, kind, mime_type, sha256, byte_len, created_at from artifacts order by created_at desc",
        )
        .fetch_all(self.store.pool())
        .await
        .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?;

        rows.into_iter().map(ArtifactRecord::try_from).collect()
    }

    pub async fn get_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> Result<crate::ArtifactDetail, ArtifactServiceError> {
        let row = sqlx::query_as::<_, StoredArtifactRecord>(
            "select artifact_id, run_id, title, kind, mime_type, sha256, byte_len, created_at from artifacts where artifact_id = ?1",
        )
        .bind(artifact_id.to_string())
        .fetch_optional(self.store.pool())
        .await
        .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?
        .ok_or(ArtifactServiceError::UnknownArtifact)?;
        let artifact = ArtifactRecord::try_from(row)?;
        let bytes = self
            .content_store
            .get(&artifact.sha256)
            .await
            .map_err(|error| ArtifactServiceError::ContentStore(error.to_string()))?;
        let content = String::from_utf8(bytes).ok();

        Ok(crate::ArtifactDetail { artifact, content })
    }
}

#[derive(Clone, Debug)]
pub struct SqliteContextService {
    store: SqliteStore,
}

impl SqliteContextService {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub async fn create_context_bundle(
        &self,
        run_id: RunId,
        title: String,
        summary: String,
        artifact_ids: Vec<ArtifactId>,
        event_ids: Vec<EventId>,
    ) -> Result<CreateContextBundleOutcome, ContextServiceError> {
        let title = non_blank_context_input(title)?;
        let summary = non_blank_context_input(summary)?;
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| ContextServiceError::Ledger(error.to_string()))?;

        ensure_sqlite_context_references(&mut tx, run_id, &artifact_ids, &event_ids).await?;

        let context_bundle_id = ContextBundleId::new();
        let event = OperationEvent::new_context_bundle_created(
            run_id,
            context_bundle_id,
            title,
            summary,
            artifact_ids,
            event_ids,
        );
        let occurred_at = event
            .occurred_at
            .format(&Rfc3339)
            .map_err(|error| ContextServiceError::Ledger(error.to_string()))?;
        let payload_json = serde_json::to_string(&event.kind)
            .map_err(|error| ContextServiceError::Ledger(error.to_string()))?;

        sqlx::query(
            "insert into operation_events (event_id, run_id, occurred_at, kind_type, payload_json) values (?1, ?2, ?3, 'context_bundle_created', ?4)",
        )
        .bind(event.id.to_string())
        .bind(run_id.to_string())
        .bind(&occurred_at)
        .bind(payload_json)
        .execute(&mut *tx)
        .await
        .map_err(|error| ContextServiceError::Ledger(error.to_string()))?;

        apply_context_bundle_created_to_sqlite_projection(&mut tx, &event).await?;
        update_running_run_for_context_bundle_created(&mut tx, &event).await?;

        tx.commit()
            .await
            .map_err(|error| ContextServiceError::Ledger(error.to_string()))?;

        Ok(CreateContextBundleOutcome { context_bundle_id })
    }

    pub async fn rebuild_context_bundle_projections(
        &self,
    ) -> Result<Vec<ContextBundleRecord>, ContextServiceError> {
        let events = self
            .store
            .list_events()
            .await
            .map_err(|error| ContextServiceError::Ledger(error.to_string()))?;
        SqliteContextBundleProjectionStore::new(self.store.clone())
            .rebuild(&events)
            .await?;
        self.list_context_bundles().await
    }

    pub async fn list_context_bundles(
        &self,
    ) -> Result<Vec<ContextBundleRecord>, ContextServiceError> {
        SqliteContextBundleProjectionStore::new(self.store.clone())
            .list_context_bundles()
            .await
    }

    pub async fn get_context_bundle(
        &self,
        context_bundle_id: ContextBundleId,
    ) -> Result<ContextBundleRecord, ContextServiceError> {
        SqliteContextBundleProjectionStore::new(self.store.clone())
            .get_context_bundle(context_bundle_id)
            .await
    }
}

#[derive(Clone, Debug)]
pub struct SqliteKnowledgeService {
    store: SqliteStore,
}

impl SqliteKnowledgeService {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub async fn create_source_from_artifact(
        &self,
        artifact_id: ArtifactId,
        title: String,
        summary: String,
    ) -> Result<CreateKnowledgeSourceOutcome, KnowledgeServiceError> {
        let title = non_blank_knowledge_input(title)?;
        let summary = non_blank_knowledge_input(summary)?;
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;

        let run_id = sqlite_run_id_for_artifact(&mut tx, artifact_id).await?;
        let outcome = insert_knowledge_source_event(
            &mut tx,
            run_id,
            KnowledgeSourceKind::Artifact,
            Some(artifact_id),
            None,
            title,
            summary,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;

        Ok(outcome)
    }

    pub async fn create_source_from_context_bundle(
        &self,
        context_bundle_id: ContextBundleId,
        title: String,
        summary: String,
    ) -> Result<CreateKnowledgeSourceOutcome, KnowledgeServiceError> {
        let title = non_blank_knowledge_input(title)?;
        let summary = non_blank_knowledge_input(summary)?;
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;

        let run_id = sqlite_run_id_for_context_bundle(&mut tx, context_bundle_id).await?;
        let outcome = insert_knowledge_source_event(
            &mut tx,
            run_id,
            KnowledgeSourceKind::ContextBundle,
            None,
            Some(context_bundle_id),
            title,
            summary,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;

        Ok(outcome)
    }

    pub async fn create_manual_source(
        &self,
        run_id: RunId,
        title: String,
        summary: String,
    ) -> Result<CreateKnowledgeSourceOutcome, KnowledgeServiceError> {
        let title = non_blank_knowledge_input(title)?;
        let summary = non_blank_knowledge_input(summary)?;
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;

        ensure_sqlite_knowledge_run(&mut tx, run_id).await?;
        let outcome = insert_knowledge_source_event(
            &mut tx,
            run_id,
            KnowledgeSourceKind::Manual,
            None,
            None,
            title,
            summary,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;

        Ok(outcome)
    }

    pub async fn rebuild_knowledge_source_projections(
        &self,
    ) -> Result<Vec<KnowledgeSourceRecord>, KnowledgeServiceError> {
        let events = self
            .store
            .list_events()
            .await
            .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;
        SqliteKnowledgeSourceProjectionStore::new(self.store.clone())
            .rebuild(&events)
            .await?;
        self.list_sources().await
    }

    pub async fn list_sources(&self) -> Result<Vec<KnowledgeSourceRecord>, KnowledgeServiceError> {
        SqliteKnowledgeSourceProjectionStore::new(self.store.clone())
            .list_sources()
            .await
    }

    pub async fn get_source(
        &self,
        knowledge_source_id: KnowledgeSourceId,
    ) -> Result<KnowledgeSourceRecord, KnowledgeServiceError> {
        SqliteKnowledgeSourceProjectionStore::new(self.store.clone())
            .get_source(knowledge_source_id)
            .await
    }
}

#[derive(Clone, Debug)]
pub struct SqliteMemoryService {
    store: SqliteStore,
}

impl SqliteMemoryService {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub async fn record_memory(
        &self,
        run_id: RunId,
        knowledge_source_id: Option<KnowledgeSourceId>,
        kind: MemoryKind,
        title: String,
        body: String,
        confidence: f32,
    ) -> Result<RecordMemoryOutcome, MemoryServiceError> {
        let title = non_blank_memory_input(title)?;
        let body = non_blank_memory_input(body)?;
        ensure_valid_memory_confidence(confidence)?;
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| MemoryServiceError::Ledger(error.to_string()))?;
        ensure_sqlite_memory_run(&mut tx, run_id).await?;
        if let Some(source_id) = knowledge_source_id {
            ensure_sqlite_memory_source(&mut tx, source_id).await?;
        }

        let memory_id = MemoryId::new();
        let event = OperationEvent::new_memory_recorded(
            run_id,
            memory_id,
            knowledge_source_id,
            kind,
            title,
            body,
            confidence,
        );
        insert_operation_event(&mut tx, &event)
            .await
            .map_err(|error| MemoryServiceError::Ledger(error.to_string()))?;
        apply_memory_recorded_to_sqlite_projection(&mut tx, &event).await?;
        update_running_run_for_memory_recorded(&mut tx, &event).await?;
        tx.commit()
            .await
            .map_err(|error| MemoryServiceError::Ledger(error.to_string()))?;

        Ok(RecordMemoryOutcome { memory_id })
    }

    pub async fn list_memories(
        &self,
        knowledge_source_id: Option<KnowledgeSourceId>,
    ) -> Result<Vec<MemoryRecord>, MemoryServiceError> {
        SqliteMemoryProjectionStore::new(self.store.clone())
            .list_memories(knowledge_source_id)
            .await
    }

    pub async fn get_memory(
        &self,
        memory_id: MemoryId,
    ) -> Result<MemoryRecord, MemoryServiceError> {
        SqliteMemoryProjectionStore::new(self.store.clone())
            .get_memory(memory_id)
            .await
    }

    pub async fn rebuild_memory_projections(
        &self,
    ) -> Result<Vec<MemoryRecord>, MemoryServiceError> {
        let events = self
            .store
            .list_events()
            .await
            .map_err(|error| MemoryServiceError::Ledger(error.to_string()))?;
        SqliteMemoryProjectionStore::new(self.store.clone())
            .rebuild(&events)
            .await?;
        self.list_memories(None).await
    }
}

#[derive(Clone, Debug)]
pub struct SqliteRetrievalService {
    store: SqliteStore,
}

impl SqliteRetrievalService {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub async fn search(
        &self,
        run_id: RunId,
        query: String,
        limit: usize,
    ) -> Result<SearchResponse, RetrievalServiceError> {
        let query = non_blank_retrieval_input(query)?;
        ensure_valid_retrieval_limit(limit)?;
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| RetrievalServiceError::Ledger(error.to_string()))?;
        ensure_sqlite_retrieval_run(&mut tx, run_id).await?;

        let rows = sqlx::query_as::<_, StoredSearchResult>(
            "select c.document_chunk_id, c.knowledge_source_id, c.run_id, c.sequence, c.text, c.char_start, c.char_end, c.created_at, s.source_kind, s.artifact_id, s.context_bundle_id, s.title, s.summary, s.created_at as source_created_at from document_chunks c join knowledge_sources s on s.knowledge_source_id = c.knowledge_source_id where lower(c.text) like '%' || lower(?1) || '%' order by c.knowledge_source_id asc, c.sequence asc limit ?2",
        )
        .bind(&query)
        .bind(limit as i64)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;

        let mut hits = Vec::with_capacity(rows.len());
        let mut result_chunk_ids = Vec::with_capacity(rows.len());
        for row in rows {
            let mut result = SearchResult::try_from(row)?;
            result.reason = format!("keyword '{query}' matched chunk text");
            result_chunk_ids.push(result.chunk.document_chunk_id);
            hits.push(result);
        }

        let retrieval_trace_id = crate::RetrievalTraceId::new();
        let explanation = format!(
            "M6 deterministic keyword retrieval matched {} chunk(s) for query '{query}'",
            result_chunk_ids.len()
        );
        let event = OperationEvent::new_retrieval_trace_recorded(
            run_id,
            retrieval_trace_id,
            query.clone(),
            result_chunk_ids,
            Vec::new(),
            explanation,
        );
        insert_operation_event(&mut tx, &event)
            .await
            .map_err(|error| RetrievalServiceError::Ledger(error.to_string()))?;
        apply_retrieval_trace_recorded_to_sqlite_projection(&mut tx, &event).await?;
        update_running_run_for_retrieval_trace_recorded(&mut tx, &event).await?;
        tx.commit()
            .await
            .map_err(|error| RetrievalServiceError::Ledger(error.to_string()))?;

        Ok(SearchResponse {
            query,
            hits,
            retrieval_trace_id,
        })
    }

    pub async fn list_retrieval_traces(
        &self,
    ) -> Result<Vec<RetrievalTraceRecord>, RetrievalServiceError> {
        SqliteRetrievalTraceProjectionStore::new(self.store.clone())
            .list_retrieval_traces()
            .await
    }

    pub async fn rebuild_retrieval_trace_projections(
        &self,
    ) -> Result<Vec<RetrievalTraceRecord>, RetrievalServiceError> {
        let events = self
            .store
            .list_events()
            .await
            .map_err(|error| RetrievalServiceError::Ledger(error.to_string()))?;
        SqliteRetrievalTraceProjectionStore::new(self.store.clone())
            .rebuild(&events)
            .await?;
        self.list_retrieval_traces().await
    }
}

#[derive(Clone, Debug)]
pub struct SqliteRetrievalTraceProjectionStore {
    store: SqliteStore,
}

impl SqliteRetrievalTraceProjectionStore {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub async fn rebuild(&self, events: &[OperationEvent]) -> Result<(), RetrievalServiceError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;

        sqlx::query("delete from retrieval_traces")
            .execute(&mut *tx)
            .await
            .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;

        for event in events {
            apply_retrieval_trace_recorded_to_sqlite_projection(&mut tx, event).await?;
        }

        tx.commit()
            .await
            .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl RetrievalTraceProjectionStore for SqliteRetrievalTraceProjectionStore {
    async fn apply(&self, event: &OperationEvent) -> Result<(), RetrievalServiceError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;
        apply_retrieval_trace_recorded_to_sqlite_projection(&mut tx, event).await?;
        tx.commit()
            .await
            .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;
        Ok(())
    }

    async fn list_retrieval_traces(
        &self,
    ) -> Result<Vec<RetrievalTraceRecord>, RetrievalServiceError> {
        let rows = sqlx::query_as::<_, StoredRetrievalTraceRecord>(
            "select retrieval_trace_id, run_id, query, result_chunk_ids_json, result_memory_ids_json, explanation, created_at from retrieval_traces order by created_at desc",
        )
        .fetch_all(self.store.pool())
        .await
        .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;

        rows.into_iter()
            .map(RetrievalTraceRecord::try_from)
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct SqliteWorkerJobProjectionStore {
    store: SqliteStore,
}

impl SqliteWorkerJobProjectionStore {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub async fn rebuild(&self, events: &[OperationEvent]) -> Result<(), WorkerRuntimeError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| WorkerRuntimeError::Projection(error.to_string()))?;

        sqlx::query("delete from worker_jobs")
            .execute(&mut *tx)
            .await
            .map_err(|error| WorkerRuntimeError::Projection(error.to_string()))?;

        for event in events {
            apply_worker_job_event_to_sqlite_projection(&mut tx, event).await?;
        }

        tx.commit()
            .await
            .map_err(|error| WorkerRuntimeError::Projection(error.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl WorkerJobProjectionStore for SqliteWorkerJobProjectionStore {
    async fn apply(&self, event: &OperationEvent) -> Result<(), WorkerRuntimeError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| WorkerRuntimeError::Projection(error.to_string()))?;
        apply_worker_job_event_to_sqlite_projection(&mut tx, event).await?;
        tx.commit()
            .await
            .map_err(|error| WorkerRuntimeError::Projection(error.to_string()))?;
        Ok(())
    }

    async fn list_worker_jobs(&self) -> Result<Vec<WorkerJobRecord>, WorkerRuntimeError> {
        let rows = sqlx::query_as::<_, StoredWorkerJobRecord>(
            "select worker_job_id, worker_id, run_id, kind, status, model, error, started_at, completed_at from worker_jobs order by started_at desc",
        )
        .fetch_all(self.store.pool())
        .await
        .map_err(|error| WorkerRuntimeError::Projection(error.to_string()))?;

        rows.into_iter().map(WorkerJobRecord::try_from).collect()
    }

    async fn rebuild(&self, events: &[OperationEvent]) -> Result<(), WorkerRuntimeError> {
        SqliteWorkerJobProjectionStore::rebuild(self, events).await
    }
}

#[derive(Clone, Debug)]
pub struct SqliteMemoryProjectionStore {
    store: SqliteStore,
}

impl SqliteMemoryProjectionStore {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub async fn rebuild(&self, events: &[OperationEvent]) -> Result<(), MemoryServiceError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| MemoryServiceError::Projection(error.to_string()))?;

        sqlx::query("delete from memory_records")
            .execute(&mut *tx)
            .await
            .map_err(|error| MemoryServiceError::Projection(error.to_string()))?;

        for event in events {
            apply_memory_recorded_to_sqlite_projection(&mut tx, event).await?;
        }

        tx.commit()
            .await
            .map_err(|error| MemoryServiceError::Projection(error.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl MemoryProjectionStore for SqliteMemoryProjectionStore {
    async fn apply(&self, event: &OperationEvent) -> Result<(), MemoryServiceError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| MemoryServiceError::Projection(error.to_string()))?;
        apply_memory_recorded_to_sqlite_projection(&mut tx, event).await?;
        tx.commit()
            .await
            .map_err(|error| MemoryServiceError::Projection(error.to_string()))?;
        Ok(())
    }

    async fn list_memories(
        &self,
        knowledge_source_id: Option<KnowledgeSourceId>,
    ) -> Result<Vec<MemoryRecord>, MemoryServiceError> {
        let rows = match knowledge_source_id {
            Some(source_id) => sqlx::query_as::<_, StoredMemoryRecord>(
                "select memory_id, knowledge_source_id, run_id, kind, title, body, confidence, created_at from memory_records where knowledge_source_id = ?1 order by created_at desc",
            )
            .bind(source_id.to_string())
            .fetch_all(self.store.pool())
            .await,
            None => sqlx::query_as::<_, StoredMemoryRecord>(
                "select memory_id, knowledge_source_id, run_id, kind, title, body, confidence, created_at from memory_records order by created_at desc",
            )
            .fetch_all(self.store.pool())
            .await,
        }
        .map_err(|error| MemoryServiceError::Projection(error.to_string()))?;

        rows.into_iter().map(MemoryRecord::try_from).collect()
    }

    async fn get_memory(&self, memory_id: MemoryId) -> Result<MemoryRecord, MemoryServiceError> {
        let row = sqlx::query_as::<_, StoredMemoryRecord>(
            "select memory_id, knowledge_source_id, run_id, kind, title, body, confidence, created_at from memory_records where memory_id = ?1",
        )
        .bind(memory_id.to_string())
        .fetch_optional(self.store.pool())
        .await
        .map_err(|error| MemoryServiceError::Projection(error.to_string()))?
        .ok_or(MemoryServiceError::UnknownMemory)?;
        MemoryRecord::try_from(row)
    }
}

#[derive(Clone, Debug)]
pub struct SqliteChunkService {
    store: SqliteStore,
    splitter: ChunkTextSplitter,
}

impl SqliteChunkService {
    pub fn new(store: SqliteStore, splitter: ChunkTextSplitter) -> Self {
        Self { store, splitter }
    }

    pub async fn chunk_source(
        &self,
        knowledge_source_id: KnowledgeSourceId,
    ) -> Result<Vec<DocumentChunkRecord>, KnowledgeServiceError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;
        let source = sqlite_knowledge_source_by_id(&mut tx, knowledge_source_id).await?;
        let drafts = self.splitter.split(&source.summary);
        let mut chunks = Vec::with_capacity(drafts.len());

        for draft in drafts {
            let document_chunk_id = crate::DocumentChunkId::new();
            let event = OperationEvent::new_document_chunk_created(
                source.run_id,
                document_chunk_id,
                source.knowledge_source_id,
                draft.sequence,
                draft.text,
                draft.char_start,
                draft.char_end,
            );
            insert_operation_event(&mut tx, &event)
                .await
                .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;
            apply_document_chunk_created_to_sqlite_projection(&mut tx, &event).await?;
            update_running_run_for_document_chunk_created(&mut tx, &event).await?;
            if let OperationEventKind::DocumentChunkCreated {
                document_chunk_id,
                knowledge_source_id,
                sequence,
                text,
                char_start,
                char_end,
            } = event.kind
            {
                chunks.push(DocumentChunkRecord {
                    document_chunk_id,
                    knowledge_source_id,
                    run_id: event.run_id,
                    sequence,
                    text,
                    char_start,
                    char_end,
                    created_at: event.occurred_at,
                });
            }
        }

        tx.commit()
            .await
            .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;

        Ok(chunks)
    }

    pub async fn list_chunks(
        &self,
        knowledge_source_id: Option<KnowledgeSourceId>,
    ) -> Result<Vec<DocumentChunkRecord>, KnowledgeServiceError> {
        SqliteDocumentChunkProjectionStore::new(self.store.clone())
            .list_chunks(knowledge_source_id)
            .await
    }

    pub async fn rebuild_document_chunk_projections(
        &self,
    ) -> Result<Vec<DocumentChunkRecord>, KnowledgeServiceError> {
        let events = self
            .store
            .list_events()
            .await
            .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;
        SqliteDocumentChunkProjectionStore::new(self.store.clone())
            .rebuild(&events)
            .await?;
        self.list_chunks(None).await
    }
}

#[derive(Clone, Debug)]
pub struct SqliteDocumentChunkProjectionStore {
    store: SqliteStore,
}

impl SqliteDocumentChunkProjectionStore {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub async fn rebuild(&self, events: &[OperationEvent]) -> Result<(), KnowledgeServiceError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;

        sqlx::query("delete from document_chunks")
            .execute(&mut *tx)
            .await
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;

        for event in events {
            apply_document_chunk_created_to_sqlite_projection(&mut tx, event).await?;
        }

        tx.commit()
            .await
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl DocumentChunkProjectionStore for SqliteDocumentChunkProjectionStore {
    async fn apply(&self, event: &OperationEvent) -> Result<(), KnowledgeServiceError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;
        apply_document_chunk_created_to_sqlite_projection(&mut tx, event).await?;
        tx.commit()
            .await
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;
        Ok(())
    }

    async fn list_chunks(
        &self,
        knowledge_source_id: Option<KnowledgeSourceId>,
    ) -> Result<Vec<DocumentChunkRecord>, KnowledgeServiceError> {
        let rows = match knowledge_source_id {
            Some(source_id) => sqlx::query_as::<_, StoredDocumentChunkRecord>(
                "select document_chunk_id, knowledge_source_id, run_id, sequence, text, char_start, char_end, created_at from document_chunks where knowledge_source_id = ?1 order by sequence asc",
            )
            .bind(source_id.to_string())
            .fetch_all(self.store.pool())
            .await,
            None => sqlx::query_as::<_, StoredDocumentChunkRecord>(
                "select document_chunk_id, knowledge_source_id, run_id, sequence, text, char_start, char_end, created_at from document_chunks order by knowledge_source_id asc, sequence asc",
            )
            .fetch_all(self.store.pool())
            .await,
        }
        .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;

        rows.into_iter()
            .map(DocumentChunkRecord::try_from)
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct SqliteKnowledgeSourceProjectionStore {
    store: SqliteStore,
}

impl SqliteKnowledgeSourceProjectionStore {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub async fn rebuild(&self, events: &[OperationEvent]) -> Result<(), KnowledgeServiceError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;

        sqlx::query("delete from knowledge_sources")
            .execute(&mut *tx)
            .await
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;

        for event in events {
            apply_knowledge_source_created_to_sqlite_projection(&mut tx, event).await?;
        }

        tx.commit()
            .await
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl KnowledgeSourceProjectionStore for SqliteKnowledgeSourceProjectionStore {
    async fn apply(&self, event: &OperationEvent) -> Result<(), KnowledgeServiceError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;
        apply_knowledge_source_created_to_sqlite_projection(&mut tx, event).await?;
        tx.commit()
            .await
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;
        Ok(())
    }

    async fn list_sources(&self) -> Result<Vec<KnowledgeSourceRecord>, KnowledgeServiceError> {
        let rows = sqlx::query_as::<_, StoredKnowledgeSourceRecord>(
            "select knowledge_source_id, run_id, source_kind, artifact_id, context_bundle_id, title, summary, created_at from knowledge_sources order by created_at desc",
        )
        .fetch_all(self.store.pool())
        .await
        .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;

        rows.into_iter()
            .map(KnowledgeSourceRecord::try_from)
            .collect()
    }

    async fn get_source(
        &self,
        knowledge_source_id: KnowledgeSourceId,
    ) -> Result<KnowledgeSourceRecord, KnowledgeServiceError> {
        let row = sqlx::query_as::<_, StoredKnowledgeSourceRecord>(
            "select knowledge_source_id, run_id, source_kind, artifact_id, context_bundle_id, title, summary, created_at from knowledge_sources where knowledge_source_id = ?1",
        )
        .bind(knowledge_source_id.to_string())
        .fetch_optional(self.store.pool())
        .await
        .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?
        .ok_or(KnowledgeServiceError::UnknownKnowledgeSource)?;
        KnowledgeSourceRecord::try_from(row)
    }
}

async fn ensure_sqlite_memory_run(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: RunId,
) -> Result<(), MemoryServiceError> {
    let run_exists: Option<String> =
        sqlx::query_scalar("select run_id from runs where run_id = ?1")
            .bind(run_id.to_string())
            .fetch_optional(&mut **tx)
            .await
            .map_err(|error| MemoryServiceError::Projection(error.to_string()))?;
    if run_exists.is_some() {
        Ok(())
    } else {
        Err(MemoryServiceError::UnknownRun)
    }
}

async fn ensure_sqlite_memory_source(
    tx: &mut Transaction<'_, Sqlite>,
    knowledge_source_id: KnowledgeSourceId,
) -> Result<(), MemoryServiceError> {
    let source_exists: Option<String> = sqlx::query_scalar(
        "select knowledge_source_id from knowledge_sources where knowledge_source_id = ?1",
    )
    .bind(knowledge_source_id.to_string())
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| MemoryServiceError::Projection(error.to_string()))?;
    if source_exists.is_some() {
        Ok(())
    } else {
        Err(MemoryServiceError::UnknownKnowledgeSource)
    }
}

async fn insert_operation_event(
    tx: &mut Transaction<'_, Sqlite>,
    event: &OperationEvent,
) -> Result<(), LedgerError> {
    let occurred_at = event
        .occurred_at
        .format(&Rfc3339)
        .map_err(|error| LedgerError::InvalidPayload(error.to_string()))?;
    let payload_json = serde_json::to_string(&event.kind)
        .map_err(|error| LedgerError::InvalidPayload(error.to_string()))?;
    sqlx::query(
        "insert into operation_events (event_id, run_id, occurred_at, kind_type, payload_json) values (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(event.id.to_string())
    .bind(event.run_id.to_string())
    .bind(occurred_at)
    .bind(kind_type_for_event(&event.kind))
    .bind(payload_json)
    .execute(&mut **tx)
    .await
    .map_err(|error| LedgerError::Storage(error.to_string()))?;

    Ok(())
}

async fn sqlite_knowledge_source_by_id(
    tx: &mut Transaction<'_, Sqlite>,
    knowledge_source_id: KnowledgeSourceId,
) -> Result<KnowledgeSourceRecord, KnowledgeServiceError> {
    let row = sqlx::query_as::<_, StoredKnowledgeSourceRecord>(
        "select knowledge_source_id, run_id, source_kind, artifact_id, context_bundle_id, title, summary, created_at from knowledge_sources where knowledge_source_id = ?1",
    )
    .bind(knowledge_source_id.to_string())
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?
    .ok_or(KnowledgeServiceError::UnknownKnowledgeSource)?;
    KnowledgeSourceRecord::try_from(row)
}

async fn sqlite_run_id_for_artifact(
    tx: &mut Transaction<'_, Sqlite>,
    artifact_id: ArtifactId,
) -> Result<RunId, KnowledgeServiceError> {
    let run_id: Option<String> =
        sqlx::query_scalar("select run_id from artifacts where artifact_id = ?1")
            .bind(artifact_id.to_string())
            .fetch_optional(&mut **tx)
            .await
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;
    parse_required_knowledge_run_id(run_id, KnowledgeServiceError::UnknownArtifact)
}

async fn sqlite_run_id_for_context_bundle(
    tx: &mut Transaction<'_, Sqlite>,
    context_bundle_id: ContextBundleId,
) -> Result<RunId, KnowledgeServiceError> {
    let run_id: Option<String> =
        sqlx::query_scalar("select run_id from context_bundles where context_bundle_id = ?1")
            .bind(context_bundle_id.to_string())
            .fetch_optional(&mut **tx)
            .await
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;
    parse_required_knowledge_run_id(run_id, KnowledgeServiceError::UnknownContextBundle)
}

async fn ensure_sqlite_knowledge_run(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: RunId,
) -> Result<(), KnowledgeServiceError> {
    let run_exists: Option<String> =
        sqlx::query_scalar("select run_id from runs where run_id = ?1")
            .bind(run_id.to_string())
            .fetch_optional(&mut **tx)
            .await
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;
    if run_exists.is_some() {
        Ok(())
    } else {
        Err(KnowledgeServiceError::UnknownRun)
    }
}

fn parse_required_knowledge_run_id(
    run_id: Option<String>,
    missing: KnowledgeServiceError,
) -> Result<RunId, KnowledgeServiceError> {
    run_id.ok_or(missing).and_then(|run_id| {
        RunId::from_str(&run_id)
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))
    })
}

async fn insert_knowledge_source_event(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: RunId,
    source_kind: KnowledgeSourceKind,
    artifact_id: Option<ArtifactId>,
    context_bundle_id: Option<ContextBundleId>,
    title: String,
    summary: String,
) -> Result<CreateKnowledgeSourceOutcome, KnowledgeServiceError> {
    let knowledge_source_id = KnowledgeSourceId::new();
    let event = OperationEvent::new_knowledge_source_created(
        run_id,
        knowledge_source_id,
        source_kind,
        artifact_id,
        context_bundle_id,
        title,
        summary,
    );
    let occurred_at = event
        .occurred_at
        .format(&Rfc3339)
        .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;
    let payload_json = serde_json::to_string(&event.kind)
        .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;

    sqlx::query(
        "insert into operation_events (event_id, run_id, occurred_at, kind_type, payload_json) values (?1, ?2, ?3, 'knowledge_source_created', ?4)",
    )
    .bind(event.id.to_string())
    .bind(run_id.to_string())
    .bind(&occurred_at)
    .bind(payload_json)
    .execute(&mut **tx)
    .await
    .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;

    apply_knowledge_source_created_to_sqlite_projection(tx, &event).await?;
    update_running_run_for_knowledge_source_created(tx, &event).await?;

    Ok(CreateKnowledgeSourceOutcome {
        knowledge_source_id,
    })
}

async fn apply_knowledge_source_created_to_sqlite_projection(
    tx: &mut Transaction<'_, Sqlite>,
    event: &OperationEvent,
) -> Result<(), KnowledgeServiceError> {
    if let OperationEventKind::KnowledgeSourceCreated {
        knowledge_source_id,
        source_kind,
        artifact_id,
        context_bundle_id,
        title,
        summary,
    } = &event.kind
    {
        let occurred_at = event
            .occurred_at
            .format(&Rfc3339)
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;
        sqlx::query(
            "insert into knowledge_sources (knowledge_source_id, run_id, source_kind, artifact_id, context_bundle_id, title, summary, created_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) on conflict(knowledge_source_id) do update set run_id = excluded.run_id, source_kind = excluded.source_kind, artifact_id = excluded.artifact_id, context_bundle_id = excluded.context_bundle_id, title = excluded.title, summary = excluded.summary, created_at = excluded.created_at",
        )
        .bind(knowledge_source_id.to_string())
        .bind(event.run_id.to_string())
        .bind(knowledge_source_kind_to_storage(*source_kind))
        .bind(artifact_id.map(|id| id.to_string()))
        .bind(context_bundle_id.map(|id| id.to_string()))
        .bind(title)
        .bind(summary)
        .bind(occurred_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;
    }

    Ok(())
}

async fn update_running_run_for_knowledge_source_created(
    tx: &mut Transaction<'_, Sqlite>,
    event: &OperationEvent,
) -> Result<(), KnowledgeServiceError> {
    if matches!(
        event.kind,
        OperationEventKind::KnowledgeSourceCreated { .. }
    ) {
        update_running_run_for_knowledge_event(tx, event).await?;
    }

    Ok(())
}

async fn apply_document_chunk_created_to_sqlite_projection(
    tx: &mut Transaction<'_, Sqlite>,
    event: &OperationEvent,
) -> Result<(), KnowledgeServiceError> {
    if let OperationEventKind::DocumentChunkCreated {
        document_chunk_id,
        knowledge_source_id,
        sequence,
        text,
        char_start,
        char_end,
    } = &event.kind
    {
        let occurred_at = event
            .occurred_at
            .format(&Rfc3339)
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;
        sqlx::query(
            "insert into document_chunks (document_chunk_id, knowledge_source_id, run_id, sequence, text, char_start, char_end, created_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) on conflict(document_chunk_id) do update set knowledge_source_id = excluded.knowledge_source_id, run_id = excluded.run_id, sequence = excluded.sequence, text = excluded.text, char_start = excluded.char_start, char_end = excluded.char_end, created_at = excluded.created_at",
        )
        .bind(document_chunk_id.to_string())
        .bind(knowledge_source_id.to_string())
        .bind(event.run_id.to_string())
        .bind(*sequence)
        .bind(text)
        .bind(*char_start)
        .bind(*char_end)
        .bind(occurred_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;
    }

    Ok(())
}

async fn update_running_run_for_document_chunk_created(
    tx: &mut Transaction<'_, Sqlite>,
    event: &OperationEvent,
) -> Result<(), KnowledgeServiceError> {
    if matches!(event.kind, OperationEventKind::DocumentChunkCreated { .. }) {
        update_running_run_for_knowledge_event(tx, event).await?;
    }

    Ok(())
}

async fn apply_memory_recorded_to_sqlite_projection(
    tx: &mut Transaction<'_, Sqlite>,
    event: &OperationEvent,
) -> Result<(), MemoryServiceError> {
    if let OperationEventKind::MemoryRecorded {
        memory_id,
        knowledge_source_id,
        kind,
        title,
        body,
        confidence,
    } = &event.kind
    {
        let occurred_at = event
            .occurred_at
            .format(&Rfc3339)
            .map_err(|error| MemoryServiceError::Projection(error.to_string()))?;
        sqlx::query(
            "insert into memory_records (memory_id, knowledge_source_id, run_id, kind, title, body, confidence, created_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) on conflict(memory_id) do update set knowledge_source_id = excluded.knowledge_source_id, run_id = excluded.run_id, kind = excluded.kind, title = excluded.title, body = excluded.body, confidence = excluded.confidence, created_at = excluded.created_at",
        )
        .bind(memory_id.to_string())
        .bind(knowledge_source_id.map(|id| id.to_string()))
        .bind(event.run_id.to_string())
        .bind(memory_kind_to_storage(*kind))
        .bind(title)
        .bind(body)
        .bind(*confidence)
        .bind(occurred_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| MemoryServiceError::Projection(error.to_string()))?;
    }

    Ok(())
}

async fn ensure_sqlite_retrieval_run(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: RunId,
) -> Result<(), RetrievalServiceError> {
    let run_exists: Option<String> =
        sqlx::query_scalar("select run_id from runs where run_id = ?1")
            .bind(run_id.to_string())
            .fetch_optional(&mut **tx)
            .await
            .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;
    if run_exists.is_some() {
        Ok(())
    } else {
        Err(RetrievalServiceError::UnknownRun)
    }
}

async fn apply_retrieval_trace_recorded_to_sqlite_projection(
    tx: &mut Transaction<'_, Sqlite>,
    event: &OperationEvent,
) -> Result<(), RetrievalServiceError> {
    if let OperationEventKind::RetrievalTraceRecorded {
        retrieval_trace_id,
        query,
        result_chunk_ids,
        result_memory_ids,
        explanation,
    } = &event.kind
    {
        let occurred_at = event
            .occurred_at
            .format(&Rfc3339)
            .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;
        let result_chunk_ids_json = ids_to_json(result_chunk_ids)
            .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;
        let result_memory_ids_json = ids_to_json(result_memory_ids)
            .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;
        sqlx::query(
            "insert into retrieval_traces (retrieval_trace_id, run_id, query, result_chunk_ids_json, result_memory_ids_json, explanation, created_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7) on conflict(retrieval_trace_id) do update set run_id = excluded.run_id, query = excluded.query, result_chunk_ids_json = excluded.result_chunk_ids_json, result_memory_ids_json = excluded.result_memory_ids_json, explanation = excluded.explanation, created_at = excluded.created_at",
        )
        .bind(retrieval_trace_id.to_string())
        .bind(event.run_id.to_string())
        .bind(query)
        .bind(result_chunk_ids_json)
        .bind(result_memory_ids_json)
        .bind(explanation)
        .bind(occurred_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;
    }

    Ok(())
}

async fn update_running_run_for_retrieval_trace_recorded(
    tx: &mut Transaction<'_, Sqlite>,
    event: &OperationEvent,
) -> Result<(), RetrievalServiceError> {
    if matches!(
        event.kind,
        OperationEventKind::RetrievalTraceRecorded { .. }
    ) {
        let occurred_at = event
            .occurred_at
            .format(&Rfc3339)
            .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;
        sqlx::query("update runs set updated_at = ?2 where run_id = ?1 and status = 'running'")
            .bind(event.run_id.to_string())
            .bind(occurred_at)
            .execute(&mut **tx)
            .await
            .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;
    }

    Ok(())
}

async fn update_running_run_for_memory_recorded(
    tx: &mut Transaction<'_, Sqlite>,
    event: &OperationEvent,
) -> Result<(), MemoryServiceError> {
    if matches!(event.kind, OperationEventKind::MemoryRecorded { .. }) {
        let occurred_at = event
            .occurred_at
            .format(&Rfc3339)
            .map_err(|error| MemoryServiceError::Projection(error.to_string()))?;
        sqlx::query("update runs set updated_at = ?2 where run_id = ?1 and status = 'running'")
            .bind(event.run_id.to_string())
            .bind(occurred_at)
            .execute(&mut **tx)
            .await
            .map_err(|error| MemoryServiceError::Projection(error.to_string()))?;
    }

    Ok(())
}

async fn update_running_run_for_knowledge_event(
    tx: &mut Transaction<'_, Sqlite>,
    event: &OperationEvent,
) -> Result<(), KnowledgeServiceError> {
    let occurred_at = event
        .occurred_at
        .format(&Rfc3339)
        .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;
    sqlx::query("update runs set updated_at = ?2 where run_id = ?1 and status = 'running'")
        .bind(event.run_id.to_string())
        .bind(occurred_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct SqliteContextBundleProjectionStore {
    store: SqliteStore,
}

impl SqliteContextBundleProjectionStore {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub async fn rebuild(&self, events: &[OperationEvent]) -> Result<(), ContextServiceError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| ContextServiceError::Projection(error.to_string()))?;

        sqlx::query("delete from context_bundles")
            .execute(&mut *tx)
            .await
            .map_err(|error| ContextServiceError::Projection(error.to_string()))?;

        for event in events {
            apply_context_bundle_created_to_sqlite_projection(&mut tx, event).await?;
        }

        tx.commit()
            .await
            .map_err(|error| ContextServiceError::Projection(error.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl ContextBundleProjectionStore for SqliteContextBundleProjectionStore {
    async fn apply(&self, event: &OperationEvent) -> Result<(), ContextServiceError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| ContextServiceError::Projection(error.to_string()))?;
        apply_context_bundle_created_to_sqlite_projection(&mut tx, event).await?;
        tx.commit()
            .await
            .map_err(|error| ContextServiceError::Projection(error.to_string()))?;
        Ok(())
    }

    async fn list_context_bundles(&self) -> Result<Vec<ContextBundleRecord>, ContextServiceError> {
        let rows = sqlx::query_as::<_, StoredContextBundleRecord>(
            "select context_bundle_id, run_id, title, summary, artifact_ids, event_ids, created_at from context_bundles order by created_at desc",
        )
        .fetch_all(self.store.pool())
        .await
        .map_err(|error| ContextServiceError::Projection(error.to_string()))?;

        rows.into_iter()
            .map(ContextBundleRecord::try_from)
            .collect()
    }

    async fn get_context_bundle(
        &self,
        context_bundle_id: ContextBundleId,
    ) -> Result<ContextBundleRecord, ContextServiceError> {
        let row = sqlx::query_as::<_, StoredContextBundleRecord>(
            "select context_bundle_id, run_id, title, summary, artifact_ids, event_ids, created_at from context_bundles where context_bundle_id = ?1",
        )
        .bind(context_bundle_id.to_string())
        .fetch_optional(self.store.pool())
        .await
        .map_err(|error| ContextServiceError::Projection(error.to_string()))?
        .ok_or(ContextServiceError::UnknownContextBundle)?;
        ContextBundleRecord::try_from(row)
    }
}

async fn ensure_sqlite_context_references(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: RunId,
    artifact_ids: &[ArtifactId],
    event_ids: &[EventId],
) -> Result<(), ContextServiceError> {
    let run_exists: Option<String> =
        sqlx::query_scalar("select run_id from runs where run_id = ?1")
            .bind(run_id.to_string())
            .fetch_optional(&mut **tx)
            .await
            .map_err(|error| ContextServiceError::Projection(error.to_string()))?;
    if run_exists.is_none() {
        return Err(ContextServiceError::UnknownRun);
    }

    for artifact_id in artifact_ids {
        let artifact_exists: Option<String> =
            sqlx::query_scalar("select artifact_id from artifacts where artifact_id = ?1")
                .bind(artifact_id.to_string())
                .fetch_optional(&mut **tx)
                .await
                .map_err(|error| ContextServiceError::Projection(error.to_string()))?;
        if artifact_exists.is_none() {
            return Err(ContextServiceError::UnknownArtifact);
        }
    }

    for event_id in event_ids {
        let event_exists: Option<String> =
            sqlx::query_scalar("select event_id from operation_events where event_id = ?1")
                .bind(event_id.to_string())
                .fetch_optional(&mut **tx)
                .await
                .map_err(|error| ContextServiceError::Projection(error.to_string()))?;
        if event_exists.is_none() {
            return Err(ContextServiceError::UnknownEvent);
        }
    }

    Ok(())
}

async fn apply_context_bundle_created_to_sqlite_projection(
    tx: &mut Transaction<'_, Sqlite>,
    event: &OperationEvent,
) -> Result<(), ContextServiceError> {
    if let OperationEventKind::ContextBundleCreated {
        context_bundle_id,
        title,
        summary,
        artifact_ids,
        event_ids,
    } = &event.kind
    {
        let occurred_at = event
            .occurred_at
            .format(&Rfc3339)
            .map_err(|error| ContextServiceError::Projection(error.to_string()))?;
        let artifact_ids_json = ids_to_json(artifact_ids)
            .map_err(|error| ContextServiceError::Projection(error.to_string()))?;
        let event_ids_json = ids_to_json(event_ids)
            .map_err(|error| ContextServiceError::Projection(error.to_string()))?;
        sqlx::query(
            "insert into context_bundles (context_bundle_id, run_id, title, summary, artifact_ids, event_ids, created_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7) on conflict(context_bundle_id) do update set run_id = excluded.run_id, title = excluded.title, summary = excluded.summary, artifact_ids = excluded.artifact_ids, event_ids = excluded.event_ids, created_at = excluded.created_at",
        )
        .bind(context_bundle_id.to_string())
        .bind(event.run_id.to_string())
        .bind(title)
        .bind(summary)
        .bind(artifact_ids_json)
        .bind(event_ids_json)
        .bind(occurred_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| ContextServiceError::Projection(error.to_string()))?;
    }

    Ok(())
}

async fn update_running_run_for_context_bundle_created(
    tx: &mut Transaction<'_, Sqlite>,
    event: &OperationEvent,
) -> Result<(), ContextServiceError> {
    if matches!(event.kind, OperationEventKind::ContextBundleCreated { .. }) {
        let occurred_at = event
            .occurred_at
            .format(&Rfc3339)
            .map_err(|error| ContextServiceError::Projection(error.to_string()))?;
        sqlx::query("update runs set updated_at = ?2 where run_id = ?1 and status = 'running'")
            .bind(event.run_id.to_string())
            .bind(occurred_at)
            .execute(&mut **tx)
            .await
            .map_err(|error| ContextServiceError::Projection(error.to_string()))?;
    }

    Ok(())
}

#[derive(Clone, Debug)]
pub struct SqliteArtifactProjectionStore {
    store: SqliteStore,
}

impl SqliteArtifactProjectionStore {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub async fn rebuild(&self, events: &[OperationEvent]) -> Result<(), ArtifactServiceError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?;

        sqlx::query("delete from artifacts")
            .execute(&mut *tx)
            .await
            .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?;

        for event in events {
            apply_artifact_created_to_sqlite_projection(&mut tx, event).await?;
        }

        tx.commit()
            .await
            .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl ArtifactProjectionStore for SqliteArtifactProjectionStore {
    async fn apply(&self, event: &OperationEvent) -> Result<(), ArtifactServiceError> {
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?;
        apply_artifact_created_to_sqlite_projection(&mut tx, event).await?;
        tx.commit()
            .await
            .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?;
        Ok(())
    }

    async fn list_artifacts(&self) -> Result<Vec<ArtifactRecord>, ArtifactServiceError> {
        let rows = sqlx::query_as::<_, StoredArtifactRecord>(
            "select artifact_id, run_id, title, kind, mime_type, sha256, byte_len, created_at from artifacts order by created_at desc",
        )
        .fetch_all(self.store.pool())
        .await
        .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?;

        rows.into_iter().map(ArtifactRecord::try_from).collect()
    }

    async fn get_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> Result<ArtifactRecord, ArtifactServiceError> {
        let row = sqlx::query_as::<_, StoredArtifactRecord>(
            "select artifact_id, run_id, title, kind, mime_type, sha256, byte_len, created_at from artifacts where artifact_id = ?1",
        )
        .bind(artifact_id.to_string())
        .fetch_optional(self.store.pool())
        .await
        .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?
        .ok_or(ArtifactServiceError::UnknownArtifact)?;
        ArtifactRecord::try_from(row)
    }
}

async fn apply_artifact_created_to_sqlite_projection(
    tx: &mut Transaction<'_, Sqlite>,
    event: &OperationEvent,
) -> Result<(), ArtifactServiceError> {
    if let OperationEventKind::ArtifactCreated {
        artifact_id,
        title,
        kind,
        mime_type,
        sha256,
        byte_len,
    } = &event.kind
    {
        let occurred_at = event
            .occurred_at
            .format(&Rfc3339)
            .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?;
        sqlx::query(
            "insert into artifacts (artifact_id, run_id, title, kind, mime_type, sha256, byte_len, created_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) on conflict(artifact_id) do update set run_id = excluded.run_id, title = excluded.title, kind = excluded.kind, mime_type = excluded.mime_type, sha256 = excluded.sha256, byte_len = excluded.byte_len, created_at = excluded.created_at",
        )
        .bind(artifact_id.to_string())
        .bind(event.run_id.to_string())
        .bind(title)
        .bind(artifact_kind_to_storage(*kind))
        .bind(mime_type)
        .bind(sha256)
        .bind(*byte_len as i64)
        .bind(occurred_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?;
    }

    Ok(())
}

async fn apply_worker_job_event_to_sqlite_projection(
    tx: &mut Transaction<'_, Sqlite>,
    event: &OperationEvent,
) -> Result<(), WorkerRuntimeError> {
    let occurred_at = event
        .occurred_at
        .format(&Rfc3339)
        .map_err(|error| WorkerRuntimeError::InvalidPayload(error.to_string()))?;

    match &event.kind {
        OperationEventKind::WorkerJobStarted {
            worker_job_id,
            worker_id,
            kind,
            model,
        } => {
            sqlx::query(
                "insert into worker_jobs (worker_job_id, worker_id, run_id, kind, status, model, error, started_at, completed_at) values (?1, ?2, ?3, ?4, 'started', ?5, null, ?6, null) on conflict(worker_job_id) do update set worker_id = excluded.worker_id, run_id = excluded.run_id, kind = excluded.kind, status = excluded.status, model = excluded.model, error = excluded.error, started_at = excluded.started_at, completed_at = excluded.completed_at",
            )
            .bind(worker_job_id.to_string())
            .bind(worker_id.to_string())
            .bind(event.run_id.to_string())
            .bind(worker_job_kind_to_storage(*kind))
            .bind(model)
            .bind(&occurred_at)
            .execute(&mut **tx)
            .await
            .map_err(|error| WorkerRuntimeError::Projection(error.to_string()))?;
        }
        OperationEventKind::WorkerJobCompleted {
            worker_job_id,
            worker_id,
            kind,
        } => {
            sqlx::query(
                "insert into worker_jobs (worker_job_id, worker_id, run_id, kind, status, model, error, started_at, completed_at) values (?1, ?2, ?3, ?4, 'completed', null, null, ?5, ?5) on conflict(worker_job_id) do update set worker_id = excluded.worker_id, run_id = excluded.run_id, kind = excluded.kind, status = excluded.status, error = excluded.error, completed_at = excluded.completed_at",
            )
            .bind(worker_job_id.to_string())
            .bind(worker_id.to_string())
            .bind(event.run_id.to_string())
            .bind(worker_job_kind_to_storage(*kind))
            .bind(&occurred_at)
            .execute(&mut **tx)
            .await
            .map_err(|error| WorkerRuntimeError::Projection(error.to_string()))?;
        }
        OperationEventKind::WorkerJobFailed {
            worker_job_id,
            worker_id,
            kind,
            reason,
        } => {
            sqlx::query(
                "insert into worker_jobs (worker_job_id, worker_id, run_id, kind, status, model, error, started_at, completed_at) values (?1, ?2, ?3, ?4, 'failed', null, ?5, ?6, ?6) on conflict(worker_job_id) do update set worker_id = excluded.worker_id, run_id = excluded.run_id, kind = excluded.kind, status = excluded.status, error = excluded.error, completed_at = excluded.completed_at",
            )
            .bind(worker_job_id.to_string())
            .bind(worker_id.to_string())
            .bind(event.run_id.to_string())
            .bind(worker_job_kind_to_storage(*kind))
            .bind(reason)
            .bind(&occurred_at)
            .execute(&mut **tx)
            .await
            .map_err(|error| WorkerRuntimeError::Projection(error.to_string()))?;
        }
        _ => {}
    }

    Ok(())
}

async fn update_running_run_for_artifact_created(
    tx: &mut Transaction<'_, Sqlite>,
    event: &OperationEvent,
) -> Result<(), ArtifactServiceError> {
    if matches!(event.kind, OperationEventKind::ArtifactCreated { .. }) {
        let occurred_at = event
            .occurred_at
            .format(&Rfc3339)
            .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?;
        sqlx::query("update runs set updated_at = ?2 where run_id = ?1 and status = 'running'")
            .bind(event.run_id.to_string())
            .bind(occurred_at)
            .execute(&mut **tx)
            .await
            .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?;
    }

    Ok(())
}

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

    async fn list_with_filter(
        &self,
        run_id: Option<RunId>,
    ) -> Result<Vec<OperationEvent>, LedgerError> {
        let rows = match run_id {
            Some(run_id) => sqlx::query_as::<_, StoredOperationEvent>(
                "select event_id, run_id, occurred_at, payload_json from operation_events where run_id = ?1 order by sequence asc",
            )
            .bind(run_id.to_string())
            .fetch_all(self.store.pool())
            .await,
            None => sqlx::query_as::<_, StoredOperationEvent>(
                "select event_id, run_id, occurred_at, payload_json from operation_events order by sequence asc",
            )
            .fetch_all(self.store.pool())
            .await,
        }
        .map_err(|error| LedgerError::Storage(error.to_string()))?;

        rows.into_iter().map(OperationEvent::try_from).collect()
    }
}

#[async_trait]
impl OperationLedger for SqliteOperationLedger {
    async fn append(&self, event: OperationEvent) -> Result<(), LedgerError> {
        let kind_type = kind_type_for_event(&event.kind);
        let payload_json = serde_json::to_string(&event.kind)
            .map_err(|error| LedgerError::InvalidPayload(error.to_string()))?;
        let occurred_at = event
            .occurred_at
            .format(&Rfc3339)
            .map_err(|error| LedgerError::InvalidPayload(error.to_string()))?;

        sqlx::query(
            "insert into operation_events (event_id, run_id, occurred_at, kind_type, payload_json) values (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(event.id.to_string())
        .bind(event.run_id.to_string())
        .bind(occurred_at)
        .bind(kind_type)
        .bind(payload_json)
        .execute(self.store.pool())
        .await
        .map_err(|error| LedgerError::Storage(error.to_string()))?;

        Ok(())
    }

    async fn list(&self) -> Result<Vec<OperationEvent>, LedgerError> {
        self.list_with_filter(None).await
    }

    async fn list_for_run(&self, run_id: RunId) -> Result<Vec<OperationEvent>, LedgerError> {
        self.list_with_filter(Some(run_id)).await
    }
}

#[derive(Clone, Debug)]
pub struct SqliteRunProjectionStore {
    store: SqliteStore,
}

impl SqliteRunProjectionStore {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    pub fn shared(self) -> SharedRunProjectionStore {
        Arc::new(self)
    }
}

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

    async fn append_tool_events_transactionally(
        &self,
        events: Vec<OperationEvent>,
    ) -> Result<(), RunServiceError> {
        let run_id = events
            .first()
            .map(|event| event.run_id)
            .ok_or(RunServiceError::InvalidToolInput)?;
        let updated_at = events
            .last()
            .ok_or(RunServiceError::InvalidToolInput)?
            .occurred_at
            .format(&Rfc3339)
            .map_err(|error| RunServiceError::Ledger(error.to_string()))?;

        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| RunServiceError::Ledger(error.to_string()))?;

        let current_status: Option<String> =
            sqlx::query_scalar("select status from runs where run_id = ?1")
                .bind(run_id.to_string())
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| RunServiceError::Projection(error.to_string()))?;
        match current_status.as_deref() {
            None => return Err(RunServiceError::InvalidRunId),
            Some("running") => {}
            Some(_) => return Err(RunServiceError::RunAlreadyTerminal),
        }

        for event in events {
            let occurred_at = event
                .occurred_at
                .format(&Rfc3339)
                .map_err(|error| RunServiceError::Ledger(error.to_string()))?;
            let kind_type = kind_type_for_event(&event.kind);
            let payload_json = serde_json::to_string(&event.kind)
                .map_err(|error| RunServiceError::Ledger(error.to_string()))?;
            sqlx::query(
                "insert into operation_events (event_id, run_id, occurred_at, kind_type, payload_json) values (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(event.id.to_string())
            .bind(run_id.to_string())
            .bind(occurred_at)
            .bind(kind_type)
            .bind(payload_json)
            .execute(&mut *tx)
            .await
            .map_err(|error| RunServiceError::Ledger(error.to_string()))?;
        }

        sqlx::query("update runs set updated_at = ?2 where run_id = ?1 and status = 'running'")
            .bind(run_id.to_string())
            .bind(updated_at)
            .execute(&mut *tx)
            .await
            .map_err(|error| RunServiceError::Projection(error.to_string()))?;

        tx.commit()
            .await
            .map_err(|error| RunServiceError::Ledger(error.to_string()))?;

        Ok(())
    }

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

        let current_status: Option<String> =
            sqlx::query_scalar("select status from runs where run_id = ?1")
                .bind(run_id.to_string())
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| RunServiceError::Projection(error.to_string()))?;
        match current_status.as_deref() {
            None => return Err(RunServiceError::InvalidRunId),
            Some("running") => {}
            Some(_) => return Err(RunServiceError::RunAlreadyTerminal),
        }

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
            | OperationEventKind::RunAssistantMessage { .. }
            | OperationEventKind::ToolCallRequested { .. }
            | OperationEventKind::ToolCallApproved { .. }
            | OperationEventKind::ToolCallDenied { .. }
            | OperationEventKind::ToolCallCompleted { .. }
            | OperationEventKind::ToolCallFailed { .. }
            | OperationEventKind::ArtifactCreated { .. }
            | OperationEventKind::ContextBundleCreated { .. }
            | OperationEventKind::KnowledgeSourceCreated { .. }
            | OperationEventKind::DocumentChunkCreated { .. }
            | OperationEventKind::MemoryRecorded { .. }
            | OperationEventKind::RetrievalTraceRecorded { .. }
            | OperationEventKind::WorkerJobStarted { .. }
            | OperationEventKind::WorkerJobCompleted { .. }
            | OperationEventKind::WorkerJobFailed { .. } => {
                sqlx::query(
                    "update runs set updated_at = ?2 where run_id = ?1 and status = 'running'",
                )
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

    fn non_blank_lifecycle_input(value: String) -> Result<String, RunServiceError> {
        let value = value.trim().to_string();
        if value.is_empty() {
            return Err(RunServiceError::InvalidLifecycleInput);
        }
        Ok(value)
    }

    fn non_blank_tool_text(value: String) -> Result<String, RunServiceError> {
        let value = value.trim().to_string();
        if value.is_empty() {
            return Err(RunServiceError::InvalidToolInput);
        }
        Ok(value)
    }
}

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

    async fn request_model(
        &self,
        run_id: RunId,
        prompt: String,
    ) -> Result<RunLifecycleOutcome, RunServiceError> {
        let prompt = Self::non_blank_lifecycle_input(prompt)?;
        self.append_lifecycle_event_transactionally(OperationEvent::new_run_model_requested(
            run_id, prompt,
        ))
        .await
    }

    async fn record_assistant_message(
        &self,
        run_id: RunId,
        content: String,
    ) -> Result<RunLifecycleOutcome, RunServiceError> {
        let content = Self::non_blank_lifecycle_input(content)?;
        self.append_lifecycle_event_transactionally(OperationEvent::new_run_assistant_message(
            run_id, content,
        ))
        .await
    }

    async fn complete_run(
        &self,
        run_id: RunId,
        summary: String,
    ) -> Result<RunLifecycleOutcome, RunServiceError> {
        let summary = Self::non_blank_lifecycle_input(summary)?;
        self.append_lifecycle_event_transactionally(OperationEvent::new_run_completed(
            run_id, summary,
        ))
        .await
    }

    async fn fail_run(
        &self,
        run_id: RunId,
        reason: String,
    ) -> Result<RunLifecycleOutcome, RunServiceError> {
        let reason = Self::non_blank_lifecycle_input(reason)?;
        self.append_lifecycle_event_transactionally(OperationEvent::new_run_failed(run_id, reason))
            .await
    }

    async fn request_tool_call(
        &self,
        run_id: RunId,
        tool_name: String,
        effect: ToolEffect,
        input_json: Value,
    ) -> Result<ToolCallOutcome, RunServiceError> {
        let tool_name = Self::non_blank_tool_text(tool_name)?;
        let tool_call_id = ToolCallId::new();
        let mut events = vec![OperationEvent::new_tool_call_requested(
            run_id,
            tool_call_id,
            tool_name,
            effect.clone(),
            input_json,
        )];
        let approval = ToolApprovalPolicy::default().decision_for(&effect);
        if approval == ToolApprovalDecision::AutoApproved {
            events.push(OperationEvent::new_tool_call_approved(
                run_id,
                tool_call_id,
                "mneme-policy",
            ));
        }
        self.append_tool_events_transactionally(events).await?;
        Ok(ToolCallOutcome {
            run_id,
            tool_call_id,
            approval,
        })
    }

    async fn approve_tool_call(
        &self,
        run_id: RunId,
        tool_call_id: ToolCallId,
        approved_by: String,
    ) -> Result<ToolLifecycleOutcome, RunServiceError> {
        let approved_by = Self::non_blank_tool_text(approved_by)?;
        self.append_tool_events_transactionally(vec![OperationEvent::new_tool_call_approved(
            run_id,
            tool_call_id,
            approved_by,
        )])
        .await?;
        Ok(ToolLifecycleOutcome {
            run_id,
            tool_call_id,
        })
    }

    async fn deny_tool_call(
        &self,
        run_id: RunId,
        tool_call_id: ToolCallId,
        reason: String,
    ) -> Result<ToolLifecycleOutcome, RunServiceError> {
        let reason = Self::non_blank_tool_text(reason)?;
        self.append_tool_events_transactionally(vec![OperationEvent::new_tool_call_denied(
            run_id,
            tool_call_id,
            reason,
        )])
        .await?;
        Ok(ToolLifecycleOutcome {
            run_id,
            tool_call_id,
        })
    }

    async fn record_tool_result(
        &self,
        run_id: RunId,
        tool_call_id: ToolCallId,
        output_json: Value,
    ) -> Result<ToolLifecycleOutcome, RunServiceError> {
        self.append_tool_events_transactionally(vec![OperationEvent::new_tool_call_completed(
            run_id,
            tool_call_id,
            output_json,
        )])
        .await?;
        Ok(ToolLifecycleOutcome {
            run_id,
            tool_call_id,
        })
    }

    async fn record_tool_failure(
        &self,
        run_id: RunId,
        tool_call_id: ToolCallId,
        reason: String,
    ) -> Result<ToolLifecycleOutcome, RunServiceError> {
        let reason = Self::non_blank_tool_text(reason)?;
        self.append_tool_events_transactionally(vec![OperationEvent::new_tool_call_failed(
            run_id,
            tool_call_id,
            reason,
        )])
        .await?;
        Ok(ToolLifecycleOutcome {
            run_id,
            tool_call_id,
        })
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

#[async_trait]
impl RunProjectionStore for SqliteRunProjectionStore {
    async fn apply(&self, event: &OperationEvent) -> Result<(), RunProjectionError> {
        let occurred_at = format_datetime(event.occurred_at)?;

        match &event.kind {
            OperationEventKind::RunStarted { instruction } => {
                sqlx::query(
                    "insert into runs (run_id, status, instruction, started_at, completed_at, failed_at, summary, error, updated_at) values (?1, 'running', ?2, ?3, null, null, null, null, ?3) on conflict(run_id) do update set status = excluded.status, instruction = excluded.instruction, started_at = excluded.started_at, completed_at = excluded.completed_at, failed_at = excluded.failed_at, summary = excluded.summary, error = excluded.error, updated_at = excluded.updated_at",
                )
                .bind(event.run_id.to_string())
                .bind(instruction)
                .bind(occurred_at)
                .execute(self.store.pool())
                .await
                .map_err(|error| RunProjectionError::Storage(error.to_string()))?;
            }
            OperationEventKind::RunModelRequested { .. }
            | OperationEventKind::RunAssistantMessage { .. }
            | OperationEventKind::ToolCallRequested { .. }
            | OperationEventKind::ToolCallApproved { .. }
            | OperationEventKind::ToolCallDenied { .. }
            | OperationEventKind::ToolCallCompleted { .. }
            | OperationEventKind::ToolCallFailed { .. }
            | OperationEventKind::ArtifactCreated { .. }
            | OperationEventKind::ContextBundleCreated { .. }
            | OperationEventKind::KnowledgeSourceCreated { .. }
            | OperationEventKind::DocumentChunkCreated { .. }
            | OperationEventKind::MemoryRecorded { .. }
            | OperationEventKind::RetrievalTraceRecorded { .. }
            | OperationEventKind::WorkerJobStarted { .. }
            | OperationEventKind::WorkerJobCompleted { .. }
            | OperationEventKind::WorkerJobFailed { .. } => {
                sqlx::query(
                    "update runs set updated_at = ?2 where run_id = ?1 and status = 'running'",
                )
                .bind(event.run_id.to_string())
                .bind(occurred_at)
                .execute(self.store.pool())
                .await
                .map_err(|error| RunProjectionError::Storage(error.to_string()))?;
            }
            OperationEventKind::RunCompleted { summary } => {
                sqlx::query(
                    "update runs set status = 'completed', completed_at = ?2, summary = ?3, updated_at = ?2 where run_id = ?1",
                )
                .bind(event.run_id.to_string())
                .bind(occurred_at)
                .bind(summary)
                .execute(self.store.pool())
                .await
                .map_err(|error| RunProjectionError::Storage(error.to_string()))?;
            }
            OperationEventKind::RunFailed { reason } => {
                sqlx::query(
                    "update runs set status = 'failed', failed_at = ?2, error = ?3, updated_at = ?2 where run_id = ?1",
                )
                .bind(event.run_id.to_string())
                .bind(occurred_at)
                .bind(reason)
                .execute(self.store.pool())
                .await
                .map_err(|error| RunProjectionError::Storage(error.to_string()))?;
            }
        }

        Ok(())
    }

    async fn rebuild(&self, events: &[OperationEvent]) -> Result<(), RunProjectionError> {
        let projections = RunProjector::replay(events);
        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|error| RunProjectionError::Storage(error.to_string()))?;

        sqlx::query("delete from runs")
            .execute(&mut *tx)
            .await
            .map_err(|error| RunProjectionError::Storage(error.to_string()))?;

        for run in projections {
            sqlx::query(
                "insert into runs (run_id, status, instruction, started_at, completed_at, failed_at, summary, error, updated_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .bind(run.run_id.to_string())
            .bind(status_to_storage(&run.status))
            .bind(run.instruction)
            .bind(format_datetime(run.started_at)?)
            .bind(format_optional_datetime(run.completed_at)?)
            .bind(format_optional_datetime(run.failed_at)?)
            .bind(run.summary)
            .bind(run.error)
            .bind(format_datetime(run.updated_at)?)
            .execute(&mut *tx)
            .await
            .map_err(|error| RunProjectionError::Storage(error.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|error| RunProjectionError::Storage(error.to_string()))?;

        Ok(())
    }

    async fn list_runs(&self) -> Result<Vec<RunProjection>, RunProjectionError> {
        let rows = sqlx::query_as::<_, StoredRunProjection>(
            "select run_id, status, instruction, started_at, completed_at, failed_at, summary, error, updated_at from runs order by updated_at desc",
        )
        .fetch_all(self.store.pool())
        .await
        .map_err(|error| RunProjectionError::Storage(error.to_string()))?;

        rows.into_iter().map(RunProjection::try_from).collect()
    }
}

#[derive(sqlx::FromRow)]
struct StoredOperationEvent {
    event_id: String,
    run_id: String,
    occurred_at: String,
    payload_json: String,
}

impl TryFrom<StoredOperationEvent> for OperationEvent {
    type Error = LedgerError;

    fn try_from(row: StoredOperationEvent) -> Result<Self, Self::Error> {
        Ok(Self {
            id: EventId::from_str(&row.event_id)
                .map_err(|error| LedgerError::InvalidPayload(error.to_string()))?,
            run_id: RunId::from_str(&row.run_id)
                .map_err(|error| LedgerError::InvalidPayload(error.to_string()))?,
            occurred_at: OffsetDateTime::parse(&row.occurred_at, &Rfc3339)
                .map_err(|error| LedgerError::InvalidPayload(error.to_string()))?,
            kind: serde_json::from_str(&row.payload_json)
                .map_err(|error| LedgerError::InvalidPayload(error.to_string()))?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct StoredArtifactRecord {
    artifact_id: String,
    run_id: String,
    title: String,
    kind: String,
    mime_type: String,
    sha256: String,
    byte_len: i64,
    created_at: String,
}

impl TryFrom<StoredArtifactRecord> for ArtifactRecord {
    type Error = ArtifactServiceError;

    fn try_from(row: StoredArtifactRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            artifact_id: ArtifactId::from_str(&row.artifact_id)
                .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?,
            run_id: RunId::from_str(&row.run_id)
                .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?,
            title: row.title,
            kind: artifact_kind_from_storage(&row.kind)?,
            mime_type: row.mime_type,
            sha256: row.sha256,
            byte_len: row
                .byte_len
                .try_into()
                .map_err(|error: std::num::TryFromIntError| {
                    ArtifactServiceError::Projection(error.to_string())
                })?,
            created_at: OffsetDateTime::parse(&row.created_at, &Rfc3339)
                .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct StoredWorkerJobRecord {
    worker_job_id: String,
    worker_id: String,
    run_id: String,
    kind: String,
    status: String,
    model: Option<String>,
    error: Option<String>,
    started_at: String,
    completed_at: Option<String>,
}

impl TryFrom<StoredWorkerJobRecord> for WorkerJobRecord {
    type Error = WorkerRuntimeError;

    fn try_from(row: StoredWorkerJobRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            worker_job_id: crate::WorkerJobId::from_str(&row.worker_job_id)
                .map_err(|error| WorkerRuntimeError::InvalidPayload(error.to_string()))?,
            worker_id: crate::WorkerId::from_str(&row.worker_id)
                .map_err(|error| WorkerRuntimeError::InvalidPayload(error.to_string()))?,
            run_id: RunId::from_str(&row.run_id)
                .map_err(|error| WorkerRuntimeError::InvalidPayload(error.to_string()))?,
            kind: worker_job_kind_from_storage(&row.kind)?,
            status: worker_job_status_from_storage(&row.status)?,
            model: row.model,
            error: row.error,
            started_at: OffsetDateTime::parse(&row.started_at, &Rfc3339)
                .map_err(|error| WorkerRuntimeError::InvalidPayload(error.to_string()))?,
            completed_at: row
                .completed_at
                .as_deref()
                .map(|value| {
                    OffsetDateTime::parse(value, &Rfc3339)
                        .map_err(|error| WorkerRuntimeError::InvalidPayload(error.to_string()))
                })
                .transpose()?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct StoredContextBundleRecord {
    context_bundle_id: String,
    run_id: String,
    title: String,
    summary: String,
    artifact_ids: String,
    event_ids: String,
    created_at: String,
}

impl TryFrom<StoredContextBundleRecord> for ContextBundleRecord {
    type Error = ContextServiceError;

    fn try_from(row: StoredContextBundleRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            context_bundle_id: ContextBundleId::from_str(&row.context_bundle_id)
                .map_err(|error| ContextServiceError::Projection(error.to_string()))?,
            run_id: RunId::from_str(&row.run_id)
                .map_err(|error| ContextServiceError::Projection(error.to_string()))?,
            title: row.title,
            summary: row.summary,
            artifact_ids: parse_ids_json(&row.artifact_ids)?,
            event_ids: parse_ids_json(&row.event_ids)?,
            created_at: OffsetDateTime::parse(&row.created_at, &Rfc3339)
                .map_err(|error| ContextServiceError::Projection(error.to_string()))?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct StoredKnowledgeSourceRecord {
    knowledge_source_id: String,
    run_id: String,
    source_kind: String,
    artifact_id: Option<String>,
    context_bundle_id: Option<String>,
    title: String,
    summary: String,
    created_at: String,
}

impl TryFrom<StoredKnowledgeSourceRecord> for KnowledgeSourceRecord {
    type Error = KnowledgeServiceError;

    fn try_from(row: StoredKnowledgeSourceRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            knowledge_source_id: KnowledgeSourceId::from_str(&row.knowledge_source_id)
                .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?,
            run_id: RunId::from_str(&row.run_id)
                .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?,
            source_kind: knowledge_source_kind_from_storage(&row.source_kind)?,
            artifact_id: parse_optional_knowledge_id(row.artifact_id)?,
            context_bundle_id: parse_optional_knowledge_id(row.context_bundle_id)?,
            title: row.title,
            summary: row.summary,
            created_at: OffsetDateTime::parse(&row.created_at, &Rfc3339)
                .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct StoredMemoryRecord {
    memory_id: String,
    knowledge_source_id: Option<String>,
    run_id: String,
    kind: String,
    title: String,
    body: String,
    confidence: f32,
    created_at: String,
}

impl TryFrom<StoredMemoryRecord> for MemoryRecord {
    type Error = MemoryServiceError;

    fn try_from(row: StoredMemoryRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            memory_id: MemoryId::from_str(&row.memory_id)
                .map_err(|error| MemoryServiceError::Projection(error.to_string()))?,
            knowledge_source_id: parse_optional_memory_id(row.knowledge_source_id)?,
            run_id: RunId::from_str(&row.run_id)
                .map_err(|error| MemoryServiceError::Projection(error.to_string()))?,
            kind: memory_kind_from_storage(&row.kind)?,
            title: row.title,
            body: row.body,
            confidence: row.confidence,
            created_at: OffsetDateTime::parse(&row.created_at, &Rfc3339)
                .map_err(|error| MemoryServiceError::Projection(error.to_string()))?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct StoredDocumentChunkRecord {
    document_chunk_id: String,
    knowledge_source_id: String,
    run_id: String,
    sequence: i64,
    text: String,
    char_start: i64,
    char_end: i64,
    created_at: String,
}

impl TryFrom<StoredDocumentChunkRecord> for DocumentChunkRecord {
    type Error = KnowledgeServiceError;

    fn try_from(row: StoredDocumentChunkRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            document_chunk_id: crate::DocumentChunkId::from_str(&row.document_chunk_id)
                .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?,
            knowledge_source_id: KnowledgeSourceId::from_str(&row.knowledge_source_id)
                .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?,
            run_id: RunId::from_str(&row.run_id)
                .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?,
            sequence: row.sequence,
            text: row.text,
            char_start: row.char_start,
            char_end: row.char_end,
            created_at: OffsetDateTime::parse(&row.created_at, &Rfc3339)
                .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct StoredRetrievalTraceRecord {
    retrieval_trace_id: String,
    run_id: String,
    query: String,
    result_chunk_ids_json: String,
    result_memory_ids_json: String,
    explanation: String,
    created_at: String,
}

impl TryFrom<StoredRetrievalTraceRecord> for RetrievalTraceRecord {
    type Error = RetrievalServiceError;

    fn try_from(row: StoredRetrievalTraceRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            retrieval_trace_id: crate::RetrievalTraceId::from_str(&row.retrieval_trace_id)
                .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?,
            run_id: RunId::from_str(&row.run_id)
                .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?,
            query: row.query,
            result_chunk_ids: parse_retrieval_ids_json(&row.result_chunk_ids_json)?,
            result_memory_ids: parse_retrieval_ids_json(&row.result_memory_ids_json)?,
            explanation: row.explanation,
            created_at: OffsetDateTime::parse(&row.created_at, &Rfc3339)
                .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct StoredSearchResult {
    document_chunk_id: String,
    knowledge_source_id: String,
    run_id: String,
    sequence: i64,
    text: String,
    char_start: i64,
    char_end: i64,
    created_at: String,
    source_kind: String,
    artifact_id: Option<String>,
    context_bundle_id: Option<String>,
    title: String,
    summary: String,
    source_created_at: String,
}

impl TryFrom<StoredSearchResult> for SearchResult {
    type Error = RetrievalServiceError;

    fn try_from(row: StoredSearchResult) -> Result<Self, Self::Error> {
        let knowledge_source_id = KnowledgeSourceId::from_str(&row.knowledge_source_id)
            .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;
        let query_reason = "keyword matched chunk text".to_string();
        Ok(Self {
            chunk: DocumentChunkRecord {
                document_chunk_id: crate::DocumentChunkId::from_str(&row.document_chunk_id)
                    .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?,
                knowledge_source_id,
                run_id: RunId::from_str(&row.run_id)
                    .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?,
                sequence: row.sequence,
                text: row.text,
                char_start: row.char_start,
                char_end: row.char_end,
                created_at: OffsetDateTime::parse(&row.created_at, &Rfc3339)
                    .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?,
            },
            source: KnowledgeSourceRecord {
                knowledge_source_id,
                run_id: RunId::from_str(&row.run_id)
                    .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?,
                source_kind: knowledge_source_kind_from_storage(&row.source_kind)
                    .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?,
                artifact_id: parse_optional_retrieval_id(row.artifact_id)?,
                context_bundle_id: parse_optional_retrieval_id(row.context_bundle_id)?,
                title: row.title,
                summary: row.summary,
                created_at: OffsetDateTime::parse(&row.source_created_at, &Rfc3339)
                    .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?,
            },
            score: 1.0,
            reason: query_reason,
        })
    }
}

#[derive(sqlx::FromRow)]
struct StoredRunProjection {
    run_id: String,
    status: String,
    instruction: String,
    started_at: String,
    completed_at: Option<String>,
    failed_at: Option<String>,
    summary: Option<String>,
    error: Option<String>,
    updated_at: String,
}

impl TryFrom<StoredRunProjection> for RunProjection {
    type Error = RunProjectionError;

    fn try_from(row: StoredRunProjection) -> Result<Self, Self::Error> {
        Ok(Self {
            run_id: RunId::from_str(&row.run_id)
                .map_err(|error| RunProjectionError::InvalidPayload(error.to_string()))?,
            status: status_from_storage(&row.status)?,
            instruction: row.instruction,
            started_at: parse_datetime(&row.started_at)?,
            completed_at: parse_optional_datetime(row.completed_at)?,
            failed_at: parse_optional_datetime(row.failed_at)?,
            summary: row.summary,
            error: row.error,
            updated_at: parse_datetime(&row.updated_at)?,
        })
    }
}

fn kind_type_for_event(kind: &OperationEventKind) -> &'static str {
    match kind {
        OperationEventKind::RunStarted { .. } => "run_started",
        OperationEventKind::RunModelRequested { .. } => "run_model_requested",
        OperationEventKind::RunAssistantMessage { .. } => "run_assistant_message",
        OperationEventKind::RunCompleted { .. } => "run_completed",
        OperationEventKind::RunFailed { .. } => "run_failed",
        OperationEventKind::ToolCallRequested { .. } => "tool_call_requested",
        OperationEventKind::ToolCallApproved { .. } => "tool_call_approved",
        OperationEventKind::ToolCallDenied { .. } => "tool_call_denied",
        OperationEventKind::ToolCallCompleted { .. } => "tool_call_completed",
        OperationEventKind::ToolCallFailed { .. } => "tool_call_failed",
        OperationEventKind::ArtifactCreated { .. } => "artifact_created",
        OperationEventKind::ContextBundleCreated { .. } => "context_bundle_created",
        OperationEventKind::KnowledgeSourceCreated { .. } => "knowledge_source_created",
        OperationEventKind::DocumentChunkCreated { .. } => "document_chunk_created",
        OperationEventKind::MemoryRecorded { .. } => "memory_recorded",
        OperationEventKind::RetrievalTraceRecorded { .. } => "retrieval_trace_recorded",
        OperationEventKind::WorkerJobStarted { .. } => "worker_job_started",
        OperationEventKind::WorkerJobCompleted { .. } => "worker_job_completed",
        OperationEventKind::WorkerJobFailed { .. } => "worker_job_failed",
    }
}

fn status_to_storage(status: &RunStatus) -> &'static str {
    match status {
        RunStatus::Running => "running",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
    }
}

fn artifact_kind_to_storage(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Text => "text",
        ArtifactKind::Markdown => "markdown",
        ArtifactKind::Json => "json",
        ArtifactKind::Binary => "binary",
    }
}

fn artifact_kind_from_storage(kind: &str) -> Result<ArtifactKind, ArtifactServiceError> {
    match kind {
        "text" => Ok(ArtifactKind::Text),
        "markdown" => Ok(ArtifactKind::Markdown),
        "json" => Ok(ArtifactKind::Json),
        "binary" => Ok(ArtifactKind::Binary),
        other => Err(ArtifactServiceError::Projection(format!(
            "unknown artifact kind: {other}"
        ))),
    }
}

fn worker_job_kind_to_storage(kind: WorkerJobKind) -> &'static str {
    match kind {
        WorkerJobKind::ModelCompletion => "model_completion",
    }
}

fn worker_job_kind_from_storage(kind: &str) -> Result<WorkerJobKind, WorkerRuntimeError> {
    match kind {
        "model_completion" => Ok(WorkerJobKind::ModelCompletion),
        other => Err(WorkerRuntimeError::InvalidPayload(format!(
            "unknown worker job kind: {other}"
        ))),
    }
}

fn worker_job_status_from_storage(status: &str) -> Result<WorkerJobStatus, WorkerRuntimeError> {
    match status {
        "started" => Ok(WorkerJobStatus::Started),
        "completed" => Ok(WorkerJobStatus::Completed),
        "failed" => Ok(WorkerJobStatus::Failed),
        other => Err(WorkerRuntimeError::InvalidPayload(format!(
            "unknown worker job status: {other}"
        ))),
    }
}

fn knowledge_source_kind_to_storage(kind: KnowledgeSourceKind) -> &'static str {
    match kind {
        KnowledgeSourceKind::Artifact => "artifact",
        KnowledgeSourceKind::ContextBundle => "context_bundle",
        KnowledgeSourceKind::Manual => "manual",
    }
}

fn knowledge_source_kind_from_storage(
    kind: &str,
) -> Result<KnowledgeSourceKind, KnowledgeServiceError> {
    match kind {
        "artifact" => Ok(KnowledgeSourceKind::Artifact),
        "context_bundle" => Ok(KnowledgeSourceKind::ContextBundle),
        "manual" => Ok(KnowledgeSourceKind::Manual),
        other => Err(KnowledgeServiceError::Projection(format!(
            "unknown knowledge source kind: {other}"
        ))),
    }
}

fn memory_kind_to_storage(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Fact => "fact",
        MemoryKind::Decision => "decision",
        MemoryKind::Preference => "preference",
        MemoryKind::Insight => "insight",
    }
}

fn memory_kind_from_storage(kind: &str) -> Result<MemoryKind, MemoryServiceError> {
    match kind {
        "fact" => Ok(MemoryKind::Fact),
        "decision" => Ok(MemoryKind::Decision),
        "preference" => Ok(MemoryKind::Preference),
        "insight" => Ok(MemoryKind::Insight),
        other => Err(MemoryServiceError::Projection(format!(
            "unknown memory kind: {other}"
        ))),
    }
}

fn non_blank_artifact_input(value: String) -> Result<String, ArtifactServiceError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(ArtifactServiceError::InvalidInput);
    }
    Ok(value)
}

fn non_blank_context_input(value: String) -> Result<String, ContextServiceError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(ContextServiceError::InvalidInput);
    }
    Ok(value)
}

fn non_blank_knowledge_input(value: String) -> Result<String, KnowledgeServiceError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(KnowledgeServiceError::InvalidInput);
    }
    Ok(value)
}

fn non_blank_memory_input(value: String) -> Result<String, MemoryServiceError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(MemoryServiceError::InvalidInput);
    }
    Ok(value)
}

fn ensure_valid_memory_confidence(confidence: f32) -> Result<(), MemoryServiceError> {
    if (0.0..=1.0).contains(&confidence) {
        Ok(())
    } else {
        Err(MemoryServiceError::InvalidInput)
    }
}

fn parse_optional_memory_id<T>(value: Option<String>) -> Result<Option<T>, MemoryServiceError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value
        .as_deref()
        .map(|id| {
            T::from_str(id).map_err(|error| MemoryServiceError::Projection(error.to_string()))
        })
        .transpose()
}

fn parse_optional_knowledge_id<T>(value: Option<String>) -> Result<Option<T>, KnowledgeServiceError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value
        .as_deref()
        .map(|id| {
            T::from_str(id).map_err(|error| KnowledgeServiceError::Projection(error.to_string()))
        })
        .transpose()
}

fn ids_to_json<T: ToString>(ids: &[T]) -> Result<String, serde_json::Error> {
    serde_json::to_string(&ids.iter().map(ToString::to_string).collect::<Vec<String>>())
}

fn parse_ids_json<T>(json: &str) -> Result<Vec<T>, ContextServiceError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let ids: Vec<String> = serde_json::from_str(json)
        .map_err(|error| ContextServiceError::Projection(error.to_string()))?;
    ids.into_iter()
        .map(|id| {
            T::from_str(&id).map_err(|error| ContextServiceError::Projection(error.to_string()))
        })
        .collect()
}

fn parse_retrieval_ids_json<T>(json: &str) -> Result<Vec<T>, RetrievalServiceError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let ids: Vec<String> = serde_json::from_str(json)
        .map_err(|error| RetrievalServiceError::Projection(error.to_string()))?;
    ids.into_iter()
        .map(|id| {
            T::from_str(&id).map_err(|error| RetrievalServiceError::Projection(error.to_string()))
        })
        .collect()
}

fn parse_optional_retrieval_id<T>(value: Option<String>) -> Result<Option<T>, RetrievalServiceError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value
        .as_deref()
        .map(|id| {
            T::from_str(id).map_err(|error| RetrievalServiceError::Projection(error.to_string()))
        })
        .transpose()
}

fn non_blank_retrieval_input(value: String) -> Result<String, RetrievalServiceError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(RetrievalServiceError::InvalidInput);
    }
    Ok(value)
}

fn ensure_valid_retrieval_limit(limit: usize) -> Result<(), RetrievalServiceError> {
    if (1..=50).contains(&limit) {
        Ok(())
    } else {
        Err(RetrievalServiceError::InvalidInput)
    }
}

fn status_from_storage(status: &str) -> Result<RunStatus, RunProjectionError> {
    match status {
        "running" => Ok(RunStatus::Running),
        "completed" => Ok(RunStatus::Completed),
        "failed" => Ok(RunStatus::Failed),
        other => Err(RunProjectionError::InvalidPayload(format!(
            "unknown run status: {other}"
        ))),
    }
}

fn format_datetime(value: OffsetDateTime) -> Result<String, RunProjectionError> {
    value
        .format(&Rfc3339)
        .map_err(|error| RunProjectionError::InvalidPayload(error.to_string()))
}

fn format_optional_datetime(
    value: Option<OffsetDateTime>,
) -> Result<Option<String>, RunProjectionError> {
    value.map(format_datetime).transpose()
}

fn parse_datetime(value: &str) -> Result<OffsetDateTime, RunProjectionError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|error| RunProjectionError::InvalidPayload(error.to_string()))
}

fn parse_optional_datetime(
    value: Option<String>,
) -> Result<Option<OffsetDateTime>, RunProjectionError> {
    value.as_deref().map(parse_datetime).transpose()
}

#[derive(Debug, Error)]
pub enum SqliteStoreError {
    #[error("sqlite connection failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("sqlite migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ArtifactId, ArtifactKind, ArtifactProjectionStore, ContextBundleId, DocumentChunkId,
        InMemoryArtifactContentStore, KnowledgeSourceId, KnowledgeSourceKind,
        LocalArtifactContentStore, MemoryId, MemoryKind, OperationEvent, OperationLedger,
        RetrievalTraceId, RunId, RunProjectionStore, RunProjector, ToolCallId, ToolCallStatus,
        ToolEffect, ToolProjector, WorkerId, WorkerJobId, WorkerJobKind, WorkerJobProjectionStore,
        WorkerJobStatus,
    };

    #[tokio::test]
    async fn sqlite_store_runs_migrations() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let count: i64 = sqlx::query_scalar(
            "select count(*) from sqlite_master where type = 'table' and name = 'operation_events'",
        )
        .fetch_one(store.pool())
        .await
        .unwrap();

        assert_eq!(count, 1);
    }

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

        assert_eq!(
            ledger.list_for_run(selected_run).await.unwrap(),
            vec![selected]
        );
    }

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
            .append(OperationEvent::new_run_model_requested(
                run_id,
                "请总结今天",
            ))
            .await
            .unwrap();
        ledger
            .append(OperationEvent::new_run_assistant_message(
                run_id,
                "今天完成了 M2。",
            ))
            .await
            .unwrap();

        let events = ledger.list_for_run(run_id).await.unwrap();

        assert_eq!(events.len(), 3);
        assert!(matches!(
            events[1].kind,
            OperationEventKind::RunModelRequested { .. }
        ));
        assert!(matches!(
            events[2].kind,
            OperationEventKind::RunAssistantMessage { .. }
        ));
    }

    #[tokio::test]
    async fn sqlite_ledger_roundtrips_tool_events() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let ledger = SqliteOperationLedger::new(store);
        let run_id = RunId::new();
        let tool_call_id = ToolCallId::new();
        let requested = OperationEvent::new_tool_call_requested(
            run_id,
            tool_call_id,
            "read_file",
            ToolEffect::ReadOnly,
            serde_json::json!({ "path": "README.md" }),
        );
        let approved = OperationEvent::new_tool_call_approved(run_id, tool_call_id, "mneme-policy");
        let completed = OperationEvent::new_tool_call_completed(
            run_id,
            tool_call_id,
            serde_json::json!({ "content": "hello" }),
        );

        ledger.append(requested).await.unwrap();
        ledger.append(approved).await.unwrap();
        ledger.append(completed).await.unwrap();

        let events = ledger.list_for_run(run_id).await.unwrap();

        assert_eq!(events.len(), 3);
        assert!(matches!(
            &events[0].kind,
            OperationEventKind::ToolCallRequested {
                tool_call_id: actual_tool_call_id,
                tool_name,
                effect,
                input_json,
            } if actual_tool_call_id == &tool_call_id
                && tool_name == "read_file"
                && effect == &ToolEffect::ReadOnly
                && input_json == &serde_json::json!({ "path": "README.md" })
        ));
        assert!(matches!(
            &events[1].kind,
            OperationEventKind::ToolCallApproved {
                tool_call_id: actual_tool_call_id,
                approved_by,
            } if actual_tool_call_id == &tool_call_id && approved_by == "mneme-policy"
        ));
        assert!(matches!(
            &events[2].kind,
            OperationEventKind::ToolCallCompleted {
                tool_call_id: actual_tool_call_id,
                output_json,
            } if actual_tool_call_id == &tool_call_id
                && output_json == &serde_json::json!({ "content": "hello" })
        ));
    }

    #[tokio::test]
    async fn sqlite_ledger_roundtrips_artifact_and_context_bundle_events() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let ledger = SqliteOperationLedger::new(store);
        let run_id = RunId::new();
        let artifact_id = ArtifactId::new();
        let context_bundle_id = ContextBundleId::new();
        let event_id = EventId::new();
        let artifact = OperationEvent::new_artifact_created(
            run_id,
            artifact_id,
            "Run summary",
            ArtifactKind::Markdown,
            "text/markdown",
            "abc123",
            42,
        );
        let context_bundle = OperationEvent::new_context_bundle_created(
            run_id,
            context_bundle_id,
            "Debug context",
            "User report and artifact",
            vec![artifact_id],
            vec![event_id],
        );

        ledger.append(artifact).await.unwrap();
        ledger.append(context_bundle).await.unwrap();

        let events = ledger.list_for_run(run_id).await.unwrap();

        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0].kind,
            OperationEventKind::ArtifactCreated {
                artifact_id: actual_artifact_id,
                kind: ArtifactKind::Markdown,
                byte_len: 42,
                ..
            } if actual_artifact_id == &artifact_id
        ));
        assert!(matches!(
            &events[1].kind,
            OperationEventKind::ContextBundleCreated {
                context_bundle_id: actual_context_bundle_id,
                artifact_ids,
                event_ids,
                ..
            } if actual_context_bundle_id == &context_bundle_id
                && artifact_ids == &vec![artifact_id]
                && event_ids == &vec![event_id]
        ));
    }

    #[tokio::test]
    async fn sqlite_ledger_roundtrips_worker_job_events() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let ledger = SqliteOperationLedger::new(store);
        let run_id = RunId::new();
        let worker_id = WorkerId::new();
        let worker_job_id = WorkerJobId::new();

        ledger
            .append(OperationEvent::new_worker_job_started(
                run_id,
                worker_job_id,
                worker_id,
                WorkerJobKind::ModelCompletion,
                Some("gpt-4o-mini".to_string()),
            ))
            .await
            .unwrap();
        ledger
            .append(OperationEvent::new_worker_job_completed(
                run_id,
                worker_job_id,
                worker_id,
                WorkerJobKind::ModelCompletion,
            ))
            .await
            .unwrap();
        ledger
            .append(OperationEvent::new_worker_job_failed(
                run_id,
                worker_job_id,
                worker_id,
                WorkerJobKind::ModelCompletion,
                "provider returned 500",
            ))
            .await
            .unwrap();

        let events = ledger.list().await.unwrap();
        assert_eq!(events.len(), 3);
        assert!(matches!(
            &events[0].kind,
            OperationEventKind::WorkerJobStarted {
                worker_job_id: actual_worker_job_id,
                worker_id: actual_worker_id,
                kind: WorkerJobKind::ModelCompletion,
                model: Some(model),
            } if actual_worker_job_id == &worker_job_id
                && actual_worker_id == &worker_id
                && model == "gpt-4o-mini"
        ));
        assert!(matches!(
            &events[1].kind,
            OperationEventKind::WorkerJobCompleted {
                worker_job_id: actual_worker_job_id,
                worker_id: actual_worker_id,
                kind: WorkerJobKind::ModelCompletion,
            } if actual_worker_job_id == &worker_job_id && actual_worker_id == &worker_id
        ));
        assert!(matches!(
            &events[2].kind,
            OperationEventKind::WorkerJobFailed {
                worker_job_id: actual_worker_job_id,
                worker_id: actual_worker_id,
                kind: WorkerJobKind::ModelCompletion,
                reason,
            } if actual_worker_job_id == &worker_job_id
                && actual_worker_id == &worker_id
                && reason == "provider returned 500"
        ));
    }

    #[tokio::test]
    async fn sqlite_ledger_roundtrips_worker_job_events_can_rebuild_projection() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let ledger = SqliteOperationLedger::new(store.clone());
        let projection = SqliteWorkerJobProjectionStore::new(store);
        let run_id = RunId::new();
        let worker_id = WorkerId::new();
        let completed_job_id = WorkerJobId::new();
        let failed_job_id = WorkerJobId::new();

        ledger
            .append(OperationEvent::new_worker_job_started(
                run_id,
                completed_job_id,
                worker_id,
                WorkerJobKind::ModelCompletion,
                Some("gpt-4o-mini".to_string()),
            ))
            .await
            .unwrap();
        ledger
            .append(OperationEvent::new_worker_job_completed(
                run_id,
                completed_job_id,
                worker_id,
                WorkerJobKind::ModelCompletion,
            ))
            .await
            .unwrap();
        ledger
            .append(OperationEvent::new_worker_job_failed(
                run_id,
                failed_job_id,
                worker_id,
                WorkerJobKind::ModelCompletion,
                "provider returned 500",
            ))
            .await
            .unwrap();

        let events = ledger.list().await.unwrap();
        projection.rebuild(&events).await.unwrap();

        let jobs = projection.list_worker_jobs().await.unwrap();
        let completed_job = jobs
            .iter()
            .find(|job| job.worker_job_id == completed_job_id)
            .unwrap();
        assert_eq!(completed_job.status, WorkerJobStatus::Completed);
        assert_eq!(completed_job.model.as_deref(), Some("gpt-4o-mini"));
        assert!(completed_job.completed_at.is_some());

        let failed_job = jobs
            .iter()
            .find(|job| job.worker_job_id == failed_job_id)
            .unwrap();
        assert_eq!(failed_job.status, WorkerJobStatus::Failed);
        assert_eq!(failed_job.error.as_deref(), Some("provider returned 500"));
        assert!(failed_job.completed_at.is_some());
    }

    #[tokio::test]
    async fn sqlite_ledger_roundtrips_knowledge_memory_retrieval_events() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let ledger = SqliteOperationLedger::new(store);
        let run_id = RunId::new();
        let source_id = KnowledgeSourceId::new();
        let chunk_id = DocumentChunkId::new();
        let memory_id = MemoryId::new();
        let trace_id = RetrievalTraceId::new();

        ledger
            .append(OperationEvent::new_knowledge_source_created(
                run_id,
                source_id,
                KnowledgeSourceKind::Manual,
                None,
                None,
                "Manual source",
                "Source summary",
            ))
            .await
            .unwrap();
        ledger
            .append(OperationEvent::new_document_chunk_created(
                run_id,
                chunk_id,
                source_id,
                0,
                "searchable text",
                0,
                15,
            ))
            .await
            .unwrap();
        ledger
            .append(OperationEvent::new_memory_recorded(
                run_id,
                memory_id,
                Some(source_id),
                MemoryKind::Fact,
                "Fact title",
                "Fact body",
                0.9,
            ))
            .await
            .unwrap();
        ledger
            .append(OperationEvent::new_retrieval_trace_recorded(
                run_id,
                trace_id,
                "searchable",
                vec![chunk_id],
                vec![memory_id],
                "deterministic keyword retrieval matched searchable text",
            ))
            .await
            .unwrap();

        let events = ledger.list().await.unwrap();
        assert_eq!(events.len(), 4);
        assert!(matches!(
            events[0].kind,
            OperationEventKind::KnowledgeSourceCreated { .. }
        ));
        assert!(matches!(
            events[1].kind,
            OperationEventKind::DocumentChunkCreated { .. }
        ));
        assert!(matches!(
            events[2].kind,
            OperationEventKind::MemoryRecorded { .. }
        ));
        assert!(matches!(
            events[3].kind,
            OperationEventKind::RetrievalTraceRecorded { .. }
        ));
    }

    #[tokio::test]
    async fn sqlite_artifact_service_creates_artifact_projection_and_ledger_event() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let run_service = SqliteRunService::new(store.clone());
        let run = run_service
            .create_run("collect artifact".to_string())
            .await
            .unwrap();
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
        let detail = artifact_service
            .get_artifact(created.artifact_id)
            .await
            .unwrap();
        let events = store.list_events().await.unwrap();

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].artifact_id, created.artifact_id);
        assert_eq!(detail.artifact.artifact_id, created.artifact_id);
        assert_eq!(detail.content.as_deref(), Some("note body"));
        assert!(events.iter().any(|event| matches!(event.kind, OperationEventKind::ArtifactCreated { artifact_id, .. } if artifact_id == created.artifact_id)));
    }

    #[tokio::test]
    async fn sqlite_context_service_creates_context_bundle_projection_and_ledger_event() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let run_service = SqliteRunService::new(store.clone());
        let run = run_service
            .create_run("bundle context".to_string())
            .await
            .unwrap();
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

    #[tokio::test]
    async fn sqlite_knowledge_service_creates_source_projection_and_ledger_event() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let run_service = SqliteRunService::new(store.clone());
        let run = run_service
            .create_run("Create source".to_string())
            .await
            .unwrap();
        let content_store = crate::InMemoryArtifactContentStore::new().shared();
        let artifact_service = SqliteArtifactService::new(store.clone(), content_store);
        let artifact = artifact_service
            .create_text_artifact(
                run.run_id,
                "Run summary".to_string(),
                crate::ArtifactKind::Markdown,
                "text/markdown".to_string(),
                "Searchable content".to_string(),
            )
            .await
            .unwrap();

        let service = SqliteKnowledgeService::new(store.clone());
        let outcome = service
            .create_source_from_artifact(
                artifact.artifact_id,
                "Run summary".to_string(),
                "Summary source".to_string(),
            )
            .await
            .unwrap();

        let sources = service.list_sources().await.unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].knowledge_source_id, outcome.knowledge_source_id);

        let events = SqliteOperationLedger::new(store).list().await.unwrap();
        assert!(events.iter().any(|event| matches!(event.kind, OperationEventKind::KnowledgeSourceCreated { knowledge_source_id, .. } if knowledge_source_id == outcome.knowledge_source_id)));
    }

    #[tokio::test]
    async fn sqlite_chunk_service_chunks_source_and_records_ledger_events() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let run_service = SqliteRunService::new(store.clone());
        let run = run_service
            .create_run("Chunk source".to_string())
            .await
            .unwrap();
        let knowledge_service = SqliteKnowledgeService::new(store.clone());
        let source = knowledge_service
            .create_manual_source(
                run.run_id,
                "Manual source".to_string(),
                "abcdefghijklmnopqrstuvwxyz".to_string(),
            )
            .await
            .unwrap();

        let service = SqliteChunkService::new(store.clone(), ChunkTextSplitter::new(10, 2));
        let chunks = service
            .chunk_source(source.knowledge_source_id)
            .await
            .unwrap();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].text, "abcdefghij");
        let events = SqliteOperationLedger::new(store).list().await.unwrap();
        assert!(events.iter().any(|event| matches!(event.kind, OperationEventKind::DocumentChunkCreated { document_chunk_id, .. } if document_chunk_id == chunks[0].document_chunk_id)));
    }

    #[tokio::test]
    async fn sqlite_memory_service_records_memory_and_ledger_event() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let run_service = SqliteRunService::new(store.clone());
        let run = run_service
            .create_run("Record memory".to_string())
            .await
            .unwrap();
        let service = SqliteMemoryService::new(store.clone());

        let outcome = service
            .record_memory(
                run.run_id,
                None,
                MemoryKind::Decision,
                "Decision".to_string(),
                "Use SQLite-first search.".to_string(),
                0.9,
            )
            .await
            .unwrap();

        let memories = service.list_memories(None).await.unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].memory_id, outcome.memory_id);
        assert_eq!(memories[0].confidence, 0.9);
        let events = SqliteOperationLedger::new(store).list().await.unwrap();
        assert!(events.iter().any(|event| matches!(event.kind, OperationEventKind::MemoryRecorded { memory_id, .. } if memory_id == outcome.memory_id)));
    }

    #[tokio::test]
    async fn sqlite_retrieval_service_searches_chunks_and_records_trace() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let run_service = SqliteRunService::new(store.clone());
        let run = run_service.create_run("Search".to_string()).await.unwrap();
        let knowledge_service = SqliteKnowledgeService::new(store.clone());
        let source = knowledge_service
            .create_manual_source(
                run.run_id,
                "Manual".to_string(),
                "SQLite deterministic retrieval".to_string(),
            )
            .await
            .unwrap();
        let chunk_service = SqliteChunkService::new(store.clone(), ChunkTextSplitter::new(1000, 0));
        chunk_service
            .chunk_source(source.knowledge_source_id)
            .await
            .unwrap();

        let service = SqliteRetrievalService::new(store.clone());
        let response = service
            .search(run.run_id, "deterministic".to_string(), 10)
            .await
            .unwrap();

        assert_eq!(response.query, "deterministic");
        assert_eq!(response.hits.len(), 1);
        assert!(response.hits[0].reason.contains("deterministic"));
        assert_eq!(
            response.hits[0].source.knowledge_source_id,
            source.knowledge_source_id
        );
        let traces = service.list_retrieval_traces().await.unwrap();
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].retrieval_trace_id, response.retrieval_trace_id);
        assert_eq!(
            traces[0].result_chunk_ids,
            vec![response.hits[0].chunk.document_chunk_id]
        );
        assert!(traces[0]
            .explanation
            .starts_with("M6 deterministic keyword retrieval"));
    }

    #[tokio::test]
    async fn sqlite_memory_service_rejects_invalid_confidence() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let run_service = SqliteRunService::new(store.clone());
        let run = run_service
            .create_run("Reject memory".to_string())
            .await
            .unwrap();
        let service = SqliteMemoryService::new(store.clone());

        assert_eq!(
            service
                .record_memory(
                    run.run_id,
                    None,
                    MemoryKind::Fact,
                    "Fact".to_string(),
                    "Body".to_string(),
                    1.1,
                )
                .await
                .unwrap_err(),
            crate::MemoryServiceError::InvalidInput
        );
        assert_eq!(store.list_events().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn sqlite_ledger_roundtrips_memory_events_can_rebuild_projection() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let ledger = SqliteOperationLedger::new(store.clone());
        let projection = SqliteMemoryProjectionStore::new(store);
        let run_id = RunId::new();
        let memory_id = MemoryId::new();
        let event = OperationEvent::new_memory_recorded(
            run_id,
            memory_id,
            None,
            MemoryKind::Insight,
            "Insight",
            "SQLite ledger can rebuild memory projections.",
            0.8,
        );

        ledger.append(event.clone()).await.unwrap();
        projection.rebuild(&[event]).await.unwrap();

        let memories = projection.list_memories(None).await.unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].memory_id, memory_id);
        assert_eq!(memories[0].run_id, run_id);
    }

    #[tokio::test]
    async fn sqlite_ledger_roundtrips_retrieval_trace_events_can_rebuild_projection() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let ledger = SqliteOperationLedger::new(store.clone());
        let projection = SqliteRetrievalTraceProjectionStore::new(store);
        let run_id = RunId::new();
        let chunk_id = DocumentChunkId::new();
        let memory_id = MemoryId::new();
        let trace_id = RetrievalTraceId::new();
        let event = OperationEvent::new_retrieval_trace_recorded(
            run_id,
            trace_id,
            "deterministic",
            vec![chunk_id],
            vec![memory_id],
            "M6 deterministic keyword retrieval matched one chunk",
        );

        ledger.append(event.clone()).await.unwrap();
        projection.rebuild(&[event]).await.unwrap();

        let traces = projection.list_retrieval_traces().await.unwrap();
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].retrieval_trace_id, trace_id);
        assert_eq!(traces[0].run_id, run_id);
        assert_eq!(traces[0].result_chunk_ids, vec![chunk_id]);
        assert_eq!(traces[0].result_memory_ids, vec![memory_id]);
    }

    #[tokio::test]
    async fn sqlite_ledger_roundtrips_document_chunk_events_can_rebuild_projection() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let ledger = SqliteOperationLedger::new(store.clone());
        let projection = SqliteDocumentChunkProjectionStore::new(store);
        let run_id = RunId::new();
        let source_id = KnowledgeSourceId::new();
        let chunk_id = DocumentChunkId::new();
        let event = OperationEvent::new_document_chunk_created(
            run_id,
            chunk_id,
            source_id,
            0,
            "searchable",
            0,
            10,
        );

        ledger.append(event.clone()).await.unwrap();
        projection.rebuild(&[event]).await.unwrap();

        let chunks = projection.list_chunks(None).await.unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].document_chunk_id, chunk_id);
    }

    #[tokio::test]
    async fn sqlite_ledger_roundtrips_knowledge_source_events_can_rebuild_projection() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
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

    #[tokio::test]
    async fn sqlite_context_service_validates_inputs_and_references_without_side_effects() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let run_service = SqliteRunService::new(store.clone());
        let run = run_service
            .create_run("validate context".to_string())
            .await
            .unwrap();
        let event_count_before = store.list_events().await.unwrap().len();
        let context_service = SqliteContextService::new(store.clone());

        assert_eq!(
            context_service
                .create_context_bundle(
                    run.run_id,
                    "   ".to_string(),
                    "summary".to_string(),
                    Vec::new(),
                    Vec::new(),
                )
                .await
                .unwrap_err(),
            ContextServiceError::InvalidInput
        );
        assert_eq!(
            context_service
                .create_context_bundle(
                    run.run_id,
                    "title".to_string(),
                    "   ".to_string(),
                    Vec::new(),
                    Vec::new(),
                )
                .await
                .unwrap_err(),
            ContextServiceError::InvalidInput
        );
        assert_eq!(
            context_service
                .create_context_bundle(
                    RunId::new(),
                    "title".to_string(),
                    "summary".to_string(),
                    Vec::new(),
                    Vec::new(),
                )
                .await
                .unwrap_err(),
            ContextServiceError::UnknownRun
        );
        assert_eq!(
            context_service
                .create_context_bundle(
                    run.run_id,
                    "title".to_string(),
                    "summary".to_string(),
                    vec![ArtifactId::new()],
                    Vec::new(),
                )
                .await
                .unwrap_err(),
            ContextServiceError::UnknownArtifact
        );
        assert_eq!(
            context_service
                .create_context_bundle(
                    run.run_id,
                    "title".to_string(),
                    "summary".to_string(),
                    Vec::new(),
                    vec![EventId::new()],
                )
                .await
                .unwrap_err(),
            ContextServiceError::UnknownEvent
        );
        assert_eq!(store.list_events().await.unwrap().len(), event_count_before);
        assert!(context_service
            .list_context_bundles()
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn sqlite_ledger_roundtrips_context_bundle_events_can_rebuild_context_projection() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let ledger = SqliteOperationLedger::new(store.clone());
        let projection = SqliteContextBundleProjectionStore::new(store.clone());
        let context_service = SqliteContextService::new(store.clone());
        let run_id = RunId::new();
        let context_bundle_id = ContextBundleId::new();
        let started = OperationEvent::new_run_started(run_id, "ledger-only context rebuild");
        let context_bundle = OperationEvent::new_context_bundle_created(
            run_id,
            context_bundle_id,
            "Ledger context",
            "Facts selected from the ledger",
            Vec::new(),
            vec![started.id],
        );

        ledger.append(started.clone()).await.unwrap();
        ledger.append(context_bundle.clone()).await.unwrap();
        assert!(projection.list_context_bundles().await.unwrap().is_empty());

        let events = ledger.list().await.unwrap();
        projection.rebuild(&events).await.unwrap();

        let bundles = context_service.list_context_bundles().await.unwrap();
        let stored = context_service
            .get_context_bundle(context_bundle_id)
            .await
            .unwrap();
        assert_eq!(bundles, vec![stored.clone()]);
        assert_eq!(stored.context_bundle_id, context_bundle_id);
        assert_eq!(stored.run_id, run_id);
        assert_eq!(stored.title, "Ledger context");
        assert_eq!(stored.summary, "Facts selected from the ledger");
        assert!(stored.artifact_ids.is_empty());
        assert_eq!(stored.event_ids, vec![started.id]);
        assert_eq!(stored.created_at, context_bundle.occurred_at);

        sqlx::query("delete from context_bundles")
            .execute(store.pool())
            .await
            .unwrap();
        let rebuilt = context_service
            .rebuild_context_bundle_projections()
            .await
            .unwrap();
        assert_eq!(rebuilt, vec![stored]);
    }

    #[tokio::test]
    async fn sqlite_ledger_roundtrips_artifact_events_can_rebuild_artifact_projection() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let ledger = SqliteOperationLedger::new(store.clone());
        let projection = SqliteArtifactProjectionStore::new(store.clone());
        let run_id = RunId::new();
        let artifact_id = ArtifactId::new();
        let started = OperationEvent::new_run_started(run_id, "ledger-only artifact rebuild");
        let artifact = OperationEvent::new_artifact_created(
            run_id,
            artifact_id,
            "Ledger artifact",
            ArtifactKind::Markdown,
            "text/markdown",
            "abc123",
            42,
        );

        ledger.append(started).await.unwrap();
        ledger.append(artifact.clone()).await.unwrap();
        assert!(projection.list_artifacts().await.unwrap().is_empty());

        let events = ledger.list().await.unwrap();
        projection.rebuild(&events).await.unwrap();

        let artifacts = projection.list_artifacts().await.unwrap();
        let stored = projection.get_artifact(artifact_id).await.unwrap();
        assert_eq!(artifacts, vec![stored.clone()]);
        assert_eq!(stored.artifact_id, artifact_id);
        assert_eq!(stored.run_id, run_id);
        assert_eq!(stored.title, "Ledger artifact");
        assert_eq!(stored.kind, ArtifactKind::Markdown);
        assert_eq!(stored.mime_type, "text/markdown");
        assert_eq!(stored.sha256, "abc123");
        assert_eq!(stored.byte_len, 42);
        assert_eq!(stored.created_at, artifact.occurred_at);
    }

    #[tokio::test]
    async fn sqlite_artifact_service_updates_running_run_updated_at() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let run_service = SqliteRunService::new(store.clone());
        let run = run_service
            .create_run("collect artifact".to_string())
            .await
            .unwrap();
        let artifact_service =
            SqliteArtifactService::new(store.clone(), InMemoryArtifactContentStore::new().shared());

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

        let artifact_event = store
            .list_events()
            .await
            .unwrap()
            .into_iter()
            .find(|event| matches!(event.kind, OperationEventKind::ArtifactCreated { artifact_id, .. } if artifact_id == created.artifact_id))
            .unwrap();
        let runs = run_service.list_runs().await.unwrap();
        assert_eq!(runs[0].run_id, run.run_id);
        assert_eq!(runs[0].updated_at, artifact_event.occurred_at);
    }

    #[tokio::test]
    async fn sqlite_artifact_service_rejects_unknown_run_without_writing_content_or_ledger() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let content_root = std::env::temp_dir().join(format!(
            "mneme-sqlite-artifact-unknown-run-{}",
            std::process::id()
        ));
        if content_root.exists() {
            std::fs::remove_dir_all(&content_root).unwrap();
        }
        let artifact_service = SqliteArtifactService::new(
            store.clone(),
            LocalArtifactContentStore::new(content_root.clone()).shared(),
        );

        let error = artifact_service
            .create_text_artifact(
                RunId::new(),
                "Orphan".to_string(),
                ArtifactKind::Text,
                "text/plain".to_string(),
                "orphan body".to_string(),
            )
            .await
            .unwrap_err();

        assert_eq!(error, ArtifactServiceError::UnknownRun);
        assert!(store.list_events().await.unwrap().is_empty());
        assert!(artifact_service.list_artifacts().await.unwrap().is_empty());
        assert!(!content_root.exists());
    }

    #[tokio::test]
    async fn sqlite_artifact_service_rejects_blank_inputs_without_side_effects() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let run_service = SqliteRunService::new(store.clone());
        let run = run_service
            .create_run("collect artifact".to_string())
            .await
            .unwrap();
        let events_before = store.list_events().await.unwrap().len();
        let artifact_service =
            SqliteArtifactService::new(store.clone(), InMemoryArtifactContentStore::new().shared());

        let error = artifact_service
            .create_text_artifact(
                run.run_id,
                "   ".to_string(),
                ArtifactKind::Text,
                "text/plain".to_string(),
                "note body".to_string(),
            )
            .await
            .unwrap_err();

        assert_eq!(error, ArtifactServiceError::InvalidInput);
        assert_eq!(store.list_events().await.unwrap().len(), events_before);
        assert!(artifact_service.list_artifacts().await.unwrap().is_empty());
    }

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

    #[tokio::test]
    async fn sqlite_run_service_creates_event_and_projection_in_one_service_call() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let service = SqliteRunService::new(store.clone());

        let outcome = service
            .create_run("事务创建 run".to_string())
            .await
            .unwrap();

        let events = SqliteOperationLedger::new(store.clone())
            .list()
            .await
            .unwrap();
        let runs = SqliteRunProjectionStore::new(store)
            .list_runs()
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(runs.len(), 1);
        assert_eq!(events[0].run_id, outcome.run_id);
        assert_eq!(runs[0].run_id, outcome.run_id);
    }

    #[tokio::test]
    async fn sqlite_run_service_rejects_blank_instruction() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let service = SqliteRunService::new(store.clone());

        let error = service.create_run("   ".to_string()).await.unwrap_err();

        assert_eq!(error, RunServiceError::InvalidInstruction);
        assert!(SqliteOperationLedger::new(store.clone())
            .list()
            .await
            .unwrap()
            .is_empty());
        assert!(SqliteRunProjectionStore::new(store)
            .list_runs()
            .await
            .unwrap()
            .is_empty());
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
        let created = service
            .create_run("失败后不能请求工具".to_string())
            .await
            .unwrap();
        service
            .fail_run(created.run_id, "模型失败".to_string())
            .await
            .unwrap();
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
        assert_eq!(
            service.list_events().await.unwrap().len(),
            event_count_before
        );
    }

    #[tokio::test]
    async fn sqlite_run_service_records_failed_lifecycle() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let service = SqliteRunService::new(store);

        let created = service
            .create_run("会失败的 run".to_string())
            .await
            .unwrap();
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

    #[tokio::test]
    async fn sqlite_run_service_rejects_unknown_lifecycle_run_id() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let service = SqliteRunService::new(store);

        let error = service
            .fail_run(RunId::new(), "未知 run".to_string())
            .await
            .unwrap_err();

        assert_eq!(error, RunServiceError::InvalidRunId);
        assert!(service.list_events().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn sqlite_run_service_rejects_post_completed_lifecycle_without_side_effects() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let service = SqliteRunService::new(store.clone());

        let created = service.create_run("整理今天".to_string()).await.unwrap();
        service
            .complete_run(created.run_id, "总结完成".to_string())
            .await
            .unwrap();
        let event_count_before: i64 = sqlx::query_scalar("select count(*) from operation_events")
            .fetch_one(store.pool())
            .await
            .unwrap();

        let error = service
            .request_model(created.run_id, "不应恢复运行".to_string())
            .await
            .unwrap_err();

        assert_eq!(error, RunServiceError::RunAlreadyTerminal);
        let event_count_after: i64 = sqlx::query_scalar("select count(*) from operation_events")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(event_count_after, event_count_before);
        let runs = service.list_runs().await.unwrap();
        assert_eq!(runs[0].status, RunStatus::Completed);
        assert_eq!(runs[0].summary.as_deref(), Some("总结完成"));
    }

    #[tokio::test]
    async fn sqlite_run_store_keeps_failed_run_terminal_after_late_intermediate_event() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let runs = SqliteRunProjectionStore::new(store);
        let run_id = RunId::new();
        let tool_call_id = ToolCallId::new();
        let started = OperationEvent::new_run_started(run_id, "执行任务");
        let failed = OperationEvent::new_run_failed(run_id, "模型不可用");
        let late_message = OperationEvent::new_run_assistant_message(run_id, "不应恢复运行");
        let late_tool_request = OperationEvent::new_tool_call_requested(
            run_id,
            tool_call_id,
            "read_file",
            ToolEffect::ReadOnly,
            serde_json::json!({ "path": "README.md" }),
        );
        let late_tool_failure =
            OperationEvent::new_tool_call_failed(run_id, tool_call_id, "不应覆盖终态");

        runs.apply(&started).await.unwrap();
        runs.apply(&failed).await.unwrap();
        runs.apply(&late_message).await.unwrap();
        runs.apply(&late_tool_request).await.unwrap();
        runs.apply(&late_tool_failure).await.unwrap();

        let listed = runs.list_runs().await.unwrap();
        assert_eq!(listed[0].status, RunStatus::Failed);
        assert_eq!(listed[0].error.as_deref(), Some("模型不可用"));
        assert_eq!(listed[0].failed_at, Some(failed.occurred_at));
        assert_eq!(listed[0].updated_at, failed.occurred_at);
    }

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

        assert_eq!(
            runs.list_runs().await.unwrap(),
            RunProjector::replay(&events)
        );
    }

    #[tokio::test]
    async fn sqlite_run_store_rebuild_handles_duplicate_started_events() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let runs = SqliteRunProjectionStore::new(store);
        let run_id = RunId::new();
        let events = vec![
            OperationEvent::new_run_started(run_id, "第一次启动"),
            OperationEvent::new_run_started(run_id, "第二次启动"),
        ];

        runs.rebuild(&events).await.unwrap();

        assert_eq!(
            runs.list_runs().await.unwrap(),
            RunProjector::replay(&events)
        );
    }

    #[tokio::test]
    async fn sqlite_run_store_ignores_unknown_terminal_events() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let runs = SqliteRunProjectionStore::new(store);
        let event = OperationEvent::new_run_failed(RunId::new(), "未知 run 失败");

        runs.apply(&event).await.unwrap();

        assert!(runs.list_runs().await.unwrap().is_empty());
    }
}
