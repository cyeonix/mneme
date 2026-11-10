use crate::{
    CreateRunOutcome, EventId, LedgerError, OperationEvent, OperationEventKind, OperationLedger,
    RunId, RunProjection, RunProjectionError, RunProjectionStore, RunProjector, RunService,
    RunServiceError, RunStatus, SharedOperationLedger, SharedRunProjectionStore, SharedRunService,
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

fn status_to_storage(status: &RunStatus) -> &'static str {
    match status {
        RunStatus::Running => "running",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
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
    use crate::{OperationEvent, OperationLedger, RunId, RunProjectionStore, RunProjector};

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
