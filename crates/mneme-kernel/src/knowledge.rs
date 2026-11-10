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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArtifactId, RunId};
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
