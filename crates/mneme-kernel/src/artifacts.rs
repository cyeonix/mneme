use crate::{ArtifactId, ContextBundleId, EventId, RunId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Text,
    Markdown,
    Json,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub artifact_id: ArtifactId,
    pub run_id: RunId,
    pub title: String,
    pub kind: ArtifactKind,
    pub mime_type: String,
    pub sha256: String,
    pub byte_len: u64,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactDetail {
    pub artifact: ArtifactRecord,
    pub content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextBundleRecord {
    pub context_bundle_id: ContextBundleId,
    pub run_id: RunId,
    pub title: String,
    pub summary: String,
    pub artifact_ids: Vec<ArtifactId>,
    pub event_ids: Vec<EventId>,
    pub created_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArtifactId, ContextBundleId, EventId, RunId};

    #[test]
    fn artifact_kind_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&ArtifactKind::Markdown).unwrap(),
            "\"markdown\""
        );
        assert_eq!(
            serde_json::to_string(&ArtifactKind::Json).unwrap(),
            "\"json\""
        );
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
