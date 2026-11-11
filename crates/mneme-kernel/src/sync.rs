use crate::ids::{BackupId, DeviceId, EventId, RunId, SyncBatchId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceRecord {
    pub device_id: DeviceId,
    pub display_name: String,
    pub public_key_fingerprint: Option<String>,
    pub registered_at: OffsetDateTime,
    pub last_seen_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncCursorRecord {
    pub device_id: DeviceId,
    pub last_server_sequence: i64,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncedEventRecord {
    pub server_sequence: i64,
    pub event_id: EventId,
    pub origin_device_id: DeviceId,
    pub run_id: RunId,
    pub occurred_at: OffsetDateTime,
    pub kind_type: String,
    pub payload_json: Value,
    pub received_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncObjectKind {
    Run,
    ToolCall,
    Artifact,
    ContextBundle,
    KnowledgeSource,
    DocumentChunk,
    Memory,
    RetrievalTrace,
    WorkerJob,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncedObjectRecord {
    pub object_id: String,
    pub object_kind: SyncObjectKind,
    pub origin_device_id: DeviceId,
    pub run_id: RunId,
    pub updated_at: OffsetDateTime,
    pub payload_json: Value,
    pub received_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupRecord {
    pub backup_id: BackupId,
    pub device_id: DeviceId,
    pub created_at: OffsetDateTime,
    pub event_count: u64,
    pub object_count: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisterDeviceRequest {
    pub display_name: String,
    pub public_key_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisterDeviceResponse {
    pub device: DeviceRecord,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PushEventsRequest {
    pub device_id: DeviceId,
    pub batch_id: SyncBatchId,
    pub events: Vec<SyncedEventRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PushEventsResponse {
    pub batch_id: SyncBatchId,
    pub accepted_event_ids: Vec<EventId>,
    pub duplicate_event_ids: Vec<EventId>,
    pub latest_server_sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullEventsRequest {
    pub device_id: DeviceId,
    pub after_server_sequence: i64,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullEventsResponse {
    pub events: Vec<SyncedEventRecord>,
    pub latest_server_sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AckEventsRequest {
    pub device_id: DeviceId,
    pub through_server_sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PushObjectsRequest {
    pub device_id: DeviceId,
    pub batch_id: SyncBatchId,
    pub objects: Vec<SyncedObjectRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PushObjectsResponse {
    pub batch_id: SyncBatchId,
    pub winners: Vec<SyncedObjectRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullObjectsRequest {
    pub device_id: DeviceId,
    pub object_kind: Option<SyncObjectKind>,
    pub updated_after: Option<OffsetDateTime>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullObjectsResponse {
    pub objects: Vec<SyncedObjectRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerOverviewResponse {
    pub devices: Vec<DeviceRecord>,
    pub latest_server_sequence: i64,
    pub event_count: u64,
    pub object_count: u64,
    pub backups: Vec<BackupRecord>,
}

pub fn lww_object_winner(
    left: SyncedObjectRecord,
    right: SyncedObjectRecord,
) -> SyncedObjectRecord {
    if right.updated_at > left.updated_at {
        return right;
    }
    if left.updated_at > right.updated_at {
        return left;
    }
    if right.origin_device_id.to_string() > left.origin_device_id.to_string() {
        right
    } else {
        left
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::str::FromStr;
    use time::macros::datetime;

    #[test]
    fn lww_object_winner_uses_updated_at_then_origin_device_id() {
        let run_id = RunId::new();
        let left_device = DeviceId::from_str("00000000-0000-4000-8000-000000000001").unwrap();
        let right_device = DeviceId::from_str("00000000-0000-4000-8000-000000000002").unwrap();
        let older = SyncedObjectRecord {
            object_id: "artifact-1".to_string(),
            object_kind: SyncObjectKind::Artifact,
            origin_device_id: left_device,
            run_id,
            updated_at: datetime!(2026-01-01 00:00 UTC),
            payload_json: json!({ "title": "older" }),
            received_at: datetime!(2026-01-01 00:01 UTC),
        };
        let newer = SyncedObjectRecord {
            updated_at: datetime!(2026-01-02 00:00 UTC),
            payload_json: json!({ "title": "newer" }),
            ..older.clone()
        };

        assert_eq!(lww_object_winner(older.clone(), newer.clone()), newer);

        let same_time_left = SyncedObjectRecord {
            origin_device_id: left_device,
            updated_at: datetime!(2026-01-03 00:00 UTC),
            payload_json: json!({ "title": "left" }),
            ..older.clone()
        };
        let same_time_right = SyncedObjectRecord {
            origin_device_id: right_device,
            updated_at: datetime!(2026-01-03 00:00 UTC),
            payload_json: json!({ "title": "right" }),
            ..older
        };

        assert_eq!(
            lww_object_winner(same_time_left, same_time_right.clone()),
            same_time_right
        );
    }

    #[test]
    fn synced_event_record_preserves_event_identity_and_origin_device() {
        let device_id = DeviceId::new();
        let event_id = EventId::new();
        let record = SyncedEventRecord {
            server_sequence: 42,
            event_id,
            origin_device_id: device_id,
            run_id: RunId::new(),
            occurred_at: time::OffsetDateTime::now_utc(),
            kind_type: "run_started".to_string(),
            payload_json: json!({ "instruction": "hello" }),
            received_at: time::OffsetDateTime::now_utc(),
        };

        assert_eq!(record.event_id, event_id);
        assert_eq!(record.origin_device_id, device_id);
    }
}
