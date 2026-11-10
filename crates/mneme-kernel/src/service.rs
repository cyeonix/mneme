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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunLifecycleOutcome {
    pub run_id: RunId,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RunServiceError {
    #[error("run instruction must not be blank")]
    InvalidInstruction,
    #[error("run id is not known to the service")]
    InvalidRunId,
    #[error("run lifecycle input must not be blank")]
    InvalidLifecycleInput,
    #[error("operation ledger failed: {0}")]
    Ledger(String),
    #[error("run projection failed: {0}")]
    Projection(String),
}

#[async_trait]
pub trait RunService: Send + Sync {
    async fn create_run(&self, instruction: String) -> Result<CreateRunOutcome, RunServiceError>;
    async fn request_model(
        &self,
        run_id: RunId,
        prompt: String,
    ) -> Result<RunLifecycleOutcome, RunServiceError>;
    async fn record_assistant_message(
        &self,
        run_id: RunId,
        content: String,
    ) -> Result<RunLifecycleOutcome, RunServiceError>;
    async fn complete_run(
        &self,
        run_id: RunId,
        summary: String,
    ) -> Result<RunLifecycleOutcome, RunServiceError>;
    async fn fail_run(
        &self,
        run_id: RunId,
        reason: String,
    ) -> Result<RunLifecycleOutcome, RunServiceError>;
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

    async fn append_and_apply_lifecycle_event(
        &self,
        event: OperationEvent,
    ) -> Result<RunLifecycleOutcome, RunServiceError> {
        let run_id = event.run_id;
        if !self
            .run_store
            .list_runs()
            .await
            .map_err(|error| RunServiceError::Projection(error.to_string()))?
            .iter()
            .any(|run| run.run_id == run_id)
        {
            return Err(RunServiceError::InvalidRunId);
        }

        self.ledger
            .append(event.clone())
            .await
            .map_err(|error| RunServiceError::Ledger(error.to_string()))?;
        self.run_store
            .apply(&event)
            .await
            .map_err(|error| RunServiceError::Projection(error.to_string()))?;
        Ok(RunLifecycleOutcome { run_id })
    }

    fn non_blank_lifecycle_input(value: String) -> Result<String, RunServiceError> {
        let value = value.trim().to_string();
        if value.is_empty() {
            return Err(RunServiceError::InvalidLifecycleInput);
        }
        Ok(value)
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

    async fn request_model(
        &self,
        run_id: RunId,
        prompt: String,
    ) -> Result<RunLifecycleOutcome, RunServiceError> {
        let prompt = Self::non_blank_lifecycle_input(prompt)?;
        self.append_and_apply_lifecycle_event(OperationEvent::new_run_model_requested(
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
        self.append_and_apply_lifecycle_event(OperationEvent::new_run_assistant_message(
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
        self.append_and_apply_lifecycle_event(OperationEvent::new_run_completed(run_id, summary))
            .await
    }

    async fn fail_run(
        &self,
        run_id: RunId,
        reason: String,
    ) -> Result<RunLifecycleOutcome, RunServiceError> {
        let reason = Self::non_blank_lifecycle_input(reason)?;
        self.append_and_apply_lifecycle_event(OperationEvent::new_run_failed(run_id, reason))
            .await
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
        RunStatus,
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

    #[tokio::test]
    async fn composed_service_records_full_success_lifecycle() {
        let ledger = InMemoryOperationLedger::new();
        let run_store = InMemoryRunProjectionStore::new();
        let service = ComposedRunService::new(ledger.clone().shared(), run_store.clone().shared());

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
        assert_eq!(runs[0].run_id, created.run_id);
        assert_eq!(runs[0].status, RunStatus::Completed);
        assert_eq!(runs[0].summary.as_deref(), Some("总结完成"));
    }

    #[tokio::test]
    async fn composed_service_rejects_blank_lifecycle_input() {
        let service = ComposedRunService::new(
            InMemoryOperationLedger::new().shared(),
            InMemoryRunProjectionStore::new().shared(),
        );
        let run_id = RunId::new();

        let error = service
            .request_model(run_id, "   ".to_string())
            .await
            .unwrap_err();

        assert_eq!(error, RunServiceError::InvalidLifecycleInput);
    }

    #[tokio::test]
    async fn composed_service_rejects_unknown_lifecycle_run_id() {
        let service = ComposedRunService::new(
            InMemoryOperationLedger::new().shared(),
            InMemoryRunProjectionStore::new().shared(),
        );

        let error = service
            .complete_run(RunId::new(), "没有对应 run".to_string())
            .await
            .unwrap_err();

        assert_eq!(error, RunServiceError::InvalidRunId);
        assert!(service.list_events().await.unwrap().is_empty());
    }
}
