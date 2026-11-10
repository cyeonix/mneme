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
}
