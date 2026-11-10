use mneme_kernel::{
    ArtifactDetail, ArtifactKind, ArtifactRecord, ContextBundleRecord, KernelHealth,
    OperationEvent, RunProjection, ToolApprovalDecision, ToolEffect,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// HTTP health 返回体。API DTO 与 Kernel 领域类型分开，便于以后做版本兼容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub service: String,
    pub status: String,
    pub version: String,
}

impl From<KernelHealth> for HealthResponse {
    fn from(value: KernelHealth) -> Self {
        Self {
            service: value.service,
            status: value.status,
            version: value.version,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRunRequest {
    pub instruction: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRunResponse {
    pub run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleTextRequest {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunLifecycleResponse {
    pub run_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub tool_name: String,
    pub effect: ToolEffect,
    pub input_json: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallResponse {
    pub run_id: String,
    pub tool_call_id: String,
    pub approval: ToolApprovalDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolLifecycleRequest {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResultRequest {
    pub output_json: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolLifecycleResponse {
    pub run_id: String,
    pub tool_call_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateArtifactRequest {
    pub title: String,
    pub kind: ArtifactKind,
    pub mime_type: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateArtifactResponse {
    pub artifact_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactsResponse {
    pub artifacts: Vec<ArtifactRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactDetailResponse {
    pub artifact: ArtifactRecord,
    pub content: Option<String>,
}

impl From<ArtifactDetail> for ArtifactDetailResponse {
    fn from(value: ArtifactDetail) -> Self {
        Self {
            artifact: value.artifact,
            content: value.content,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateContextBundleRequest {
    pub title: String,
    pub summary: String,
    pub artifact_ids: Vec<String>,
    pub event_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateContextBundleResponse {
    pub context_bundle_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextBundlesResponse {
    pub context_bundles: Vec<ContextBundleRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventsResponse {
    pub events: Vec<OperationEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunsResponse {
    pub runs: Vec<RunProjection>,
}
