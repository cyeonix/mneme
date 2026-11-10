use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use serde_json::Value;

use crate::{
    OperationEvent, OperationEventKind, RunId, RunProjection, SharedOperationLedger,
    SharedRunProjectionStore, ToolApprovalDecision, ToolApprovalPolicy, ToolCallId, ToolEffect,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallOutcome {
    pub run_id: RunId,
    pub tool_call_id: ToolCallId,
    pub approval: ToolApprovalDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolLifecycleOutcome {
    pub run_id: RunId,
    pub tool_call_id: ToolCallId,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RunServiceError {
    #[error("run instruction must not be blank")]
    InvalidInstruction,
    #[error("run id is not known to the service")]
    InvalidRunId,
    #[error("run lifecycle input must not be blank")]
    InvalidLifecycleInput,
    #[error("tool input must not be blank")]
    InvalidToolInput,
    #[error("run is already terminal")]
    RunAlreadyTerminal,
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
    async fn request_tool_call(
        &self,
        run_id: RunId,
        tool_name: String,
        effect: ToolEffect,
        input_json: Value,
    ) -> Result<ToolCallOutcome, RunServiceError>;
    async fn approve_tool_call(
        &self,
        run_id: RunId,
        tool_call_id: ToolCallId,
        approved_by: String,
    ) -> Result<ToolLifecycleOutcome, RunServiceError>;
    async fn deny_tool_call(
        &self,
        run_id: RunId,
        tool_call_id: ToolCallId,
        reason: String,
    ) -> Result<ToolLifecycleOutcome, RunServiceError>;
    async fn record_tool_result(
        &self,
        run_id: RunId,
        tool_call_id: ToolCallId,
        output_json: Value,
    ) -> Result<ToolLifecycleOutcome, RunServiceError>;
    async fn record_tool_failure(
        &self,
        run_id: RunId,
        tool_call_id: ToolCallId,
        reason: String,
    ) -> Result<ToolLifecycleOutcome, RunServiceError>;
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
        self.ensure_running_run(run_id).await?;

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

    async fn ensure_running_run(&self, run_id: RunId) -> Result<(), RunServiceError> {
        let runs = self
            .run_store
            .list_runs()
            .await
            .map_err(|error| RunServiceError::Projection(error.to_string()))?;
        let run = runs
            .iter()
            .find(|run| run.run_id == run_id)
            .ok_or(RunServiceError::InvalidRunId)?;
        if run.status != crate::RunStatus::Running {
            return Err(RunServiceError::RunAlreadyTerminal);
        }
        Ok(())
    }

    async fn append_and_apply_tool_event(
        &self,
        event: OperationEvent,
    ) -> Result<ToolLifecycleOutcome, RunServiceError> {
        let run_id = event.run_id;
        let tool_call_id =
            tool_call_id_for_event(&event).ok_or(RunServiceError::InvalidToolInput)?;
        self.ensure_running_run(run_id).await?;
        self.ledger
            .append(event.clone())
            .await
            .map_err(|error| RunServiceError::Ledger(error.to_string()))?;
        self.run_store
            .apply(&event)
            .await
            .map_err(|error| RunServiceError::Projection(error.to_string()))?;
        Ok(ToolLifecycleOutcome {
            run_id,
            tool_call_id,
        })
    }

    fn non_blank_lifecycle_input(value: String) -> Result<String, RunServiceError> {
        let value = value.trim().to_string();
        if value.is_empty() {
            return Err(RunServiceError::InvalidLifecycleInput);
        }
        Ok(value)
    }

    fn non_blank_tool_text(value: String) -> Result<String, RunServiceError> {
        let value = value.trim().to_string();
        if value.is_empty() {
            return Err(RunServiceError::InvalidToolInput);
        }
        Ok(value)
    }
}

fn tool_call_id_for_event(event: &OperationEvent) -> Option<ToolCallId> {
    match &event.kind {
        OperationEventKind::ToolCallRequested { tool_call_id, .. }
        | OperationEventKind::ToolCallApproved { tool_call_id, .. }
        | OperationEventKind::ToolCallDenied { tool_call_id, .. }
        | OperationEventKind::ToolCallCompleted { tool_call_id, .. }
        | OperationEventKind::ToolCallFailed { tool_call_id, .. } => Some(*tool_call_id),
        _ => None,
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

    async fn request_tool_call(
        &self,
        run_id: RunId,
        tool_name: String,
        effect: ToolEffect,
        input_json: Value,
    ) -> Result<ToolCallOutcome, RunServiceError> {
        let tool_name = Self::non_blank_tool_text(tool_name)?;
        self.ensure_running_run(run_id).await?;
        let tool_call_id = ToolCallId::new();
        let requested = OperationEvent::new_tool_call_requested(
            run_id,
            tool_call_id,
            tool_name,
            effect.clone(),
            input_json,
        );
        self.ledger
            .append(requested.clone())
            .await
            .map_err(|error| RunServiceError::Ledger(error.to_string()))?;
        self.run_store
            .apply(&requested)
            .await
            .map_err(|error| RunServiceError::Projection(error.to_string()))?;

        let approval = ToolApprovalPolicy::default().decision_for(&effect);
        if approval == ToolApprovalDecision::AutoApproved {
            let approved =
                OperationEvent::new_tool_call_approved(run_id, tool_call_id, "mneme-policy");
            self.ledger
                .append(approved.clone())
                .await
                .map_err(|error| RunServiceError::Ledger(error.to_string()))?;
            self.run_store
                .apply(&approved)
                .await
                .map_err(|error| RunServiceError::Projection(error.to_string()))?;
        }

        Ok(ToolCallOutcome {
            run_id,
            tool_call_id,
            approval,
        })
    }

    async fn approve_tool_call(
        &self,
        run_id: RunId,
        tool_call_id: ToolCallId,
        approved_by: String,
    ) -> Result<ToolLifecycleOutcome, RunServiceError> {
        let approved_by = Self::non_blank_tool_text(approved_by)?;
        self.append_and_apply_tool_event(OperationEvent::new_tool_call_approved(
            run_id,
            tool_call_id,
            approved_by,
        ))
        .await
    }

    async fn deny_tool_call(
        &self,
        run_id: RunId,
        tool_call_id: ToolCallId,
        reason: String,
    ) -> Result<ToolLifecycleOutcome, RunServiceError> {
        let reason = Self::non_blank_tool_text(reason)?;
        self.append_and_apply_tool_event(OperationEvent::new_tool_call_denied(
            run_id,
            tool_call_id,
            reason,
        ))
        .await
    }

    async fn record_tool_result(
        &self,
        run_id: RunId,
        tool_call_id: ToolCallId,
        output_json: Value,
    ) -> Result<ToolLifecycleOutcome, RunServiceError> {
        self.append_and_apply_tool_event(OperationEvent::new_tool_call_completed(
            run_id,
            tool_call_id,
            output_json,
        ))
        .await
    }

    async fn record_tool_failure(
        &self,
        run_id: RunId,
        tool_call_id: ToolCallId,
        reason: String,
    ) -> Result<ToolLifecycleOutcome, RunServiceError> {
        let reason = Self::non_blank_tool_text(reason)?;
        self.append_and_apply_tool_event(OperationEvent::new_tool_call_failed(
            run_id,
            tool_call_id,
            reason,
        ))
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
        InMemoryOperationLedger, InMemoryRunProjectionStore, OperationEventKind, OperationLedger,
        RunProjectionStore, RunStatus, ToolApprovalDecision, ToolEffect,
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
    async fn composed_service_records_read_only_tool_request_as_auto_approved() {
        let ledger = InMemoryOperationLedger::new();
        let run_store = InMemoryRunProjectionStore::new();
        let service = ComposedRunService::new(ledger.clone().shared(), run_store.clone().shared());
        let created = service.create_run("读取文件".to_string()).await.unwrap();

        let outcome = service
            .request_tool_call(
                created.run_id,
                "read_file".to_string(),
                ToolEffect::ReadOnly,
                serde_json::json!({ "path": "README.md" }),
            )
            .await
            .unwrap();

        let events = ledger.list().await.unwrap();
        assert_eq!(outcome.run_id, created.run_id);
        assert_eq!(outcome.approval, ToolApprovalDecision::AutoApproved);
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events[1].kind,
            OperationEventKind::ToolCallRequested { .. }
        ));
        assert!(matches!(
            events[2].kind,
            OperationEventKind::ToolCallApproved { .. }
        ));
    }

    #[tokio::test]
    async fn composed_service_records_side_effect_tool_request_as_requires_approval() {
        let ledger = InMemoryOperationLedger::new();
        let run_store = InMemoryRunProjectionStore::new();
        let service = ComposedRunService::new(ledger.clone().shared(), run_store.clone().shared());
        let created = service.create_run("写文件".to_string()).await.unwrap();

        let outcome = service
            .request_tool_call(
                created.run_id,
                "write_file".to_string(),
                ToolEffect::LocalWrite,
                serde_json::json!({ "path": "README.md", "content": "hello" }),
            )
            .await
            .unwrap();

        assert_eq!(outcome.approval, ToolApprovalDecision::RequiresApproval);
        assert_eq!(ledger.list().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn composed_service_rejects_tool_request_after_run_completed() {
        let service = ComposedRunService::new(
            InMemoryOperationLedger::new().shared(),
            InMemoryRunProjectionStore::new().shared(),
        );
        let created = service
            .create_run("完成后不能请求工具".to_string())
            .await
            .unwrap();
        service
            .complete_run(created.run_id, "完成".to_string())
            .await
            .unwrap();

        let error = service
            .request_tool_call(
                created.run_id,
                "read_file".to_string(),
                ToolEffect::ReadOnly,
                serde_json::json!({ "path": "README.md" }),
            )
            .await
            .unwrap_err();

        assert_eq!(error, RunServiceError::RunAlreadyTerminal);
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

    #[tokio::test]
    async fn composed_service_rejects_post_completed_lifecycle_without_side_effects() {
        let ledger = InMemoryOperationLedger::new();
        let run_store = InMemoryRunProjectionStore::new();
        let service = ComposedRunService::new(ledger.clone().shared(), run_store.shared());

        let created = service.create_run("整理今天".to_string()).await.unwrap();
        service
            .complete_run(created.run_id, "总结完成".to_string())
            .await
            .unwrap();
        let events_before = service.list_events().await.unwrap().len();

        let error = service
            .request_model(created.run_id, "不应恢复运行".to_string())
            .await
            .unwrap_err();

        assert_eq!(error, RunServiceError::RunAlreadyTerminal);
        assert_eq!(service.list_events().await.unwrap().len(), events_before);
        let runs = service.list_runs().await.unwrap();
        assert_eq!(runs[0].status, RunStatus::Completed);
        assert_eq!(runs[0].summary.as_deref(), Some("总结完成"));
    }

    #[tokio::test]
    async fn composed_service_rejects_post_failed_lifecycle_without_side_effects() {
        let ledger = InMemoryOperationLedger::new();
        let run_store = InMemoryRunProjectionStore::new();
        let service = ComposedRunService::new(ledger.clone().shared(), run_store.shared());

        let created = service.create_run("整理今天".to_string()).await.unwrap();
        service
            .fail_run(created.run_id, "模型不可用".to_string())
            .await
            .unwrap();
        let events_before = service.list_events().await.unwrap().len();

        let error = service
            .record_assistant_message(created.run_id, "不应恢复运行".to_string())
            .await
            .unwrap_err();

        assert_eq!(error, RunServiceError::RunAlreadyTerminal);
        assert_eq!(service.list_events().await.unwrap().len(), events_before);
        let runs = service.list_runs().await.unwrap();
        assert_eq!(runs[0].status, RunStatus::Failed);
        assert_eq!(runs[0].error.as_deref(), Some("模型不可用"));
    }
}
