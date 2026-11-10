use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{OperationEvent, OperationEventKind, RunId};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunProjection {
    pub run_id: RunId,
    pub status: RunStatus,
    pub instruction: String,
    pub started_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
    pub failed_at: Option<OffsetDateTime>,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub updated_at: OffsetDateTime,
}

pub struct RunProjector;

impl RunProjector {
    pub fn replay(events: &[OperationEvent]) -> Vec<RunProjection> {
        let mut runs = Vec::<RunProjection>::new();

        for event in events {
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
                | OperationEventKind::RunAssistantMessage { .. } => {
                    if let Some(run) = runs.iter_mut().find(|run| run.run_id == event.run_id) {
                        run.status = RunStatus::Running;
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
        }

        runs.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        runs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OperationEvent, RunId};

    #[test]
    fn replay_builds_completed_run_projection() {
        let run_id = RunId::new();
        let events = vec![
            OperationEvent::new_run_started(run_id, "生成第一份计划"),
            OperationEvent::new_run_completed(run_id, "计划已生成"),
        ];

        let runs = RunProjector::replay(&events);

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id, run_id);
        assert_eq!(runs[0].status, RunStatus::Completed);
        assert_eq!(runs[0].instruction, "生成第一份计划");
        assert_eq!(runs[0].summary.as_deref(), Some("计划已生成"));
    }

    #[test]
    fn replay_is_deterministic_for_same_event_stream() {
        let run_id = RunId::new();
        let events = vec![OperationEvent::new_run_started(run_id, "稳定 replay")];

        assert_eq!(RunProjector::replay(&events), RunProjector::replay(&events));
    }

    #[test]
    fn replay_treats_duplicate_started_events_as_one_run_projection() {
        let run_id = RunId::new();
        let events = vec![
            OperationEvent::new_run_started(run_id, "第一次启动"),
            OperationEvent::new_run_started(run_id, "第二次启动"),
        ];

        let runs = RunProjector::replay(&events);

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id, run_id);
        assert_eq!(runs[0].instruction, "第二次启动");
        assert_eq!(runs[0].status, RunStatus::Running);
    }

    #[test]
    fn replay_keeps_run_running_after_model_and_assistant_events() {
        let run_id = RunId::new();
        let events = vec![
            OperationEvent::new_run_started(run_id, "整理今天的工作"),
            OperationEvent::new_run_model_requested(run_id, "请总结今天的工作"),
            OperationEvent::new_run_assistant_message(run_id, "今天完成了服务边界收敛。"),
        ];

        let runs = RunProjector::replay(&events);

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id, run_id);
        assert_eq!(runs[0].status, RunStatus::Running);
        assert_eq!(runs[0].instruction, "整理今天的工作");
        assert_eq!(runs[0].updated_at, events[2].occurred_at);
    }
}
