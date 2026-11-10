use crate::{
    ArtifactId, ContextBundleId, EventId, OperationEvent, OperationEventKind, RunId,
    SharedOperationLedger, SharedRunProjectionStore,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use thiserror::Error;
use time::OffsetDateTime;
use tokio::sync::RwLock;

pub type SharedArtifactContentStore = Arc<dyn ArtifactContentStore>;
pub type SharedArtifactProjectionStore = Arc<dyn ArtifactProjectionStore>;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredArtifactContent {
    pub sha256: String,
    pub byte_len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateArtifactOutcome {
    pub artifact_id: ArtifactId,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ArtifactServiceError {
    #[error("artifact input must not be blank")]
    InvalidInput,
    #[error("run id is not known to the artifact service")]
    UnknownRun,
    #[error("artifact id is not known to the artifact service")]
    UnknownArtifact,
    #[error("artifact content store failed: {0}")]
    ContentStore(String),
    #[error("operation ledger failed: {0}")]
    Ledger(String),
    #[error("artifact projection failed: {0}")]
    Projection(String),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ArtifactStoreError {
    #[error("artifact store I/O error: {0}")]
    Io(String),
    #[error("artifact content not found: {0}")]
    NotFound(String),
}

impl From<std::io::Error> for ArtifactStoreError {
    fn from(error: std::io::Error) -> Self {
        ArtifactStoreError::Io(error.to_string())
    }
}

#[async_trait]
pub trait ArtifactContentStore: Send + Sync {
    async fn put(&self, content: Vec<u8>) -> Result<StoredArtifactContent, ArtifactStoreError>;

    async fn get(&self, sha256: &str) -> Result<Vec<u8>, ArtifactStoreError>;
}

#[async_trait]
pub trait ArtifactProjectionStore: Send + Sync {
    async fn apply(&self, event: &OperationEvent) -> Result<(), ArtifactServiceError>;
    async fn list_artifacts(&self) -> Result<Vec<ArtifactRecord>, ArtifactServiceError>;
    async fn get_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> Result<ArtifactRecord, ArtifactServiceError>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryArtifactContentStore {
    contents: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl InMemoryArtifactContentStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared(self) -> SharedArtifactContentStore {
        Arc::new(self)
    }
}

#[async_trait]
impl ArtifactContentStore for InMemoryArtifactContentStore {
    async fn put(&self, content: Vec<u8>) -> Result<StoredArtifactContent, ArtifactStoreError> {
        let sha256 = sha256_hex(&content);
        let byte_len = content.len() as u64;
        self.contents
            .write()
            .await
            .entry(sha256.clone())
            .or_insert(content);
        Ok(StoredArtifactContent { sha256, byte_len })
    }

    async fn get(&self, sha256: &str) -> Result<Vec<u8>, ArtifactStoreError> {
        self.contents
            .read()
            .await
            .get(sha256)
            .cloned()
            .ok_or_else(|| ArtifactStoreError::NotFound(sha256.to_string()))
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryArtifactProjectionStore {
    artifacts: Arc<RwLock<Vec<ArtifactRecord>>>,
}

impl InMemoryArtifactProjectionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared(self) -> SharedArtifactProjectionStore {
        Arc::new(self)
    }
}

#[async_trait]
impl ArtifactProjectionStore for InMemoryArtifactProjectionStore {
    async fn apply(&self, event: &OperationEvent) -> Result<(), ArtifactServiceError> {
        if let OperationEventKind::ArtifactCreated {
            artifact_id,
            title,
            kind,
            mime_type,
            sha256,
            byte_len,
        } = &event.kind
        {
            let mut artifacts = self.artifacts.write().await;
            let record = ArtifactRecord {
                artifact_id: *artifact_id,
                run_id: event.run_id,
                title: title.clone(),
                kind: *kind,
                mime_type: mime_type.clone(),
                sha256: sha256.clone(),
                byte_len: *byte_len,
                created_at: event.occurred_at,
            };

            if let Some(existing) = artifacts
                .iter_mut()
                .find(|artifact| artifact.artifact_id == *artifact_id)
            {
                *existing = record;
            } else {
                artifacts.push(record);
            }
            artifacts.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        }

        Ok(())
    }

    async fn list_artifacts(&self) -> Result<Vec<ArtifactRecord>, ArtifactServiceError> {
        Ok(self.artifacts.read().await.clone())
    }

    async fn get_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> Result<ArtifactRecord, ArtifactServiceError> {
        self.artifacts
            .read()
            .await
            .iter()
            .find(|artifact| artifact.artifact_id == artifact_id)
            .cloned()
            .ok_or(ArtifactServiceError::UnknownArtifact)
    }
}

#[derive(Clone)]
pub struct ComposedArtifactService {
    ledger: SharedOperationLedger,
    run_store: SharedRunProjectionStore,
    artifact_projection: SharedArtifactProjectionStore,
    artifact_store: SharedArtifactContentStore,
}

impl ComposedArtifactService {
    pub fn new(
        ledger: SharedOperationLedger,
        run_store: SharedRunProjectionStore,
        artifact_projection: SharedArtifactProjectionStore,
        artifact_store: SharedArtifactContentStore,
    ) -> Self {
        Self {
            ledger,
            run_store,
            artifact_projection,
            artifact_store,
        }
    }

    async fn ensure_known_run(&self, run_id: RunId) -> Result<(), ArtifactServiceError> {
        let runs = self
            .run_store
            .list_runs()
            .await
            .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?;
        if runs.iter().any(|run| run.run_id == run_id) {
            Ok(())
        } else {
            Err(ArtifactServiceError::UnknownRun)
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
        let title = non_blank(title)?;
        let mime_type = non_blank(mime_type)?;
        if content.trim().is_empty() {
            return Err(ArtifactServiceError::InvalidInput);
        }
        self.ensure_known_run(run_id).await?;

        let stored = self
            .artifact_store
            .put(content.into_bytes())
            .await
            .map_err(|error| ArtifactServiceError::ContentStore(error.to_string()))?;
        let artifact_id = ArtifactId::new();
        let event = OperationEvent::new_artifact_created(
            run_id,
            artifact_id,
            title,
            kind,
            mime_type,
            stored.sha256,
            stored.byte_len,
        );

        self.ledger
            .append(event.clone())
            .await
            .map_err(|error| ArtifactServiceError::Ledger(error.to_string()))?;
        self.run_store
            .apply(&event)
            .await
            .map_err(|error| ArtifactServiceError::Projection(error.to_string()))?;
        self.artifact_projection.apply(&event).await?;

        Ok(CreateArtifactOutcome { artifact_id })
    }

    pub async fn list_artifacts(&self) -> Result<Vec<ArtifactRecord>, ArtifactServiceError> {
        self.artifact_projection.list_artifacts().await
    }

    pub async fn get_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> Result<ArtifactDetail, ArtifactServiceError> {
        let artifact = self.artifact_projection.get_artifact(artifact_id).await?;
        let bytes = self
            .artifact_store
            .get(&artifact.sha256)
            .await
            .map_err(|error| ArtifactServiceError::ContentStore(error.to_string()))?;
        let content = String::from_utf8(bytes).ok();
        Ok(ArtifactDetail { artifact, content })
    }
}

#[derive(Debug, Clone)]
pub struct LocalArtifactContentStore {
    root: PathBuf,
}

impl LocalArtifactContentStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn shared(self) -> SharedArtifactContentStore {
        Arc::new(self)
    }

    fn content_path(&self, sha256: &str) -> PathBuf {
        self.root.join("sha256").join(sha256)
    }
}

#[async_trait]
impl ArtifactContentStore for LocalArtifactContentStore {
    async fn put(&self, content: Vec<u8>) -> Result<StoredArtifactContent, ArtifactStoreError> {
        let sha256 = sha256_hex(&content);
        let byte_len = content.len() as u64;
        let path = self.content_path(&sha256);
        tokio::fs::create_dir_all(path.parent().expect("content path has parent")).await?;
        if !tokio::fs::try_exists(&path).await? {
            tokio::fs::write(path, content).await?;
        }
        Ok(StoredArtifactContent { sha256, byte_len })
    }

    async fn get(&self, sha256: &str) -> Result<Vec<u8>, ArtifactStoreError> {
        match tokio::fs::read(self.content_path(sha256)).await {
            Ok(bytes) => Ok(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(ArtifactStoreError::NotFound(sha256.to_string()))
            }
            Err(error) => Err(error.into()),
        }
    }
}

fn non_blank(value: String) -> Result<String, ArtifactServiceError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(ArtifactServiceError::InvalidInput);
    }
    Ok(value)
}

fn sha256_hex(content: &[u8]) -> String {
    let digest = Sha256::digest(content);
    format!("{digest:x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArtifactId, ContextBundleId, EventId, RunId, RunService};

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

    #[tokio::test]
    async fn composed_artifact_service_creates_lists_and_gets_artifact() {
        let ledger = crate::InMemoryOperationLedger::new().shared();
        let run_store = crate::InMemoryRunProjectionStore::new().shared();
        let run_service = crate::ComposedRunService::new(ledger.clone(), run_store.clone());
        let run = run_service
            .create_run("write a summary".to_string())
            .await
            .unwrap();
        let artifact_store = InMemoryArtifactContentStore::new().shared();
        let artifact_projection = InMemoryArtifactProjectionStore::new().shared();
        let service =
            ComposedArtifactService::new(ledger, run_store, artifact_projection, artifact_store);

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
        let root =
            std::env::temp_dir().join(format!("mneme-artifact-store-{}", std::process::id()));
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
}
