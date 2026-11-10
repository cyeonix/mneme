use crate::ids::{ArtifactId, ContextBundleId, EventId, RunId, ToolCallId};
use crate::{ArtifactKind, ToolEffect};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

/// 第一版事件类型。
/// 先覆盖 run 生命周期，后续再扩展 tool、artifact、context、memory 等事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OperationEventKind {
    RunStarted {
        instruction: String,
    },
    RunModelRequested {
        prompt: String,
    },
    RunAssistantMessage {
        content: String,
    },
    RunCompleted {
        summary: String,
    },
    RunFailed {
        reason: String,
    },
    ToolCallRequested {
        tool_call_id: ToolCallId,
        tool_name: String,
        effect: ToolEffect,
        input_json: Value,
    },
    ToolCallApproved {
        tool_call_id: ToolCallId,
        approved_by: String,
    },
    ToolCallDenied {
        tool_call_id: ToolCallId,
        reason: String,
    },
    ToolCallCompleted {
        tool_call_id: ToolCallId,
        output_json: Value,
    },
    ToolCallFailed {
        tool_call_id: ToolCallId,
        reason: String,
    },
    ArtifactCreated {
        artifact_id: ArtifactId,
        title: String,
        kind: ArtifactKind,
        mime_type: String,
        sha256: String,
        byte_len: u64,
    },
    ContextBundleCreated {
        context_bundle_id: ContextBundleId,
        title: String,
        summary: String,
        artifact_ids: Vec<ArtifactId>,
        event_ids: Vec<EventId>,
    },
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

    pub fn new_run_model_requested(run_id: RunId, prompt: impl Into<String>) -> Self {
        Self {
            id: EventId::new(),
            run_id,
            occurred_at: OffsetDateTime::now_utc(),
            kind: OperationEventKind::RunModelRequested {
                prompt: prompt.into(),
            },
        }
    }

    pub fn new_run_assistant_message(run_id: RunId, content: impl Into<String>) -> Self {
        Self {
            id: EventId::new(),
            run_id,
            occurred_at: OffsetDateTime::now_utc(),
            kind: OperationEventKind::RunAssistantMessage {
                content: content.into(),
            },
        }
    }

    pub fn new_run_completed(run_id: RunId, summary: impl Into<String>) -> Self {
        Self {
            id: EventId::new(),
            run_id,
            occurred_at: OffsetDateTime::now_utc(),
            kind: OperationEventKind::RunCompleted {
                summary: summary.into(),
            },
        }
    }

    pub fn new_run_failed(run_id: RunId, reason: impl Into<String>) -> Self {
        Self {
            id: EventId::new(),
            run_id,
            occurred_at: OffsetDateTime::now_utc(),
            kind: OperationEventKind::RunFailed {
                reason: reason.into(),
            },
        }
    }

    pub fn new_tool_call_requested(
        run_id: RunId,
        tool_call_id: ToolCallId,
        tool_name: impl Into<String>,
        effect: ToolEffect,
        input_json: Value,
    ) -> Self {
        Self {
            id: EventId::new(),
            run_id,
            occurred_at: OffsetDateTime::now_utc(),
            kind: OperationEventKind::ToolCallRequested {
                tool_call_id,
                tool_name: tool_name.into(),
                effect,
                input_json,
            },
        }
    }

    pub fn new_tool_call_approved(
        run_id: RunId,
        tool_call_id: ToolCallId,
        approved_by: impl Into<String>,
    ) -> Self {
        Self {
            id: EventId::new(),
            run_id,
            occurred_at: OffsetDateTime::now_utc(),
            kind: OperationEventKind::ToolCallApproved {
                tool_call_id,
                approved_by: approved_by.into(),
            },
        }
    }

    pub fn new_tool_call_denied(
        run_id: RunId,
        tool_call_id: ToolCallId,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            id: EventId::new(),
            run_id,
            occurred_at: OffsetDateTime::now_utc(),
            kind: OperationEventKind::ToolCallDenied {
                tool_call_id,
                reason: reason.into(),
            },
        }
    }

    pub fn new_tool_call_completed(
        run_id: RunId,
        tool_call_id: ToolCallId,
        output_json: Value,
    ) -> Self {
        Self {
            id: EventId::new(),
            run_id,
            occurred_at: OffsetDateTime::now_utc(),
            kind: OperationEventKind::ToolCallCompleted {
                tool_call_id,
                output_json,
            },
        }
    }

    pub fn new_tool_call_failed(
        run_id: RunId,
        tool_call_id: ToolCallId,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            id: EventId::new(),
            run_id,
            occurred_at: OffsetDateTime::now_utc(),
            kind: OperationEventKind::ToolCallFailed {
                tool_call_id,
                reason: reason.into(),
            },
        }
    }

    pub fn new_artifact_created(
        run_id: RunId,
        artifact_id: ArtifactId,
        title: impl Into<String>,
        kind: ArtifactKind,
        mime_type: impl Into<String>,
        sha256: impl Into<String>,
        byte_len: u64,
    ) -> Self {
        Self {
            id: EventId::new(),
            run_id,
            occurred_at: OffsetDateTime::now_utc(),
            kind: OperationEventKind::ArtifactCreated {
                artifact_id,
                title: title.into(),
                kind,
                mime_type: mime_type.into(),
                sha256: sha256.into(),
                byte_len,
            },
        }
    }

    pub fn new_context_bundle_created(
        run_id: RunId,
        context_bundle_id: ContextBundleId,
        title: impl Into<String>,
        summary: impl Into<String>,
        artifact_ids: Vec<ArtifactId>,
        event_ids: Vec<EventId>,
    ) -> Self {
        Self {
            id: EventId::new(),
            run_id,
            occurred_at: OffsetDateTime::now_utc(),
            kind: OperationEventKind::ContextBundleCreated {
                context_bundle_id,
                title: title.into(),
                summary: summary.into(),
                artifact_ids,
                event_ids,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArtifactId, ArtifactKind, ContextBundleId, ToolCallId, ToolEffect};

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

    #[test]
    fn run_model_requested_event_captures_prompt() {
        let run_id = RunId::new();
        let event = OperationEvent::new_run_model_requested(run_id, "请总结今天的工作");

        assert_eq!(event.run_id, run_id);
        assert!(matches!(
            event.kind,
            OperationEventKind::RunModelRequested { ref prompt }
                if prompt == "请总结今天的工作"
        ));
    }

    #[test]
    fn run_assistant_message_event_captures_content() {
        let run_id = RunId::new();
        let event = OperationEvent::new_run_assistant_message(run_id, "今天完成了服务边界收敛。");

        assert_eq!(event.run_id, run_id);
        assert!(matches!(
            event.kind,
            OperationEventKind::RunAssistantMessage { ref content }
                if content == "今天完成了服务边界收敛。"
        ));
    }

    #[test]
    fn artifact_created_event_roundtrips_through_json() {
        let run_id = RunId::new();
        let artifact_id = ArtifactId::new();
        let event = OperationEvent::new_artifact_created(
            run_id,
            artifact_id,
            "Run summary",
            ArtifactKind::Markdown,
            "text/markdown",
            "abc123",
            42,
        );

        let json = serde_json::to_string(&event).unwrap();
        let decoded: OperationEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.run_id, run_id);
        assert!(matches!(
            decoded.kind,
            OperationEventKind::ArtifactCreated { artifact_id: id, kind: ArtifactKind::Markdown, byte_len: 42, .. } if id == artifact_id
        ));
    }

    #[test]
    fn context_bundle_created_event_roundtrips_through_json() {
        let run_id = RunId::new();
        let context_bundle_id = ContextBundleId::new();
        let artifact_id = ArtifactId::new();
        let event_id = EventId::new();
        let event = OperationEvent::new_context_bundle_created(
            run_id,
            context_bundle_id,
            "Debug context",
            "User report and artifact",
            vec![artifact_id],
            vec![event_id],
        );

        let json = serde_json::to_string(&event).unwrap();
        let decoded: OperationEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.run_id, run_id);
        assert!(matches!(
            decoded.kind,
            OperationEventKind::ContextBundleCreated { context_bundle_id: id, artifact_ids, event_ids, .. }
                if id == context_bundle_id && artifact_ids == vec![artifact_id] && event_ids == vec![event_id]
        ));
    }

    #[test]
    fn tool_call_requested_event_captures_effect_and_input() {
        let run_id = RunId::new();
        let tool_call_id = ToolCallId::new();
        let input_json = serde_json::json!({ "path": "README.md" });
        let event = OperationEvent::new_tool_call_requested(
            run_id,
            tool_call_id,
            "read_file",
            ToolEffect::ReadOnly,
            input_json.clone(),
        );

        assert_eq!(event.run_id, run_id);
        assert!(matches!(
            event.kind,
            OperationEventKind::ToolCallRequested {
                tool_call_id: actual_tool_call_id,
                ref tool_name,
                ref effect,
                input_json: ref actual_input_json,
            } if actual_tool_call_id == tool_call_id
                && tool_name == "read_file"
                && effect == &ToolEffect::ReadOnly
                && actual_input_json == &input_json
        ));
    }

    #[test]
    fn tool_call_result_events_capture_tool_call_id() {
        let run_id = RunId::new();
        let tool_call_id = ToolCallId::new();

        let approved = OperationEvent::new_tool_call_approved(run_id, tool_call_id, "mneme-policy");
        let denied = OperationEvent::new_tool_call_denied(run_id, tool_call_id, "需要人工确认");
        let completed = OperationEvent::new_tool_call_completed(
            run_id,
            tool_call_id,
            serde_json::json!({ "ok": true }),
        );
        let failed = OperationEvent::new_tool_call_failed(run_id, tool_call_id, "工具执行失败");

        assert!(
            matches!(approved.kind, OperationEventKind::ToolCallApproved { tool_call_id: id, .. } if id == tool_call_id)
        );
        assert!(
            matches!(denied.kind, OperationEventKind::ToolCallDenied { tool_call_id: id, .. } if id == tool_call_id)
        );
        assert!(
            matches!(completed.kind, OperationEventKind::ToolCallCompleted { tool_call_id: id, .. } if id == tool_call_id)
        );
        assert!(
            matches!(failed.kind, OperationEventKind::ToolCallFailed { tool_call_id: id, .. } if id == tool_call_id)
        );
    }
}
