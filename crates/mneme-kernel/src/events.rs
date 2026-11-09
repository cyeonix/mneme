use crate::ids::{EventId, RunId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// 第一版事件类型。
/// 先覆盖 run 生命周期，后续再扩展 tool、artifact、context、memory 等事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OperationEventKind {
    RunStarted { instruction: String },
    RunCompleted { summary: String },
    RunFailed { reason: String },
}

/// Operation Ledger 的最小事件结构。
/// 事件一旦写入就不应该原地修改，后续状态通过追加新事件表达。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationEvent {
    pub id: EventId,
    pub run_id: RunId,
    pub occurred_at: OffsetDateTime,
    pub kind: OperationEventKind,
}

impl OperationEvent {
    pub fn new_run_started(run_id: RunId, instruction: impl Into<String>) -> Self {
        Self {
            id: EventId::new(),
            run_id,
            occurred_at: OffsetDateTime::now_utc(),
            kind: OperationEventKind::RunStarted {
                instruction: instruction.into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_started_event_captures_instruction_and_run_id() {
        let run_id = RunId::new();
        let event = OperationEvent::new_run_started(run_id, "Draft the Mneme roadmap");

        assert_eq!(event.run_id, run_id);
        assert!(matches!(
            event.kind,
            OperationEventKind::RunStarted { ref instruction }
                if instruction == "Draft the Mneme roadmap"
        ));
    }
}
