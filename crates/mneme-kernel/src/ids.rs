use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use uuid::Uuid;

/// Ledger 中每条事件的唯一 ID。
/// 用 newtype 包一层，避免把 EventId 和 RunId 误传。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(Uuid);

/// 一次 Agent/用户工作流运行的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(Uuid);

/// 一次工具调用请求的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolCallId(Uuid);

/// 运行产物的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtifactId(Uuid);

/// 上下文包的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContextBundleId(Uuid);

/// 知识来源的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KnowledgeSourceId(Uuid);

/// 可检索文本块的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentChunkId(Uuid);

/// 可解释记忆的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryId(Uuid);

/// 一次检索审计记录的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RetrievalTraceId(Uuid);

/// 执行后台 Worker 的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkerId(Uuid);

/// 后台 Worker Job 的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkerJobId(Uuid);

impl EventId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for EventId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

impl RunId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for RunId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

impl ToolCallId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ToolCallId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ToolCallId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ToolCallId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

impl ArtifactId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ArtifactId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ArtifactId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ArtifactId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

impl ContextBundleId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ContextBundleId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ContextBundleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ContextBundleId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

impl KnowledgeSourceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for KnowledgeSourceId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for KnowledgeSourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for KnowledgeSourceId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

impl DocumentChunkId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for DocumentChunkId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for DocumentChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for DocumentChunkId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

impl MemoryId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MemoryId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MemoryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for MemoryId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

impl RetrievalTraceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RetrievalTraceId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RetrievalTraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for RetrievalTraceId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

impl WorkerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for WorkerId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for WorkerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for WorkerId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

impl WorkerJobId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for WorkerJobId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for WorkerJobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for WorkerJobId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_event_ids_are_unique() {
        let first = EventId::new();
        let second = EventId::new();

        assert_ne!(first, second);
    }

    #[test]
    fn generated_run_ids_are_displayable() {
        let run_id = RunId::new().to_string();

        assert_eq!(run_id.len(), 36);
        assert!(run_id.contains('-'));
    }

    #[test]
    fn generated_tool_call_ids_are_displayable() {
        let tool_call_id = ToolCallId::new().to_string();

        assert_eq!(tool_call_id.len(), 36);
        assert!(tool_call_id.contains('-'));
    }

    #[test]
    fn generated_artifact_and_context_bundle_ids_are_displayable() {
        let artifact_id = ArtifactId::new().to_string();
        let context_bundle_id = ContextBundleId::new().to_string();

        assert_eq!(artifact_id.len(), 36);
        assert!(artifact_id.contains('-'));
        assert_eq!(context_bundle_id.len(), 36);
        assert!(context_bundle_id.contains('-'));
    }

    #[test]
    fn generated_knowledge_memory_and_retrieval_ids_are_displayable() {
        let source_id = KnowledgeSourceId::new().to_string();
        let chunk_id = DocumentChunkId::new().to_string();
        let memory_id = MemoryId::new().to_string();
        let trace_id = RetrievalTraceId::new().to_string();

        for id in [source_id, chunk_id, memory_id, trace_id] {
            assert_eq!(id.len(), 36);
            assert!(id.contains('-'));
        }
    }

    #[test]
    fn knowledge_memory_and_retrieval_ids_parse_from_display() {
        let source_id = KnowledgeSourceId::new();
        let chunk_id = DocumentChunkId::new();
        let memory_id = MemoryId::new();
        let trace_id = RetrievalTraceId::new();

        assert_eq!(
            source_id.to_string().parse::<KnowledgeSourceId>().unwrap(),
            source_id
        );
        assert_eq!(
            chunk_id.to_string().parse::<DocumentChunkId>().unwrap(),
            chunk_id
        );
        assert_eq!(
            memory_id.to_string().parse::<MemoryId>().unwrap(),
            memory_id
        );
        assert_eq!(
            trace_id.to_string().parse::<RetrievalTraceId>().unwrap(),
            trace_id
        );
    }
}
