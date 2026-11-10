use crate::{OperationEvent, OperationEventKind, RunProjection, RunProjector, RunStatus};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub type SharedRunProjectionStore = Arc<dyn RunProjectionStore>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RunProjectionError {
    #[error("run projection store lock is poisoned")]
    Poisoned,
    #[error("run projection storage failed: {0}")]
    Storage(String),
    #[error("run projection payload is invalid: {0}")]
    InvalidPayload(String),
}

#[async_trait]
pub trait RunProjectionStore: Send + Sync {
    async fn apply(&self, event: &OperationEvent) -> Result<(), RunProjectionError>;
    async fn rebuild(&self, events: &[OperationEvent]) -> Result<(), RunProjectionError>;
    async fn list_runs(&self) -> Result<Vec<RunProjection>, RunProjectionError>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryRunProjectionStore {
    runs: Arc<Mutex<Vec<RunProjection>>>,
}

impl InMemoryRunProjectionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared(self) -> SharedRunProjectionStore {
        Arc::new(self)
    }
}

#[async_trait]
impl RunProjectionStore for InMemoryRunProjectionStore {
    async fn apply(&self, event: &OperationEvent) -> Result<(), RunProjectionError> {
        let mut runs = self.runs.lock().map_err(|_| RunProjectionError::Poisoned)?;

        match &event.kind {
            OperationEventKind::RunStarted { instruction } => {
                if let Some(run) = runs.iter_mut().find(|run| run.run_id == event.run_id) {
                    run.status = RunStatus::Running;
                    run.instruction = instruction.clone();
                    run.started_at = event.occurred_at;
                    run.completed_at = None;
                    run.failed_at = None;
                    run.summary = None;
                    run.error = None;
                    run.updated_at = event.occurred_at;
                } else {
                    runs.push(RunProjection {
                        run_id: event.run_id,
                        status: RunStatus::Running,
                        instruction: instruction.clone(),
                        started_at: event.occurred_at,
                        completed_at: None,
                        failed_at: None,
                        summary: None,
                        error: None,
                        updated_at: event.occurred_at,
                    });
                }
            }
            OperationEventKind::RunModelRequested { .. }
            | OperationEventKind::RunAssistantMessage { .. }
            | OperationEventKind::ToolCallRequested { .. }
            | OperationEventKind::ToolCallApproved { .. }
            | OperationEventKind::ToolCallDenied { .. }
            | OperationEventKind::ToolCallCompleted { .. }
            | OperationEventKind::ToolCallFailed { .. }
            | OperationEventKind::ArtifactCreated { .. }
            | OperationEventKind::ContextBundleCreated { .. } => {
                if let Some(run) = runs
                    .iter_mut()
                    .find(|run| run.run_id == event.run_id && run.status == RunStatus::Running)
                {
                    run.updated_at = event.occurred_at;
                }
            }
            OperationEventKind::RunCompleted { summary } => {
                if let Some(run) = runs.iter_mut().find(|run| run.run_id == event.run_id) {
                    run.status = RunStatus::Completed;
                    run.completed_at = Some(event.occurred_at);
                    run.summary = Some(summary.clone());
                    run.updated_at = event.occurred_at;
                }
            }
            OperationEventKind::RunFailed { reason } => {
                if let Some(run) = runs.iter_mut().find(|run| run.run_id == event.run_id) {
                    run.status = RunStatus::Failed;
                    run.failed_at = Some(event.occurred_at);
                    run.error = Some(reason.clone());
                    run.updated_at = event.occurred_at;
                }
            }
        }

        runs.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(())
    }

    async fn rebuild(&self, events: &[OperationEvent]) -> Result<(), RunProjectionError> {
        let mut runs = self.runs.lock().map_err(|_| RunProjectionError::Poisoned)?;
        *runs = RunProjector::replay(events);
        Ok(())
    }

    async fn list_runs(&self) -> Result<Vec<RunProjection>, RunProjectionError> {
        let runs = self.runs.lock().map_err(|_| RunProjectionError::Poisoned)?;
        Ok(runs.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OperationEvent, RunId, ToolCallId, ToolEffect};

    #[tokio::test]
    async fn in_memory_run_store_applies_started_event() {
        let store = InMemoryRunProjectionStore::new();
        let run_id = RunId::new();
        let event = OperationEvent::new_run_started(run_id, "记录一次 run");

        store.apply(&event).await.unwrap();

        let runs = store.list_runs().await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id, run_id);
        assert_eq!(runs[0].status, RunStatus::Running);
    }

    #[tokio::test]
    async fn in_memory_run_store_matches_projector_replay_for_terminal_events() {
        let store = InMemoryRunProjectionStore::new();
        let run_id = RunId::new();
        let unknown_run = RunId::new();
        let events = vec![
            OperationEvent::new_run_failed(unknown_run, "忽略未知 run"),
            OperationEvent::new_run_started(run_id, "执行任务"),
            OperationEvent::new_run_completed(run_id, "完成任务"),
        ];

        for event in &events {
            store.apply(event).await.unwrap();
        }

        assert_eq!(
            store.list_runs().await.unwrap(),
            RunProjector::replay(&events)
        );
    }

    #[tokio::test]
    async fn in_memory_run_store_keeps_completed_run_terminal_after_late_intermediate_event() {
        let store = InMemoryRunProjectionStore::new();
        let run_id = RunId::new();
        let tool_call_id = ToolCallId::new();
        let started = OperationEvent::new_run_started(run_id, "执行任务");
        let completed = OperationEvent::new_run_completed(run_id, "完成任务");
        let late_model_request = OperationEvent::new_run_model_requested(run_id, "不应恢复运行");
        let late_tool_request = OperationEvent::new_tool_call_requested(
            run_id,
            tool_call_id,
            "read_file",
            ToolEffect::ReadOnly,
            serde_json::json!({ "path": "README.md" }),
        );
        let late_tool_completion = OperationEvent::new_tool_call_completed(
            run_id,
            tool_call_id,
            serde_json::json!({ "content": "不应覆盖终态" }),
        );

        store.apply(&started).await.unwrap();
        store.apply(&completed).await.unwrap();
        store.apply(&late_model_request).await.unwrap();
        store.apply(&late_tool_request).await.unwrap();
        store.apply(&late_tool_completion).await.unwrap();

        let runs = store.list_runs().await.unwrap();
        assert_eq!(runs[0].status, RunStatus::Completed);
        assert_eq!(runs[0].summary.as_deref(), Some("完成任务"));
        assert_eq!(runs[0].completed_at, Some(completed.occurred_at));
        assert_eq!(runs[0].updated_at, completed.occurred_at);
    }
}
