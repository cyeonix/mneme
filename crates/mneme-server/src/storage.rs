use mneme_kernel::SyncObjectKind;

pub trait SyncServerStore: Clone + Send + Sync + 'static {
    fn pool(&self) -> &sqlx::SqlitePool;
}

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

    pub async fn has_device(&self, device_id: &str) -> Result<bool, sqlx::Error> {
        let count =
            sqlx::query_scalar::<_, i64>("select count(1) from devices where device_id = ?1")
                .bind(device_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(count > 0)
    }

    pub async fn latest_server_sequence(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, Option<i64>>("select max(server_sequence) from synced_events")
            .fetch_one(&self.pool)
            .await
            .map(|value| value.unwrap_or(0))
    }

    pub fn object_kind_to_text(object_kind: SyncObjectKind) -> &'static str {
        match object_kind {
            SyncObjectKind::Run => "run",
            SyncObjectKind::ToolCall => "tool_call",
            SyncObjectKind::Artifact => "artifact",
            SyncObjectKind::ContextBundle => "context_bundle",
            SyncObjectKind::KnowledgeSource => "knowledge_source",
            SyncObjectKind::DocumentChunk => "document_chunk",
            SyncObjectKind::Memory => "memory",
            SyncObjectKind::RetrievalTrace => "retrieval_trace",
            SyncObjectKind::WorkerJob => "worker_job",
        }
    }

    pub fn parse_object_kind(value: &str) -> Option<SyncObjectKind> {
        match value {
            "run" => Some(SyncObjectKind::Run),
            "tool_call" => Some(SyncObjectKind::ToolCall),
            "artifact" => Some(SyncObjectKind::Artifact),
            "context_bundle" => Some(SyncObjectKind::ContextBundle),
            "knowledge_source" => Some(SyncObjectKind::KnowledgeSource),
            "document_chunk" => Some(SyncObjectKind::DocumentChunk),
            "memory" => Some(SyncObjectKind::Memory),
            "retrieval_trace" => Some(SyncObjectKind::RetrievalTrace),
            "worker_job" => Some(SyncObjectKind::WorkerJob),
            _ => None,
        }
    }
}

impl SyncServerStore for SqliteSyncServerStore {
    fn pool(&self) -> &sqlx::SqlitePool {
        self.pool()
    }
}
