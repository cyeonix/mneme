use mneme_kernel::{KernelHealth, OperationEvent, RunProjection};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventsResponse {
    pub events: Vec<OperationEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunsResponse {
    pub runs: Vec<RunProjection>,
}
