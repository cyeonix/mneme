use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use crate::{
    OperationEvent, RunId, RunProjection, SharedOperationLedger, SharedRunProjectionStore,
};

pub type SharedRunService = Arc<dyn RunService>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateRunOutcome {
    pub run_id: RunId,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RunServiceError {
    #[error("run instruction must not be blank")]
    InvalidInstruction,
    #[error("operation ledger failed: {0}")]
    Ledger(String),
    #[error("run projection failed: {0}")]
    Projection(String),
}

#[async_trait]
pub trait RunService: Send + Sync {
    async fn create_run(&self, instruction: String) -> Result<CreateRunOutcome, RunServiceError>;
    async fn list_events(&self) -> Result<Vec<OperationEvent>, RunServiceError>;
    async fn list_runs(&self) -> Result<Vec<RunProjection>, RunServiceError>;
    async fn rebuild_run_projections(&self) -> Result<Vec<RunProjection>, RunServiceError>;
}

#[derive(Clone)]
pub struct ComposedRunService {
    ledger: SharedOperationLedger,
    run_store: SharedRunProjectionStore,
}

impl ComposedRunService {
    pub fn new(ledger: SharedOperationLedger, run_store: SharedRunProjectionStore) -> Self {
        Self { ledger, run_store }
    }

    pub fn shared(self) -> SharedRunService {
        Arc::new(self)
    }
}

#[async_trait]
impl RunService for ComposedRunService {
    async fn create_run(&self, instruction: String) -> Result<CreateRunOutcome, RunServiceError> {
        let instruction = instruction.trim().to_string();
        if instruction.is_empty() {
            return Err(RunServiceError::InvalidInstruction);
        }

        let run_id = RunId::new();
        let event = OperationEvent::new_run_started(run_id, instruction);
        self.ledger
            .append(event.clone())
            .await
            .map_err(|error| RunServiceError::Ledger(error.to_string()))?;
        self.run_store
            .apply(&event)
            .await
            .map_err(|error| RunServiceError::Projection(error.to_string()))?;

        Ok(CreateRunOutcome { run_id })
    }

    async fn list_events(&self) -> Result<Vec<OperationEvent>, RunServiceError> {
        self.ledger
            .list()
            .await
            .map_err(|error| RunServiceError::Ledger(error.to_string()))
    }

    async fn list_runs(&self) -> Result<Vec<RunProjection>, RunServiceError> {
        self.run_store
            .list_runs()
            .await
            .map_err(|error| RunServiceError::Projection(error.to_string()))
    }

    async fn rebuild_run_projections(&self) -> Result<Vec<RunProjection>, RunServiceError> {
        let events = self.list_events().await?;
        self.run_store
            .rebuild(&events)
            .await
            .map_err(|error| RunServiceError::Projection(error.to_string()))?;
        self.list_runs().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        InMemoryOperationLedger, InMemoryRunProjectionStore, OperationLedger, RunProjectionStore,
    };

    #[tokio::test]
    async fn composed_service_creates_run_event_and_projection() {
        let ledger = InMemoryOperationLedger::new();
        let run_store = InMemoryRunProjectionStore::new();
        let service = ComposedRunService::new(ledger.clone().shared(), run_store.clone().shared());

        let outcome = service
            .create_run("整理服务边界".to_string())
            .await
            .unwrap();

        assert_eq!(ledger.list().await.unwrap().len(), 1);
        assert_eq!(run_store.list_runs().await.unwrap().len(), 1);
        assert_eq!(
            outcome.run_id,
            run_store.list_runs().await.unwrap()[0].run_id
        );
    }

    #[tokio::test]
    async fn composed_service_rejects_blank_instruction() {
        let service = ComposedRunService::new(
            InMemoryOperationLedger::new().shared(),
            InMemoryRunProjectionStore::new().shared(),
        );

        let error = service.create_run("   ".to_string()).await.unwrap_err();

        assert_eq!(error, RunServiceError::InvalidInstruction);
    }

    #[tokio::test]
    async fn composed_service_rebuilds_run_projection_from_events() {
        let ledger = InMemoryOperationLedger::new();
        let run_store = InMemoryRunProjectionStore::new();
        let service = ComposedRunService::new(ledger.clone().shared(), run_store.clone().shared());

        let outcome = service
            .create_run("需要重建的 run".to_string())
            .await
            .unwrap();
        run_store.rebuild(&[]).await.unwrap();
        assert!(run_store.list_runs().await.unwrap().is_empty());

        let rebuilt = service.rebuild_run_projections().await.unwrap();

        assert_eq!(rebuilt.len(), 1);
        assert_eq!(rebuilt[0].run_id, outcome.run_id);
    }
}
