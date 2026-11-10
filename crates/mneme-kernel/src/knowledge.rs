use crate::{
    ArtifactId, ArtifactServiceError, ContextBundleId, ContextServiceError, DocumentChunkId,
    KnowledgeSourceId, MemoryId, OperationEvent, OperationEventKind, RetrievalTraceId, RunId,
    SharedArtifactProjectionStore, SharedContextBundleProjectionStore, SharedOperationLedger,
    SharedRunProjectionStore,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use time::OffsetDateTime;
use tokio::sync::RwLock;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeSourceRecord {
    pub knowledge_source_id: KnowledgeSourceId,
    pub run_id: RunId,
    pub source_kind: KnowledgeSourceKind,
    pub artifact_id: Option<ArtifactId>,
    pub context_bundle_id: Option<ContextBundleId>,
    pub title: String,
    pub summary: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentChunkRecord {
    pub document_chunk_id: DocumentChunkId,
    pub knowledge_source_id: KnowledgeSourceId,
    pub run_id: RunId,
    pub sequence: i64,
    pub text: String,
    pub char_start: i64,
    pub char_end: i64,
    pub created_at: OffsetDateTime,
}

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

impl ChunkTextSplitter {
    pub fn new(target_chars: usize, overlap_chars: usize) -> Self {
        let target_chars = target_chars.max(1);
        let overlap_chars = overlap_chars.min(target_chars - 1);
        Self {
            target_chars,
            overlap_chars,
        }
    }

    pub fn split(&self, text: &str) -> Vec<ChunkDraft> {
        let chars = text.chars().collect::<Vec<_>>();
        if chars.is_empty() {
            return Vec::new();
        }

        let mut chunks = Vec::new();
        let mut start = 0;
        let step = self.target_chars - self.overlap_chars;
        while start < chars.len() {
            let end = (start + self.target_chars).min(chars.len());
            if start > 0 && end - start <= self.overlap_chars {
                break;
            }
            chunks.push(ChunkDraft {
                sequence: chunks.len() as i64,
                text: chars[start..end].iter().collect(),
                char_start: start as i64,
                char_end: end as i64,
            });
            start += step;
        }

        chunks
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub memory_id: MemoryId,
    pub knowledge_source_id: Option<KnowledgeSourceId>,
    pub run_id: RunId,
    pub kind: MemoryKind,
    pub title: String,
    pub body: String,
    pub confidence: f32,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetrievalTraceRecord {
    pub retrieval_trace_id: RetrievalTraceId,
    pub run_id: RunId,
    pub query: String,
    pub result_chunk_ids: Vec<DocumentChunkId>,
    pub result_memory_ids: Vec<MemoryId>,
    pub explanation: String,
    pub created_at: OffsetDateTime,
}

pub type SharedKnowledgeSourceProjectionStore = Arc<dyn KnowledgeSourceProjectionStore>;
pub type SharedDocumentChunkProjectionStore = Arc<dyn DocumentChunkProjectionStore>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateKnowledgeSourceOutcome {
    pub knowledge_source_id: KnowledgeSourceId,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum KnowledgeServiceError {
    #[error("knowledge source input must not be blank")]
    InvalidInput,
    #[error("run id is not known to the knowledge service")]
    UnknownRun,
    #[error("artifact id is not known to the knowledge service")]
    UnknownArtifact,
    #[error("context bundle id is not known to the knowledge service")]
    UnknownContextBundle,
    #[error("knowledge source id is not known to the knowledge service")]
    UnknownKnowledgeSource,
    #[error("operation ledger failed: {0}")]
    Ledger(String),
    #[error("knowledge source projection failed: {0}")]
    Projection(String),
}

#[async_trait]
pub trait KnowledgeSourceProjectionStore: Send + Sync {
    async fn apply(&self, event: &OperationEvent) -> Result<(), KnowledgeServiceError>;
    async fn list_sources(&self) -> Result<Vec<KnowledgeSourceRecord>, KnowledgeServiceError>;
    async fn get_source(
        &self,
        knowledge_source_id: KnowledgeSourceId,
    ) -> Result<KnowledgeSourceRecord, KnowledgeServiceError>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryKnowledgeSourceProjectionStore {
    sources: Arc<RwLock<Vec<KnowledgeSourceRecord>>>,
}

impl InMemoryKnowledgeSourceProjectionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared(self) -> SharedKnowledgeSourceProjectionStore {
        Arc::new(self)
    }
}

#[async_trait]
impl KnowledgeSourceProjectionStore for InMemoryKnowledgeSourceProjectionStore {
    async fn apply(&self, event: &OperationEvent) -> Result<(), KnowledgeServiceError> {
        if let OperationEventKind::KnowledgeSourceCreated {
            knowledge_source_id,
            source_kind,
            artifact_id,
            context_bundle_id,
            title,
            summary,
        } = &event.kind
        {
            let mut sources = self.sources.write().await;
            let record = KnowledgeSourceRecord {
                knowledge_source_id: *knowledge_source_id,
                run_id: event.run_id,
                source_kind: *source_kind,
                artifact_id: *artifact_id,
                context_bundle_id: *context_bundle_id,
                title: title.clone(),
                summary: summary.clone(),
                created_at: event.occurred_at,
            };

            if let Some(existing) = sources
                .iter_mut()
                .find(|source| source.knowledge_source_id == *knowledge_source_id)
            {
                *existing = record;
            } else {
                sources.push(record);
            }
            sources.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        }

        Ok(())
    }

    async fn list_sources(&self) -> Result<Vec<KnowledgeSourceRecord>, KnowledgeServiceError> {
        Ok(self.sources.read().await.clone())
    }

    async fn get_source(
        &self,
        knowledge_source_id: KnowledgeSourceId,
    ) -> Result<KnowledgeSourceRecord, KnowledgeServiceError> {
        self.sources
            .read()
            .await
            .iter()
            .find(|source| source.knowledge_source_id == knowledge_source_id)
            .cloned()
            .ok_or(KnowledgeServiceError::UnknownKnowledgeSource)
    }
}

#[async_trait]
pub trait DocumentChunkProjectionStore: Send + Sync {
    async fn apply(&self, event: &OperationEvent) -> Result<(), KnowledgeServiceError>;
    async fn list_chunks(
        &self,
        knowledge_source_id: Option<KnowledgeSourceId>,
    ) -> Result<Vec<DocumentChunkRecord>, KnowledgeServiceError>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryDocumentChunkProjectionStore {
    chunks: Arc<RwLock<Vec<DocumentChunkRecord>>>,
}

impl InMemoryDocumentChunkProjectionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared(self) -> SharedDocumentChunkProjectionStore {
        Arc::new(self)
    }
}

#[async_trait]
impl DocumentChunkProjectionStore for InMemoryDocumentChunkProjectionStore {
    async fn apply(&self, event: &OperationEvent) -> Result<(), KnowledgeServiceError> {
        if let OperationEventKind::DocumentChunkCreated {
            document_chunk_id,
            knowledge_source_id,
            sequence,
            text,
            char_start,
            char_end,
        } = &event.kind
        {
            let mut chunks = self.chunks.write().await;
            let record = DocumentChunkRecord {
                document_chunk_id: *document_chunk_id,
                knowledge_source_id: *knowledge_source_id,
                run_id: event.run_id,
                sequence: *sequence,
                text: text.clone(),
                char_start: *char_start,
                char_end: *char_end,
                created_at: event.occurred_at,
            };

            if let Some(existing) = chunks
                .iter_mut()
                .find(|chunk| chunk.document_chunk_id == *document_chunk_id)
            {
                *existing = record;
            } else {
                chunks.push(record);
            }
            chunks.sort_by(|left, right| {
                left.knowledge_source_id
                    .to_string()
                    .cmp(&right.knowledge_source_id.to_string())
                    .then(left.sequence.cmp(&right.sequence))
                    .then(left.created_at.cmp(&right.created_at))
            });
        }

        Ok(())
    }

    async fn list_chunks(
        &self,
        knowledge_source_id: Option<KnowledgeSourceId>,
    ) -> Result<Vec<DocumentChunkRecord>, KnowledgeServiceError> {
        Ok(self
            .chunks
            .read()
            .await
            .iter()
            .filter(|chunk| {
                knowledge_source_id
                    .map(|source_id| chunk.knowledge_source_id == source_id)
                    .unwrap_or(true)
            })
            .cloned()
            .collect())
    }
}

#[derive(Clone)]
pub struct ComposedKnowledgeService {
    ledger: SharedOperationLedger,
    run_store: SharedRunProjectionStore,
    artifact_projection: SharedArtifactProjectionStore,
    context_projection: SharedContextBundleProjectionStore,
    source_projection: SharedKnowledgeSourceProjectionStore,
}

impl ComposedKnowledgeService {
    pub fn new(
        ledger: SharedOperationLedger,
        run_store: SharedRunProjectionStore,
        artifact_projection: SharedArtifactProjectionStore,
        context_projection: SharedContextBundleProjectionStore,
        source_projection: SharedKnowledgeSourceProjectionStore,
    ) -> Self {
        Self {
            ledger,
            run_store,
            artifact_projection,
            context_projection,
            source_projection,
        }
    }

    async fn ensure_known_run(&self, run_id: RunId) -> Result<(), KnowledgeServiceError> {
        let runs = self
            .run_store
            .list_runs()
            .await
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;
        if runs.iter().any(|run| run.run_id == run_id) {
            Ok(())
        } else {
            Err(KnowledgeServiceError::UnknownRun)
        }
    }

    pub async fn create_source_from_artifact(
        &self,
        artifact_id: ArtifactId,
        title: String,
        summary: String,
    ) -> Result<CreateKnowledgeSourceOutcome, KnowledgeServiceError> {
        let title = non_blank_knowledge_input(title)?;
        let summary = non_blank_knowledge_input(summary)?;
        let artifact = self
            .artifact_projection
            .get_artifact(artifact_id)
            .await
            .map_err(map_artifact_error)?;
        self.create_source(
            artifact.run_id,
            KnowledgeSourceKind::Artifact,
            Some(artifact_id),
            None,
            title,
            summary,
        )
        .await
    }

    pub async fn create_source_from_context_bundle(
        &self,
        context_bundle_id: ContextBundleId,
        title: String,
        summary: String,
    ) -> Result<CreateKnowledgeSourceOutcome, KnowledgeServiceError> {
        let title = non_blank_knowledge_input(title)?;
        let summary = non_blank_knowledge_input(summary)?;
        let bundle = self
            .context_projection
            .get_context_bundle(context_bundle_id)
            .await
            .map_err(map_context_error)?;
        self.create_source(
            bundle.run_id,
            KnowledgeSourceKind::ContextBundle,
            None,
            Some(context_bundle_id),
            title,
            summary,
        )
        .await
    }

    pub async fn create_manual_source(
        &self,
        run_id: RunId,
        title: String,
        summary: String,
    ) -> Result<CreateKnowledgeSourceOutcome, KnowledgeServiceError> {
        let title = non_blank_knowledge_input(title)?;
        let summary = non_blank_knowledge_input(summary)?;
        self.ensure_known_run(run_id).await?;
        self.create_source(
            run_id,
            KnowledgeSourceKind::Manual,
            None,
            None,
            title,
            summary,
        )
        .await
    }

    async fn create_source(
        &self,
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

        self.ledger
            .append(event.clone())
            .await
            .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;
        self.run_store
            .apply(&event)
            .await
            .map_err(|error| KnowledgeServiceError::Projection(error.to_string()))?;
        self.source_projection.apply(&event).await?;

        Ok(CreateKnowledgeSourceOutcome {
            knowledge_source_id,
        })
    }

    pub async fn list_sources(&self) -> Result<Vec<KnowledgeSourceRecord>, KnowledgeServiceError> {
        self.source_projection.list_sources().await
    }

    pub async fn get_source(
        &self,
        knowledge_source_id: KnowledgeSourceId,
    ) -> Result<KnowledgeSourceRecord, KnowledgeServiceError> {
        self.source_projection.get_source(knowledge_source_id).await
    }
}

#[derive(Clone)]
pub struct ComposedChunkService {
    ledger: SharedOperationLedger,
    source_projection: SharedKnowledgeSourceProjectionStore,
    chunk_projection: SharedDocumentChunkProjectionStore,
    splitter: ChunkTextSplitter,
}

impl ComposedChunkService {
    pub fn new(
        ledger: SharedOperationLedger,
        source_projection: SharedKnowledgeSourceProjectionStore,
        chunk_projection: SharedDocumentChunkProjectionStore,
        splitter: ChunkTextSplitter,
    ) -> Self {
        Self {
            ledger,
            source_projection,
            chunk_projection,
            splitter,
        }
    }

    pub async fn chunk_source(
        &self,
        knowledge_source_id: KnowledgeSourceId,
    ) -> Result<Vec<DocumentChunkRecord>, KnowledgeServiceError> {
        let source = self
            .source_projection
            .get_source(knowledge_source_id)
            .await?;
        let drafts = self.splitter.split(&source.summary);
        let mut chunks = Vec::with_capacity(drafts.len());

        for draft in drafts {
            let document_chunk_id = DocumentChunkId::new();
            let event = OperationEvent::new_document_chunk_created(
                source.run_id,
                document_chunk_id,
                source.knowledge_source_id,
                draft.sequence,
                draft.text,
                draft.char_start,
                draft.char_end,
            );
            self.ledger
                .append(event.clone())
                .await
                .map_err(|error| KnowledgeServiceError::Ledger(error.to_string()))?;
            self.chunk_projection.apply(&event).await?;
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

        Ok(chunks)
    }

    pub async fn list_chunks(
        &self,
        knowledge_source_id: Option<KnowledgeSourceId>,
    ) -> Result<Vec<DocumentChunkRecord>, KnowledgeServiceError> {
        self.chunk_projection.list_chunks(knowledge_source_id).await
    }
}

fn non_blank_knowledge_input(value: String) -> Result<String, KnowledgeServiceError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(KnowledgeServiceError::InvalidInput);
    }
    Ok(value)
}

fn map_artifact_error(error: ArtifactServiceError) -> KnowledgeServiceError {
    match error {
        ArtifactServiceError::UnknownArtifact => KnowledgeServiceError::UnknownArtifact,
        other => KnowledgeServiceError::Projection(other.to_string()),
    }
}

fn map_context_error(error: ContextServiceError) -> KnowledgeServiceError {
    match error {
        ContextServiceError::UnknownContextBundle => KnowledgeServiceError::UnknownContextBundle,
        other => KnowledgeServiceError::Projection(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArtifactId, OperationEvent, RunId, RunService};
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

    #[tokio::test]
    async fn composed_chunk_service_chunks_manual_source() {
        let ledger = crate::InMemoryOperationLedger::new().shared();
        let run_store = crate::InMemoryRunProjectionStore::new().shared();
        let artifact_projection = crate::InMemoryArtifactProjectionStore::new().shared();
        let context_projection = crate::InMemoryContextBundleProjectionStore::new().shared();
        let source_projection = InMemoryKnowledgeSourceProjectionStore::new().shared();
        let chunk_projection = InMemoryDocumentChunkProjectionStore::new().shared();

        let run_service = crate::ComposedRunService::new(ledger.clone(), run_store.clone());
        let run = run_service
            .create_run("Chunk source".to_string())
            .await
            .unwrap();
        let knowledge_service = ComposedKnowledgeService::new(
            ledger.clone(),
            run_store,
            artifact_projection,
            context_projection,
            source_projection.clone(),
        );
        let source = knowledge_service
            .create_manual_source(
                run.run_id,
                "Manual source".to_string(),
                "abcdefghijklmnopqrstuvwxyz".to_string(),
            )
            .await
            .unwrap();

        let chunk_service = ComposedChunkService::new(
            ledger.clone(),
            source_projection,
            chunk_projection,
            ChunkTextSplitter::new(10, 2),
        );
        let chunks = chunk_service
            .chunk_source(source.knowledge_source_id)
            .await
            .unwrap();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].sequence, 0);
        assert_eq!(chunks[0].knowledge_source_id, source.knowledge_source_id);
        assert_eq!(chunks[0].run_id, run.run_id);
        assert_eq!(chunks[0].text, "abcdefghij");
    }

    #[tokio::test]
    async fn composed_knowledge_service_creates_artifact_source() {
        let ledger = crate::InMemoryOperationLedger::new().shared();
        let run_store = crate::InMemoryRunProjectionStore::new().shared();
        let artifact_projection = crate::InMemoryArtifactProjectionStore::new().shared();
        let context_projection = crate::InMemoryContextBundleProjectionStore::new().shared();
        let source_projection = InMemoryKnowledgeSourceProjectionStore::new().shared();

        let run_service = crate::ComposedRunService::new(ledger.clone(), run_store.clone());
        let run = run_service
            .create_run("Create a source".to_string())
            .await
            .unwrap();
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
            .create_source_from_artifact(
                artifact_id,
                "Run summary".to_string(),
                "Summary source".to_string(),
            )
            .await
            .unwrap();

        let sources = service.list_sources().await.unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].knowledge_source_id, outcome.knowledge_source_id);
        assert_eq!(sources[0].run_id, run.run_id);
        assert_eq!(sources[0].source_kind, KnowledgeSourceKind::Artifact);
        assert_eq!(sources[0].artifact_id, Some(artifact_id));
    }
}
