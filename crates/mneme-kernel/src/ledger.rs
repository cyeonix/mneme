use crate::{OperationEvent, RunId};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub type SharedOperationLedger = Arc<dyn OperationLedger>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LedgerError {
    #[error("operation ledger lock is poisoned")]
    Poisoned,
}

/// Operation Ledger 的抽象边界。
/// 后续 SQLite 实现也应该实现这个 trait，让 API 层不关心具体存储。
#[async_trait]
pub trait OperationLedger: Send + Sync {
    async fn append(&self, event: OperationEvent) -> Result<(), LedgerError>;
    async fn list(&self) -> Result<Vec<OperationEvent>, LedgerError>;
    async fn list_for_run(&self, run_id: RunId) -> Result<Vec<OperationEvent>, LedgerError>;
}

/// 第一阶段只使用内存版 Ledger，用来验证领域模型和 API 边界。
#[derive(Debug, Clone, Default)]
pub struct InMemoryOperationLedger {
    events: Arc<Mutex<Vec<OperationEvent>>>,
}

impl InMemoryOperationLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared(self) -> SharedOperationLedger {
        Arc::new(self)
    }
}

#[async_trait]
impl OperationLedger for InMemoryOperationLedger {
    async fn append(&self, event: OperationEvent) -> Result<(), LedgerError> {
        let mut events = self.events.lock().map_err(|_| LedgerError::Poisoned)?;
        events.push(event);
        Ok(())
    }

    async fn list(&self) -> Result<Vec<OperationEvent>, LedgerError> {
        let events = self.events.lock().map_err(|_| LedgerError::Poisoned)?;
        Ok(events.clone())
    }

    async fn list_for_run(&self, run_id: RunId) -> Result<Vec<OperationEvent>, LedgerError> {
        let events = self.events.lock().map_err(|_| LedgerError::Poisoned)?;
        Ok(events
            .iter()
            .filter(|event| event.run_id == run_id)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OperationEvent;

    #[tokio::test]
    async fn ledger_appends_events_in_order() {
        let ledger = InMemoryOperationLedger::new();
        let first_run = RunId::new();
        let first = OperationEvent::new_run_started(first_run, "第一条指令");
        let second = OperationEvent::new_run_started(RunId::new(), "第二条指令");

        ledger.append(first.clone()).await.unwrap();
        ledger.append(second.clone()).await.unwrap();

        assert_eq!(ledger.list().await.unwrap(), vec![first, second]);
    }

    #[tokio::test]
    async fn ledger_lists_events_for_one_run() {
        let ledger = InMemoryOperationLedger::new();
        let selected_run = RunId::new();
        let other_run = RunId::new();
        let selected_event = OperationEvent::new_run_started(selected_run, "Selected run");
        let other_event = OperationEvent::new_run_started(other_run, "Other run");

        ledger.append(selected_event.clone()).await.unwrap();
        ledger.append(other_event).await.unwrap();

        assert_eq!(
            ledger.list_for_run(selected_run).await.unwrap(),
            vec![selected_event]
        );
    }
}
