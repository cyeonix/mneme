use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{OperationEvent, OperationEventKind, RunId, ToolCallId};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffect {
    ReadOnly,
    LocalWrite,
    Network,
    Shell,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolApprovalDecision {
    AutoApproved,
    RequiresApproval,
}

#[derive(Clone, Debug, Default)]
pub struct ToolApprovalPolicy;

impl ToolApprovalPolicy {
    pub fn decision_for(&self, effect: &ToolEffect) -> ToolApprovalDecision {
        match effect {
            ToolEffect::ReadOnly => ToolApprovalDecision::AutoApproved,
            ToolEffect::LocalWrite | ToolEffect::Network | ToolEffect::Shell => {
                ToolApprovalDecision::RequiresApproval
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Requested,
    Approved,
    Denied,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolCallProjection {
    pub run_id: RunId,
    pub tool_call_id: ToolCallId,
    pub tool_name: String,
    pub effect: ToolEffect,
    pub status: ToolCallStatus,
    pub requested_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub approved_by: Option<String>,
    pub denial_reason: Option<String>,
    pub failure_reason: Option<String>,
}

pub struct ToolProjector;

impl ToolProjector {
    pub fn replay(events: &[OperationEvent]) -> Vec<ToolCallProjection> {
        let mut tools = Vec::<ToolCallProjection>::new();

        for event in events {
            match &event.kind {
                OperationEventKind::ToolCallRequested {
                    tool_call_id,
                    tool_name,
                    effect,
                    ..
                } => {
                    if let Some(tool) = tools
                        .iter_mut()
                        .find(|tool| tool.tool_call_id == *tool_call_id)
                    {
                        tool.status = ToolCallStatus::Requested;
                        tool.tool_name = tool_name.clone();
                        tool.effect = effect.clone();
                        tool.requested_at = event.occurred_at;
                        tool.updated_at = event.occurred_at;
                        tool.approved_by = None;
                        tool.denial_reason = None;
                        tool.failure_reason = None;
                    } else {
                        tools.push(ToolCallProjection {
                            run_id: event.run_id,
                            tool_call_id: *tool_call_id,
                            tool_name: tool_name.clone(),
                            effect: effect.clone(),
                            status: ToolCallStatus::Requested,
                            requested_at: event.occurred_at,
                            updated_at: event.occurred_at,
                            approved_by: None,
                            denial_reason: None,
                            failure_reason: None,
                        });
                    }
                }
                OperationEventKind::ToolCallApproved {
                    tool_call_id,
                    approved_by,
                } => {
                    if let Some(tool) = tools
                        .iter_mut()
                        .find(|tool| tool.tool_call_id == *tool_call_id)
                    {
                        tool.status = ToolCallStatus::Approved;
                        tool.approved_by = Some(approved_by.clone());
                        tool.updated_at = event.occurred_at;
                    }
                }
                OperationEventKind::ToolCallDenied {
                    tool_call_id,
                    reason,
                } => {
                    if let Some(tool) = tools
                        .iter_mut()
                        .find(|tool| tool.tool_call_id == *tool_call_id)
                    {
                        tool.status = ToolCallStatus::Denied;
                        tool.denial_reason = Some(reason.clone());
                        tool.updated_at = event.occurred_at;
                    }
                }
                OperationEventKind::ToolCallCompleted { tool_call_id, .. } => {
                    if let Some(tool) = tools
                        .iter_mut()
                        .find(|tool| tool.tool_call_id == *tool_call_id)
                    {
                        tool.status = ToolCallStatus::Completed;
                        tool.updated_at = event.occurred_at;
                    }
                }
                OperationEventKind::ToolCallFailed {
                    tool_call_id,
                    reason,
                } => {
                    if let Some(tool) = tools
                        .iter_mut()
                        .find(|tool| tool.tool_call_id == *tool_call_id)
                    {
                        tool.status = ToolCallStatus::Failed;
                        tool.failure_reason = Some(reason.clone());
                        tool.updated_at = event.occurred_at;
                    }
                }
                _ => {}
            }
        }

        tools.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        tools
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_policy_auto_approves_read_only_tools() {
        assert_eq!(
            ToolApprovalPolicy::default().decision_for(&ToolEffect::ReadOnly),
            ToolApprovalDecision::AutoApproved
        );
    }

    #[test]
    fn approval_policy_requires_approval_for_side_effects() {
        let policy = ToolApprovalPolicy::default();

        assert_eq!(
            policy.decision_for(&ToolEffect::LocalWrite),
            ToolApprovalDecision::RequiresApproval
        );
        assert_eq!(
            policy.decision_for(&ToolEffect::Network),
            ToolApprovalDecision::RequiresApproval
        );
        assert_eq!(
            policy.decision_for(&ToolEffect::Shell),
            ToolApprovalDecision::RequiresApproval
        );
    }

    #[test]
    fn tool_projector_replays_request_approval_and_completion() {
        let run_id = RunId::new();
        let tool_call_id = ToolCallId::new();
        let events = vec![
            OperationEvent::new_tool_call_requested(
                run_id,
                tool_call_id,
                "read_file",
                ToolEffect::ReadOnly,
                serde_json::json!({ "path": "README.md" }),
            ),
            OperationEvent::new_tool_call_approved(run_id, tool_call_id, "mneme-policy"),
            OperationEvent::new_tool_call_completed(
                run_id,
                tool_call_id,
                serde_json::json!({ "content": "hello" }),
            ),
        ];

        let tools = ToolProjector::replay(&events);

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].tool_call_id, tool_call_id);
        assert_eq!(tools[0].status, ToolCallStatus::Completed);
        assert_eq!(tools[0].approved_by.as_deref(), Some("mneme-policy"));
    }
}
