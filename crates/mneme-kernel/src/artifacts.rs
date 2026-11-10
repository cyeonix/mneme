use crate::{ArtifactId, ContextBundleId, EventId, RunId};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use thiserror::Error;
use time::OffsetDateTime;
use tokio::sync::RwLock;

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

#[derive(Debug, Clone, Default)]
pub struct InMemoryArtifactContentStore {
    contents: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl InMemoryArtifactContentStore {
    pub fn new() -> Self {
        Self::default()
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

#[derive(Debug, Clone)]
pub struct LocalArtifactContentStore {
    root: PathBuf,
}

impl LocalArtifactContentStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
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

fn sha256_hex(content: &[u8]) -> String {
    let digest = Sha256::digest(content);
    format!("{digest:x}")
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
