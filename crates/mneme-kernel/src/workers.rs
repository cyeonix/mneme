use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{RunId, WorkerId, WorkerJobId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerJobKind {
    ModelCompletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerJobStatus {
    Started,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerJobRecord {
    pub worker_job_id: WorkerJobId,
    pub worker_id: WorkerId,
    pub run_id: RunId,
    pub kind: WorkerJobKind,
    pub status: WorkerJobStatus,
    pub model: Option<String>,
    pub error: Option<String>,
    pub started_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
}

#[cfg(test)]
mod tests {
    use crate::{WorkerId, WorkerJobId};

    #[test]
    fn worker_ids_are_displayable_and_parseable() {
        let worker_id = WorkerId::new();
        assert_eq!(
            worker_id.to_string().parse::<WorkerId>().unwrap(),
            worker_id
        );

        let job_id = WorkerJobId::new();
        assert_eq!(job_id.to_string().parse::<WorkerJobId>().unwrap(), job_id);
    }
}
