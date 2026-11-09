use crate::{OperationEvent, RunId};
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LedgerError {
    #[error("operation ledger lock is poisoned")]
    Poisoned,
}

/// Operation Ledger 的抽象边界。
/// 后续 SQLite 实现也应该实现这个 trait，让 API 层不关心具体存储。
pub trait OperationLedger: Send + Sync {
    fn append(&self, event: OperationEvent) -> Result<(), LedgerError>;
    fn list(&self) -> Result<Vec<OperationEvent>, LedgerError>;
    fn list_for_run(&self, run_id: RunId) -> Result<Vec<OperationEvent>, LedgerError>;
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
}

impl OperationLedger for InMemoryOperationLedger {
    fn append(&self, event: OperationEvent) -> Result<(), LedgerError> {
        let mut events = self.events.lock().map_err(|_| LedgerError::Poisoned)?;
        events.push(event);
        Ok(())
    }

    fn list(&self) -> Result<Vec<OperationEvent>, LedgerError> {
        let events = self.events.lock().map_err(|_| LedgerError::Poisoned)?;
        Ok(events.clone())
    }

    fn list_for_run(&self, run_id: RunId) -> Result<Vec<OperationEvent>, LedgerError> {
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

    #[test]
    fn ledger_appends_events_in_order() {
        let ledger = InMemoryOperationLedger::new();
        let run_id = RunId::new();
        let first = OperationEvent::new_run_started(run_id, "First instruction");
        let second = OperationEvent::new_run_started(run_id, "Second instruction");

        ledger.append(first.clone()).expect("append first event");
        ledger.append(second.clone()).expect("append second event");

        assert_eq!(ledger.list().expect("list events"), vec![first, second]);
    }

    #[test]
    fn ledger_lists_events_for_one_run() {
        let ledger = InMemoryOperationLedger::new();
        let selected_run = RunId::new();
        let other_run = RunId::new();
        let selected_event = OperationEvent::new_run_started(selected_run, "Selected run");
        let other_event = OperationEvent::new_run_started(other_run, "Other run");

        ledger
            .append(selected_event.clone())
            .expect("append selected event");
        ledger.append(other_event).expect("append other event");

        assert_eq!(
            ledger
                .list_for_run(selected_run)
                .expect("list selected run"),
            vec![selected_event]
        );
    }
}
