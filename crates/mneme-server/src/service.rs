use crate::storage::SqliteSyncServerStore;
use mneme_kernel::{
    DeviceId, DeviceRecord, EventId, RunId, SyncCursorRecord, SyncObjectKind, SyncedEventRecord,
    SyncedObjectRecord,
};
use std::str::FromStr;
use time::{
    format_description::{well_known::Rfc3339, FormatItem},
    macros::format_description,
    OffsetDateTime, UtcOffset,
};

#[derive(Debug, thiserror::Error)]
pub enum SyncServerError {
    #[error("invalid input")]
    InvalidInput,
    #[error("unknown device")]
    UnknownDevice,
    #[error("storage error: {0}")]
    Storage(String),
}

#[derive(Debug)]
pub struct DeviceRegistrationOutcome {
    pub device: DeviceRecord,
    pub cursor: SyncCursorRecord,
    pub device_token: String,
}

#[derive(Debug)]
pub struct PushEventsOutcome {
    pub accepted: usize,
    pub duplicates: usize,
    pub latest_server_sequence: i64,
}

#[derive(Debug)]
pub struct PullEventsOutcome {
    pub events: Vec<SyncedEventRecord>,
    pub latest_server_sequence: i64,
}

#[derive(Debug)]
pub struct PushObjectsOutcome {
    pub accepted: usize,
    pub kept_existing: usize,
}

#[derive(Debug)]
pub struct PullObjectsOutcome {
    pub objects: Vec<SyncedObjectRecord>,
}

#[derive(Clone)]
pub struct SyncServerService {
    store: SqliteSyncServerStore,
}

impl SyncServerService {
    pub fn new(store: SqliteSyncServerStore) -> Self {
        Self { store }
    }

    pub async fn register_device(
        &self,
        display_name: String,
        public_key_fingerprint: Option<String>,
    ) -> Result<DeviceRegistrationOutcome, SyncServerError> {
        let display_name = non_blank(display_name)?;
        let public_key_fingerprint = public_key_fingerprint.map(non_blank).transpose()?;
        let device_id = DeviceId::new();
        let now = OffsetDateTime::now_utc();
        let now_text = format_datetime(now)?;

        let mut tx = self.store.pool().begin().await.map_err(storage_error)?;

        sqlx::query(
            "insert into devices (device_id, display_name, public_key_fingerprint, registered_at, last_seen_at) values (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(device_id.to_string())
        .bind(&display_name)
        .bind(public_key_fingerprint)
        .bind(&now_text)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;

        sqlx::query(
            "insert into sync_cursors (device_id, last_server_sequence, updated_at) values (?1, 0, ?2)",
        )
        .bind(device_id.to_string())
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;

        tx.commit().await.map_err(storage_error)?;

        let device = DeviceRecord {
            device_id,
            display_name,
            registered_at: now,
            last_seen_at: now,
        };
        let cursor = SyncCursorRecord {
            device_id,
            last_server_sequence: 0,
            updated_at: now,
        };
        let device_token = format!("mneme-dev-token-{}", device_id);

        Ok(DeviceRegistrationOutcome {
            device,
            cursor,
            device_token,
        })
    }

    pub async fn list_devices(&self) -> Result<Vec<DeviceRecord>, SyncServerError> {
        let rows = sqlx::query_as::<_, StoredDevice>(
            "select device_id, display_name, registered_at, last_seen_at from devices order by registered_at asc",
        )
        .fetch_all(self.store.pool())
        .await
        .map_err(storage_error)?;

        rows.into_iter().map(DeviceRecord::try_from).collect()
    }

    pub async fn list_cursors(&self) -> Result<Vec<SyncCursorRecord>, SyncServerError> {
        let rows = sqlx::query_as::<_, StoredSyncCursor>(
            "select device_id, last_server_sequence, updated_at from sync_cursors order by device_id asc",
        )
        .fetch_all(self.store.pool())
        .await
        .map_err(storage_error)?;

        rows.into_iter().map(SyncCursorRecord::try_from).collect()
    }

    pub async fn push_events(
        &self,
        device_id: DeviceId,
        events: Vec<SyncedEventRecord>,
    ) -> Result<PushEventsOutcome, SyncServerError> {
        self.ensure_known_device(device_id).await?;

        let mut accepted = 0;
        let mut duplicates = 0;
        let mut latest_server_sequence = self
            .store
            .latest_server_sequence()
            .await
            .map_err(storage_error)?;

        for event in events {
            if event.origin_device_id != device_id {
                return Err(SyncServerError::InvalidInput);
            }

            let received_at = OffsetDateTime::now_utc();
            let result = sqlx::query(
                "insert or ignore into synced_events (event_id, origin_device_id, run_id, occurred_at, kind_type, payload_json, received_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(event.event_id.to_string())
            .bind(event.origin_device_id.to_string())
            .bind(event.run_id.to_string())
            .bind(format_datetime(event.occurred_at)?)
            .bind(event.kind_type)
            .bind(event.payload_json.to_string())
            .bind(format_datetime(received_at)?)
            .execute(self.store.pool())
            .await
            .map_err(storage_error)?;

            if result.rows_affected() == 0 {
                duplicates += 1;
            } else {
                accepted += 1;
                latest_server_sequence = result.last_insert_rowid();
            }
        }

        Ok(PushEventsOutcome {
            accepted,
            duplicates,
            latest_server_sequence,
        })
    }

    pub async fn pull_events(
        &self,
        device_id: DeviceId,
        after_server_sequence: i64,
        limit: usize,
    ) -> Result<PullEventsOutcome, SyncServerError> {
        if after_server_sequence < 0 {
            return Err(SyncServerError::InvalidInput);
        }
        self.ensure_known_device(device_id).await?;

        let limit = limit.clamp(1, 500) as i64;
        let rows = sqlx::query_as::<_, StoredSyncedEvent>(
            "select server_sequence, event_id, origin_device_id, run_id, occurred_at, kind_type, payload_json, received_at from synced_events where server_sequence > ?1 order by server_sequence asc limit ?2",
        )
        .bind(after_server_sequence)
        .bind(limit)
        .fetch_all(self.store.pool())
        .await
        .map_err(storage_error)?;

        let latest_server_sequence = self
            .store
            .latest_server_sequence()
            .await
            .map_err(storage_error)?;
        let events = rows
            .into_iter()
            .map(SyncedEventRecord::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PullEventsOutcome {
            events,
            latest_server_sequence,
        })
    }

    pub async fn push_objects(
        &self,
        device_id: DeviceId,
        objects: Vec<SyncedObjectRecord>,
    ) -> Result<PushObjectsOutcome, SyncServerError> {
        self.ensure_known_device(device_id).await?;

        let mut accepted = 0;
        let mut kept_existing = 0;

        for incoming in objects {
            if incoming.origin_device_id != device_id {
                return Err(SyncServerError::InvalidInput);
            }

            if self.upsert_object(&incoming).await? {
                accepted += 1;
            } else {
                kept_existing += 1;
            }
        }

        Ok(PushObjectsOutcome {
            accepted,
            kept_existing,
        })
    }

    pub async fn pull_objects(
        &self,
        device_id: DeviceId,
        object_kind: Option<SyncObjectKind>,
        limit: usize,
    ) -> Result<PullObjectsOutcome, SyncServerError> {
        self.ensure_known_device(device_id).await?;

        let limit = limit.clamp(1, 500) as i64;
        let rows = if let Some(object_kind) = object_kind {
            sqlx::query_as::<_, StoredSyncedObject>(
                "select object_kind, object_id, origin_device_id, run_id, updated_at, payload_json, received_at from synced_objects where object_kind = ?1 order by updated_at desc, object_id asc limit ?2",
            )
            .bind(SqliteSyncServerStore::object_kind_to_text(object_kind))
            .bind(limit)
            .fetch_all(self.store.pool())
            .await
            .map_err(storage_error)?
        } else {
            sqlx::query_as::<_, StoredSyncedObject>(
                "select object_kind, object_id, origin_device_id, run_id, updated_at, payload_json, received_at from synced_objects order by updated_at desc, object_id asc limit ?1",
            )
            .bind(limit)
            .fetch_all(self.store.pool())
            .await
            .map_err(storage_error)?
        };
        let objects = rows
            .into_iter()
            .map(SyncedObjectRecord::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PullObjectsOutcome { objects })
    }

    pub async fn ack_events(
        &self,
        device_id: DeviceId,
        last_seen_server_sequence: i64,
    ) -> Result<SyncCursorRecord, SyncServerError> {
        if last_seen_server_sequence < 0 {
            return Err(SyncServerError::InvalidInput);
        }

        let now = OffsetDateTime::now_utc();
        let now_text = format_datetime(now)?;
        let mut tx = self.store.pool().begin().await.map_err(storage_error)?;

        let cursor_row = sqlx::query_as::<_, StoredSyncCursor>(
            "select device_id, last_server_sequence, updated_at from sync_cursors where device_id = ?1",
        )
        .bind(device_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?
        .ok_or(SyncServerError::UnknownDevice)?;
        let acknowledged_sequence = cursor_row
            .last_server_sequence
            .max(last_seen_server_sequence);

        sqlx::query(
            "update sync_cursors set last_server_sequence = ?1, updated_at = ?2 where device_id = ?3",
        )
        .bind(acknowledged_sequence)
        .bind(&now_text)
        .bind(device_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;

        sqlx::query("update devices set last_seen_at = ?1 where device_id = ?2")
            .bind(&now_text)
            .bind(device_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;

        tx.commit().await.map_err(storage_error)?;

        Ok(SyncCursorRecord {
            device_id,
            last_server_sequence: acknowledged_sequence,
            updated_at: now,
        })
    }

    async fn upsert_object(&self, object: &SyncedObjectRecord) -> Result<bool, SyncServerError> {
        let updated_at = format_datetime(object.updated_at)?;
        let result = sqlx::query(
            "insert into synced_objects (object_kind, object_id, origin_device_id, run_id, updated_at, payload_json, received_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7) on conflict(object_kind, object_id) do update set origin_device_id = excluded.origin_device_id, run_id = excluded.run_id, updated_at = excluded.updated_at, payload_json = excluded.payload_json, received_at = excluded.received_at where excluded.updated_at > synced_objects.updated_at or (excluded.updated_at = synced_objects.updated_at and excluded.origin_device_id > synced_objects.origin_device_id)",
        )
        .bind(SqliteSyncServerStore::object_kind_to_text(object.object_kind))
        .bind(&object.object_id)
        .bind(object.origin_device_id.to_string())
        .bind(object.run_id.to_string())
        .bind(&updated_at)
        .bind(object.payload_json.to_string())
        .bind(format_datetime(OffsetDateTime::now_utc())?)
        .execute(self.store.pool())
        .await
        .map_err(storage_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn ensure_known_device(&self, device_id: DeviceId) -> Result<(), SyncServerError> {
        if !self
            .store
            .has_device(&device_id.to_string())
            .await
            .map_err(storage_error)?
        {
            return Err(SyncServerError::UnknownDevice);
        }
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct StoredDevice {
    device_id: String,
    display_name: String,
    registered_at: String,
    last_seen_at: String,
}

impl TryFrom<StoredDevice> for DeviceRecord {
    type Error = SyncServerError;

    fn try_from(row: StoredDevice) -> Result<Self, Self::Error> {
        Ok(Self {
            device_id: DeviceId::from_str(&row.device_id).map_err(storage_error)?,
            display_name: row.display_name,
            registered_at: parse_datetime(&row.registered_at)?,
            last_seen_at: parse_datetime(&row.last_seen_at)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct StoredSyncCursor {
    device_id: String,
    last_server_sequence: i64,
    updated_at: String,
}

impl TryFrom<StoredSyncCursor> for SyncCursorRecord {
    type Error = SyncServerError;

    fn try_from(row: StoredSyncCursor) -> Result<Self, Self::Error> {
        Ok(Self {
            device_id: DeviceId::from_str(&row.device_id).map_err(storage_error)?,
            last_server_sequence: row.last_server_sequence,
            updated_at: parse_datetime(&row.updated_at)?,
        })
    }
}

#[derive(sqlx::FromRow)]
struct StoredSyncedEvent {
    server_sequence: i64,
    event_id: String,
    origin_device_id: String,
    run_id: String,
    occurred_at: String,
    kind_type: String,
    payload_json: String,
    received_at: String,
}

#[derive(sqlx::FromRow)]
struct StoredSyncedObject {
    object_kind: String,
    object_id: String,
    origin_device_id: String,
    run_id: String,
    updated_at: String,
    payload_json: String,
    received_at: String,
}

impl TryFrom<StoredSyncedEvent> for SyncedEventRecord {
    type Error = SyncServerError;

    fn try_from(row: StoredSyncedEvent) -> Result<Self, Self::Error> {
        Ok(Self {
            server_sequence: row.server_sequence,
            event_id: EventId::from_str(&row.event_id).map_err(storage_error)?,
            origin_device_id: DeviceId::from_str(&row.origin_device_id).map_err(storage_error)?,
            run_id: RunId::from_str(&row.run_id).map_err(storage_error)?,
            occurred_at: parse_datetime(&row.occurred_at)?,
            kind_type: row.kind_type,
            payload_json: serde_json::from_str(&row.payload_json).map_err(storage_error)?,
            received_at: parse_datetime(&row.received_at)?,
        })
    }
}

impl TryFrom<StoredSyncedObject> for SyncedObjectRecord {
    type Error = SyncServerError;

    fn try_from(row: StoredSyncedObject) -> Result<Self, Self::Error> {
        Ok(Self {
            object_id: row.object_id,
            object_kind: SqliteSyncServerStore::parse_object_kind(&row.object_kind)
                .ok_or(SyncServerError::InvalidInput)?,
            origin_device_id: DeviceId::from_str(&row.origin_device_id).map_err(storage_error)?,
            run_id: RunId::from_str(&row.run_id).map_err(storage_error)?,
            updated_at: parse_datetime(&row.updated_at)?,
            payload_json: serde_json::from_str(&row.payload_json).map_err(storage_error)?,
            received_at: parse_datetime(&row.received_at)?,
        })
    }
}

fn non_blank(value: String) -> Result<String, SyncServerError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(SyncServerError::InvalidInput);
    }
    Ok(trimmed.to_string())
}

static SQLITE_DATETIME_FORMAT: &[FormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:9]Z");

fn format_datetime(value: OffsetDateTime) -> Result<String, SyncServerError> {
    value
        .to_offset(UtcOffset::UTC)
        .format(SQLITE_DATETIME_FORMAT)
        .map_err(storage_error)
}

fn parse_datetime(value: &str) -> Result<OffsetDateTime, SyncServerError> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(storage_error)
}

fn storage_error(error: impl std::fmt::Display) -> SyncServerError {
    SyncServerError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::SqliteSyncServerStore;
    use mneme_kernel::{EventId, RunId, SyncObjectKind, SyncedEventRecord, SyncedObjectRecord};

    fn sample_synced_event(origin_device_id: DeviceId, kind_type: &str) -> SyncedEventRecord {
        SyncedEventRecord {
            server_sequence: 0,
            event_id: EventId::new(),
            origin_device_id,
            run_id: RunId::new(),
            occurred_at: time::OffsetDateTime::now_utc(),
            kind_type: kind_type.to_string(),
            payload_json: serde_json::json!({ "sample": true }),
            received_at: time::OffsetDateTime::now_utc(),
        }
    }

    fn sample_synced_object(
        origin_device_id: DeviceId,
        run_id: RunId,
        object_id: &str,
        title: &str,
        day: u8,
    ) -> SyncedObjectRecord {
        SyncedObjectRecord {
            object_id: object_id.to_string(),
            object_kind: SyncObjectKind::Artifact,
            origin_device_id,
            run_id,
            updated_at: time::OffsetDateTime::from_unix_timestamp(1_700_000_000 + i64::from(day))
                .unwrap(),
            payload_json: serde_json::json!({ "title": title }),
            received_at: time::OffsetDateTime::now_utc(),
        }
    }

    #[tokio::test]
    async fn object_push_uses_lww_conflict_rule() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);
        let device_a = service
            .register_device("A".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let device_b = service
            .register_device("B".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let run_id = RunId::new();

        let older = sample_synced_object(device_a, run_id, "artifact-1", "older", 1);
        let newer = sample_synced_object(device_b, run_id, "artifact-1", "newer", 2);

        service.push_objects(device_a, vec![older]).await.unwrap();
        service.push_objects(device_b, vec![newer]).await.unwrap();

        let pulled = service.pull_objects(device_a, None, 50).await.unwrap();
        assert_eq!(pulled.objects.len(), 1);
        assert_eq!(pulled.objects[0].payload_json["title"], "newer");
    }

    #[tokio::test]
    async fn object_push_treats_subsecond_timestamp_as_newer_than_whole_second() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);
        let device_a = service
            .register_device("A".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let device_b = service
            .register_device("B".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let run_id = RunId::new();
        let base_time = time::OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let older_whole_second = SyncedObjectRecord {
            updated_at: base_time,
            ..sample_synced_object(device_a, run_id, "artifact-1", "whole-second", 1)
        };
        let newer_subsecond = SyncedObjectRecord {
            updated_at: base_time + time::Duration::milliseconds(100),
            ..sample_synced_object(device_b, run_id, "artifact-1", "subsecond-newer", 1)
        };

        let first = service
            .push_objects(device_a, vec![older_whole_second])
            .await
            .unwrap();
        let second = service
            .push_objects(device_b, vec![newer_subsecond])
            .await
            .unwrap();

        assert_eq!(first.accepted, 1);
        assert_eq!(second.accepted, 1);
        assert_eq!(second.kept_existing, 0);
        let pulled = service.pull_objects(device_a, None, 50).await.unwrap();
        assert_eq!(pulled.objects.len(), 1);
        assert_eq!(pulled.objects[0].payload_json["title"], "subsecond-newer");
    }

    #[tokio::test]
    async fn object_push_uses_origin_device_id_as_same_timestamp_tiebreaker() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);
        let lower_device = service
            .register_device("00000000-0000-4000-8000-000000000001".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let higher_device = service
            .register_device("zzzzzzzz-zzzz-4zzz-8zzz-zzzzzzzzzzzz".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let (lower_device, higher_device) = if lower_device.to_string() < higher_device.to_string()
        {
            (lower_device, higher_device)
        } else {
            (higher_device, lower_device)
        };
        let run_id = RunId::new();
        let lower = sample_synced_object(lower_device, run_id, "artifact-1", "lower", 1);
        let higher = sample_synced_object(higher_device, run_id, "artifact-1", "higher", 1);

        let first = service
            .push_objects(lower_device, vec![lower])
            .await
            .unwrap();
        let second = service
            .push_objects(higher_device, vec![higher])
            .await
            .unwrap();

        assert_eq!(first.accepted, 1);
        assert_eq!(second.accepted, 1);
        let pulled = service.pull_objects(lower_device, None, 50).await.unwrap();
        assert_eq!(pulled.objects.len(), 1);
        assert_eq!(pulled.objects[0].origin_device_id, higher_device);
        assert_eq!(pulled.objects[0].payload_json["title"], "higher");
    }

    #[tokio::test]
    async fn object_push_same_timestamp_keeps_higher_origin_device_id_winner() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);
        let first_device = service
            .register_device("first".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let second_device = service
            .register_device("second".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let (lower_device, higher_device) = if first_device.to_string() < second_device.to_string()
        {
            (first_device, second_device)
        } else {
            (second_device, first_device)
        };
        let run_id = RunId::new();
        let higher = sample_synced_object(higher_device, run_id, "artifact-1", "higher", 1);
        let lower = sample_synced_object(lower_device, run_id, "artifact-1", "lower", 1);

        service
            .push_objects(higher_device, vec![higher])
            .await
            .unwrap();
        let stale = service
            .push_objects(lower_device, vec![lower])
            .await
            .unwrap();

        assert_eq!(stale.accepted, 0);
        assert_eq!(stale.kept_existing, 1);
        let pulled = service.pull_objects(higher_device, None, 50).await.unwrap();
        assert_eq!(pulled.objects.len(), 1);
        assert_eq!(pulled.objects[0].origin_device_id, higher_device);
        assert_eq!(pulled.objects[0].payload_json["title"], "higher");
    }

    #[tokio::test]
    async fn object_upsert_rejects_stale_writer_after_newer_winner_exists() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);
        let device_a = service
            .register_device("A".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let device_b = service
            .register_device("B".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let run_id = RunId::new();
        let older = sample_synced_object(device_a, run_id, "artifact-1", "older", 1);
        let newer = sample_synced_object(device_b, run_id, "artifact-1", "newer", 2);

        assert!(service.upsert_object(&newer).await.unwrap());
        assert!(!service.upsert_object(&older).await.unwrap());

        let pulled = service.pull_objects(device_a, None, 50).await.unwrap();
        assert_eq!(pulled.objects.len(), 1);
        assert_eq!(pulled.objects[0].payload_json["title"], "newer");
    }

    #[tokio::test]
    async fn pull_objects_rejects_unknown_device() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);

        assert!(matches!(
            service
                .pull_objects(DeviceId::new(), None, 50)
                .await
                .unwrap_err(),
            SyncServerError::UnknownDevice
        ));
    }

    #[tokio::test]
    async fn pull_objects_clamps_limit_to_one_and_five_hundred() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);
        let device = service
            .register_device("A".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let run_id = RunId::new();
        let mut objects = Vec::new();
        for index in 0..501 {
            objects.push(sample_synced_object(
                device,
                run_id,
                &format!("artifact-{index:03}"),
                &format!("object-{index:03}"),
                1,
            ));
        }
        service.push_objects(device, objects).await.unwrap();

        let clamped_min = service.pull_objects(device, None, 0).await.unwrap();
        let clamped_max = service.pull_objects(device, None, 501).await.unwrap();

        assert_eq!(clamped_min.objects.len(), 1);
        assert_eq!(clamped_min.objects[0].object_id, "artifact-000");
        assert_eq!(clamped_max.objects.len(), 500);
        assert_eq!(clamped_max.objects[0].object_id, "artifact-000");
        assert_eq!(clamped_max.objects[499].object_id, "artifact-499");
    }

    #[tokio::test]
    async fn pull_objects_filters_kind_and_orders_by_latest_update() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);
        let device = service
            .register_device("A".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let run_id = RunId::new();

        let artifact_older = sample_synced_object(device, run_id, "artifact-1", "older", 1);
        let artifact_newer = sample_synced_object(device, run_id, "artifact-2", "newer", 3);
        let run_object = SyncedObjectRecord {
            object_id: "run-1".to_string(),
            object_kind: SyncObjectKind::Run,
            origin_device_id: device,
            run_id,
            updated_at: time::OffsetDateTime::from_unix_timestamp(1_700_000_002).unwrap(),
            payload_json: serde_json::json!({ "title": "run" }),
            received_at: time::OffsetDateTime::now_utc(),
        };

        service
            .push_objects(device, vec![artifact_older, run_object, artifact_newer])
            .await
            .unwrap();

        let pulled = service
            .pull_objects(device, Some(SyncObjectKind::Artifact), 50)
            .await
            .unwrap();
        assert_eq!(pulled.objects.len(), 2);
        assert_eq!(pulled.objects[0].object_id, "artifact-2");
        assert_eq!(pulled.objects[1].object_id, "artifact-1");
    }

    #[tokio::test]
    async fn object_push_rejects_unknown_device_and_origin_mismatch() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);
        let device = service
            .register_device("A".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let run_id = RunId::new();

        assert!(matches!(
            service
                .push_objects(
                    DeviceId::new(),
                    vec![sample_synced_object(device, run_id, "artifact-1", "bad", 1)]
                )
                .await
                .unwrap_err(),
            SyncServerError::UnknownDevice
        ));

        assert!(matches!(
            service
                .push_objects(
                    device,
                    vec![sample_synced_object(
                        DeviceId::new(),
                        run_id,
                        "artifact-1",
                        "bad",
                        1
                    )]
                )
                .await
                .unwrap_err(),
            SyncServerError::InvalidInput
        ));
    }

    #[tokio::test]
    async fn event_push_is_idempotent_and_pull_respects_cursor() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);
        let device_a = service
            .register_device("A".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let device_b = service
            .register_device("B".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let event = sample_synced_event(device_a, "run_started");

        let first = service
            .push_events(device_a, vec![event.clone()])
            .await
            .unwrap();
        assert_eq!(first.accepted, 1);
        assert_eq!(first.duplicates, 0);

        let second = service.push_events(device_a, vec![event]).await.unwrap();
        assert_eq!(second.accepted, 0);
        assert_eq!(second.duplicates, 1);

        let pulled = service.pull_events(device_b, 0, 50).await.unwrap();
        assert_eq!(pulled.events.len(), 1);
        assert_eq!(pulled.events[0].origin_device_id, device_a);

        service
            .ack_events(device_b, pulled.events[0].server_sequence)
            .await
            .unwrap();
        let after_ack = service
            .pull_events(device_b, pulled.events[0].server_sequence, 50)
            .await
            .unwrap();
        assert!(after_ack.events.is_empty());
    }

    #[tokio::test]
    async fn event_push_rejects_unknown_device_and_origin_mismatch() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);
        let device_a = service
            .register_device("A".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;

        assert!(matches!(
            service
                .push_events(
                    DeviceId::new(),
                    vec![sample_synced_event(DeviceId::new(), "run_started")]
                )
                .await
                .unwrap_err(),
            SyncServerError::UnknownDevice
        ));

        assert!(matches!(
            service
                .push_events(
                    device_a,
                    vec![sample_synced_event(DeviceId::new(), "run_started")]
                )
                .await
                .unwrap_err(),
            SyncServerError::InvalidInput
        ));
    }

    #[tokio::test]
    async fn pull_events_rejects_unknown_device_and_negative_cursor() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);
        let device_a = service
            .register_device("A".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;

        assert!(matches!(
            service
                .pull_events(DeviceId::new(), 0, 50)
                .await
                .unwrap_err(),
            SyncServerError::UnknownDevice
        ));
        assert!(matches!(
            service.pull_events(device_a, -1, 50).await.unwrap_err(),
            SyncServerError::InvalidInput
        ));
    }

    #[tokio::test]
    async fn service_ack_events_does_not_move_cursor_backward() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);
        let registered = service
            .register_device("Phone".to_string(), None)
            .await
            .unwrap();

        service
            .ack_events(registered.device.device_id, 42)
            .await
            .unwrap();
        let cursor = service
            .ack_events(registered.device.device_id, 10)
            .await
            .unwrap();

        assert_eq!(cursor.last_server_sequence, 42);
    }

    #[tokio::test]
    async fn service_register_device_creates_initial_cursor() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);

        let registered = service
            .register_device("Laptop".to_string(), Some("fp-1".to_string()))
            .await
            .unwrap();
        assert_eq!(registered.device.display_name, "Laptop");
        assert_eq!(registered.cursor.last_server_sequence, 0);
        assert!(!registered.device_token.is_empty());

        let devices = service.list_devices().await.unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_id, registered.device.device_id);
    }

    #[tokio::test]
    async fn service_rejects_blank_device_name() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);

        assert!(matches!(
            service
                .register_device("   ".to_string(), None)
                .await
                .unwrap_err(),
            SyncServerError::InvalidInput
        ));
    }

    #[tokio::test]
    async fn service_ack_events_updates_existing_cursor() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);
        let registered = service
            .register_device("Tablet".to_string(), None)
            .await
            .unwrap();

        let cursor = service
            .ack_events(registered.device.device_id, 42)
            .await
            .unwrap();
        assert_eq!(cursor.device_id, registered.device.device_id);
        assert_eq!(cursor.last_server_sequence, 42);

        let cursors = service.list_cursors().await.unwrap();
        assert_eq!(cursors, vec![cursor]);
    }

    #[tokio::test]
    async fn service_rejects_ack_for_unknown_device() {
        let store = SqliteSyncServerStore::connect("sqlite::memory:")
            .await
            .unwrap();
        let service = SyncServerService::new(store);

        assert!(matches!(
            service
                .ack_events(mneme_kernel::DeviceId::new(), 1)
                .await
                .unwrap_err(),
            SyncServerError::UnknownDevice
        ));
    }
}
