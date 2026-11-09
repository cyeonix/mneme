use crate::{
    EventId, LedgerError, OperationEvent, OperationEventKind, OperationLedger, RunId,
    SharedOperationLedger,
};
use async_trait::async_trait;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
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
        let kind_type = match &event.kind {
            OperationEventKind::RunStarted { .. } => "run_started",
            OperationEventKind::RunCompleted { .. } => "run_completed",
            OperationEventKind::RunFailed { .. } => "run_failed",
        };
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
    use crate::{OperationEvent, OperationLedger, RunId};

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
}
