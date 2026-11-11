use crate::storage::SqliteSyncServerStore;
use mneme_kernel::{
    BackupId, BackupRecord, DeviceId, DeviceRecord, EventId, RunId, SyncCursorRecord,
    SyncObjectKind, SyncedEventRecord, SyncedObjectRecord,
};
use sha2::{Digest, Sha256};
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

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BackupPayload {
    pub devices: Vec<DeviceRecord>,
    pub cursors: Vec<SyncCursorRecord>,
    pub events: Vec<SyncedEventRecord>,
    pub objects: Vec<SyncedObjectRecord>,
}

#[derive(Debug)]
pub struct BackupExportOutcome {
    pub backup: BackupRecord,
    pub payload_json: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerOverviewResponse {
    pub devices: usize,
    pub synced_events: usize,
    pub synced_objects: usize,
    pub latest_server_sequence: i64,
    pub latest_event_received_at: Option<String>,
    pub latest_backup_created_at: Option<String>,
    pub backups: Vec<BackupRecord>,
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

    pub async fn export_backup(&self) -> Result<BackupExportOutcome, SyncServerError> {
        let payload = BackupPayload {
            devices: self.list_devices().await?,
            cursors: self.list_cursors().await?,
            events: self.list_events().await?,
            objects: self.list_objects().await?,
        };
        let payload_json = serde_json::to_value(&payload).map_err(storage_error)?;
        let payload_bytes = serde_json::to_vec(&payload_json).map_err(storage_error)?;
        let sha256 = format!("{:x}", Sha256::digest(&payload_bytes));
        let backup_id = BackupId::new();
        let created_at = OffsetDateTime::now_utc();
        let exporting_device = payload
            .devices
            .first()
            .map(|device| device.device_id)
            .unwrap_or_else(DeviceId::new);
        let backup = BackupRecord {
            backup_id,
            device_id: exporting_device,
            created_at,
            event_count: payload.events.len() as u64,
            object_count: payload.objects.len() as u64,
            sha256,
        };

        sqlx::query(
            "insert into backups (backup_id, device_id, created_at, event_count, object_count, sha256, payload_json) values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(backup.backup_id.to_string())
        .bind(backup.device_id.to_string())
        .bind(format_datetime(backup.created_at)?)
        .bind(backup.event_count as i64)
        .bind(backup.object_count as i64)
        .bind(&backup.sha256)
        .bind(payload_json.to_string())
        .execute(self.store.pool())
        .await
        .map_err(storage_error)?;

        Ok(BackupExportOutcome {
            backup,
            payload_json,
        })
    }

    pub async fn import_backup(
        &self,
        payload_json: serde_json::Value,
    ) -> Result<BackupRecord, SyncServerError> {
        let payload: BackupPayload = serde_json::from_value(payload_json.clone())
            .map_err(|_| SyncServerError::InvalidInput)?;
        let payload_bytes = serde_json::to_vec(&payload_json).map_err(storage_error)?;
        let sha256 = format!("{:x}", Sha256::digest(&payload_bytes));
        let backup_id = BackupId::new();
        let created_at = OffsetDateTime::now_utc();
        let importing_device = payload
            .devices
            .first()
            .map(|device| device.device_id)
            .unwrap_or_else(DeviceId::new);
        let backup = BackupRecord {
            backup_id,
            device_id: importing_device,
            created_at,
            event_count: payload.events.len() as u64,
            object_count: payload.objects.len() as u64,
            sha256,
        };

        let mut tx = self.store.pool().begin().await.map_err(storage_error)?;
        for device in &payload.devices {
            sqlx::query(
                "insert into devices (device_id, display_name, registered_at, last_seen_at) values (?1, ?2, ?3, ?4) on conflict(device_id) do update set display_name = excluded.display_name, registered_at = excluded.registered_at, last_seen_at = excluded.last_seen_at",
            )
            .bind(device.device_id.to_string())
            .bind(&device.display_name)
            .bind(format_datetime(device.registered_at)?)
            .bind(format_datetime(device.last_seen_at)?)
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        }
        for event in &payload.events {
            if event.server_sequence <= 0 {
                return Err(SyncServerError::InvalidInput);
            }
            sqlx::query(
                "insert into synced_events (server_sequence, event_id, origin_device_id, run_id, occurred_at, kind_type, payload_json, received_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) on conflict(event_id) do nothing",
            )
            .bind(event.server_sequence)
            .bind(event.event_id.to_string())
            .bind(event.origin_device_id.to_string())
            .bind(event.run_id.to_string())
            .bind(format_datetime(event.occurred_at)?)
            .bind(&event.kind_type)
            .bind(event.payload_json.to_string())
            .bind(format_datetime(event.received_at)?)
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        }
        for cursor in &payload.cursors {
            sqlx::query(
                "insert into sync_cursors (device_id, last_server_sequence, updated_at) values (?1, ?2, ?3) on conflict(device_id) do update set last_server_sequence = excluded.last_server_sequence, updated_at = excluded.updated_at",
            )
            .bind(cursor.device_id.to_string())
            .bind(cursor.last_server_sequence)
            .bind(format_datetime(cursor.updated_at)?)
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        }
        for object in &payload.objects {
            Self::upsert_object_in_tx(&mut tx, object, object.received_at).await?;
        }
        sqlx::query(
            "insert into backups (backup_id, device_id, created_at, event_count, object_count, sha256, payload_json) values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(backup.backup_id.to_string())
        .bind(backup.device_id.to_string())
        .bind(format_datetime(backup.created_at)?)
        .bind(backup.event_count as i64)
        .bind(backup.object_count as i64)
        .bind(&backup.sha256)
        .bind(payload_json.to_string())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;

        Ok(backup)
    }

    pub async fn server_overview(&self) -> Result<ServerOverviewResponse, SyncServerError> {
        Ok(ServerOverviewResponse {
            devices: self.store.count_devices().await.map_err(storage_error)?,
            synced_events: self
                .store
                .count_synced_events()
                .await
                .map_err(storage_error)?,
            synced_objects: self
                .store
                .count_synced_objects()
                .await
                .map_err(storage_error)?,
            latest_server_sequence: self
                .store
                .latest_server_sequence()
                .await
                .map_err(storage_error)?,
            latest_event_received_at: self
                .store
                .latest_event_received_at()
                .await
                .map_err(storage_error)?,
            latest_backup_created_at: self
                .store
                .latest_backup_created_at()
                .await
                .map_err(storage_error)?,
            backups: self.list_backups().await?,
        })
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

    async fn list_events(&self) -> Result<Vec<SyncedEventRecord>, SyncServerError> {
        let rows = sqlx::query_as::<_, StoredSyncedEvent>(
            "select server_sequence, event_id, origin_device_id, run_id, occurred_at, kind_type, payload_json, received_at from synced_events order by server_sequence asc",
        )
        .fetch_all(self.store.pool())
        .await
        .map_err(storage_error)?;

        rows.into_iter().map(SyncedEventRecord::try_from).collect()
    }

    async fn list_objects(&self) -> Result<Vec<SyncedObjectRecord>, SyncServerError> {
        let rows = sqlx::query_as::<_, StoredSyncedObject>(
            "select object_kind, object_id, origin_device_id, run_id, updated_at, payload_json, received_at from synced_objects order by object_kind asc, object_id asc",
        )
        .fetch_all(self.store.pool())
        .await
        .map_err(storage_error)?;

        rows.into_iter().map(SyncedObjectRecord::try_from).collect()
    }

    pub async fn list_backups(&self) -> Result<Vec<BackupRecord>, SyncServerError> {
        let rows = sqlx::query_as::<_, StoredBackup>(
            "select backup_id, device_id, created_at, event_count, object_count, sha256 from backups order by created_at desc, backup_id asc",
        )
        .fetch_all(self.store.pool())
        .await
        .map_err(storage_error)?;

        rows.into_iter().map(BackupRecord::try_from).collect()
    }

    async fn upsert_object(&self, object: &SyncedObjectRecord) -> Result<bool, SyncServerError> {
        let mut tx = self.store.pool().begin().await.map_err(storage_error)?;
        let changed = Self::upsert_object_in_tx(&mut tx, object, OffsetDateTime::now_utc()).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(changed)
    }

    async fn upsert_object_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        object: &SyncedObjectRecord,
        received_at: OffsetDateTime,
    ) -> Result<bool, SyncServerError> {
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
        .bind(format_datetime(received_at)?)
        .execute(&mut **tx)
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

#[derive(sqlx::FromRow)]
struct StoredBackup {
    backup_id: String,
    device_id: String,
    created_at: String,
    event_count: i64,
    object_count: i64,
    sha256: String,
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

impl TryFrom<StoredBackup> for BackupRecord {
    type Error = SyncServerError;

    fn try_from(row: StoredBackup) -> Result<Self, Self::Error> {
        Ok(Self {
            backup_id: BackupId::from_str(&row.backup_id).map_err(storage_error)?,
            device_id: DeviceId::from_str(&row.device_id).map_err(storage_error)?,
            created_at: parse_datetime(&row.created_at)?,
            event_count: row.event_count as u64,
            object_count: row.object_count as u64,
            sha256: row.sha256,
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
    async fn backup_export_import_roundtrips_sync_state() {
        let source = SyncServerService::new(
            SqliteSyncServerStore::connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        let device = source
            .register_device("A".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        source
            .push_events(device, vec![sample_synced_event(device, "run_started")])
            .await
            .unwrap();
        source
            .push_objects(
                device,
                vec![sample_synced_object(
                    device,
                    RunId::new(),
                    "artifact-1",
                    "A",
                    1,
                )],
            )
            .await
            .unwrap();

        let backup = source.export_backup().await.unwrap();

        let target = SyncServerService::new(
            SqliteSyncServerStore::connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        target
            .import_backup(backup.payload_json.clone())
            .await
            .unwrap();

        let overview = target.server_overview().await.unwrap();
        assert_eq!(overview.devices, 1);
        assert_eq!(overview.synced_events, 1);
        assert_eq!(overview.synced_objects, 1);
    }

    #[tokio::test]
    async fn backup_import_is_idempotent_and_applies_object_lww() {
        let source = SyncServerService::new(
            SqliteSyncServerStore::connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        let lower_device = source
            .register_device("lower".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        let higher_device = source
            .register_device("higher".to_string(), None)
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
        let event = sample_synced_event(lower_device, "run_started");
        source
            .push_events(lower_device, vec![event.clone()])
            .await
            .unwrap();
        source
            .push_objects(
                lower_device,
                vec![sample_synced_object(
                    lower_device,
                    run_id,
                    "artifact-1",
                    "lower",
                    1,
                )],
            )
            .await
            .unwrap();
        let backup = source.export_backup().await.unwrap();

        let target = SyncServerService::new(
            SqliteSyncServerStore::connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        target
            .import_backup(backup.payload_json.clone())
            .await
            .unwrap();
        target
            .push_objects(
                higher_device,
                vec![sample_synced_object(
                    higher_device,
                    run_id,
                    "artifact-1",
                    "higher",
                    1,
                )],
            )
            .await
            .unwrap();
        target.import_backup(backup.payload_json).await.unwrap();

        let overview = target.server_overview().await.unwrap();
        assert_eq!(overview.synced_events, 1);
        assert_eq!(overview.synced_objects, 1);
        let pulled = target.pull_objects(lower_device, None, 50).await.unwrap();
        assert_eq!(pulled.objects[0].origin_device_id, higher_device);
        assert_eq!(pulled.objects[0].payload_json["title"], "higher");
    }

    #[tokio::test]
    async fn server_overview_counts_state_and_lists_latest_backup() {
        let service = SyncServerService::new(
            SqliteSyncServerStore::connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        let device = service
            .register_device("A".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        service
            .push_events(device, vec![sample_synced_event(device, "run_started")])
            .await
            .unwrap();
        let backup = service.export_backup().await.unwrap().backup;

        let overview = service.server_overview().await.unwrap();

        assert_eq!(overview.devices, 1);
        assert_eq!(overview.synced_events, 1);
        assert_eq!(overview.synced_objects, 0);
        assert_eq!(overview.latest_server_sequence, 1);
        assert_eq!(overview.latest_event_received_at.is_some(), true);
        assert_eq!(overview.latest_backup_created_at.is_some(), true);
        assert_eq!(overview.backups, vec![backup]);
    }

    #[tokio::test]
    async fn import_backup_rejects_invalid_payload_shape() {
        let service = SyncServerService::new(
            SqliteSyncServerStore::connect("sqlite::memory:")
                .await
                .unwrap(),
        );

        assert!(matches!(
            service
                .import_backup(serde_json::json!({ "devices": [] }))
                .await
                .unwrap_err(),
            SyncServerError::InvalidInput
        ));
    }

    #[tokio::test]
    async fn backup_import_preserves_backed_up_object_received_at() {
        let source = SyncServerService::new(
            SqliteSyncServerStore::connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        let device = source
            .register_device("A".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        source
            .push_objects(
                device,
                vec![sample_synced_object(
                    device,
                    RunId::new(),
                    "artifact-1",
                    "A",
                    1,
                )],
            )
            .await
            .unwrap();
        let backup = source.export_backup().await.unwrap();
        let backed_payload: BackupPayload =
            serde_json::from_value(backup.payload_json.clone()).unwrap();

        let target = SyncServerService::new(
            SqliteSyncServerStore::connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        target.import_backup(backup.payload_json).await.unwrap();

        let pulled = target.pull_objects(device, None, 50).await.unwrap();
        assert_eq!(pulled.objects, backed_payload.objects);
    }

    #[tokio::test]
    async fn backup_import_preserves_event_server_sequence_and_cursor_consistency() {
        let source = SyncServerService::new(
            SqliteSyncServerStore::connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        let device = source
            .register_device("A".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        source
            .push_events(device, vec![sample_synced_event(device, "run_started")])
            .await
            .unwrap();
        source.ack_events(device, 42).await.unwrap();
        let mut backup = source.export_backup().await.unwrap().payload_json;
        backup["events"][0]["server_sequence"] = serde_json::json!(42);
        backup["cursors"][0]["last_server_sequence"] = serde_json::json!(42);

        let target = SyncServerService::new(
            SqliteSyncServerStore::connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        target.import_backup(backup).await.unwrap();

        let events = target.list_events().await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].server_sequence, 42);
        let overview = target.server_overview().await.unwrap();
        assert_eq!(overview.latest_server_sequence, 42);
        let cursors = target.list_cursors().await.unwrap();
        assert_eq!(cursors[0].last_server_sequence, 42);
    }

    #[tokio::test]
    async fn backup_import_rolls_back_all_rows_when_object_import_fails() {
        let source = SyncServerService::new(
            SqliteSyncServerStore::connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        let device = source
            .register_device("A".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        source
            .push_events(device, vec![sample_synced_event(device, "run_started")])
            .await
            .unwrap();
        source
            .push_objects(
                device,
                vec![sample_synced_object(
                    device,
                    RunId::new(),
                    "artifact-1",
                    "A",
                    1,
                )],
            )
            .await
            .unwrap();
        let mut backup = source.export_backup().await.unwrap().payload_json;
        backup["objects"][0]["origin_device_id"] = serde_json::json!(DeviceId::new().to_string());

        let target = SyncServerService::new(
            SqliteSyncServerStore::connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        assert!(target.import_backup(backup).await.is_err());

        let overview = target.server_overview().await.unwrap();
        assert_eq!(overview.devices, 0);
        assert_eq!(overview.synced_events, 0);
        assert_eq!(overview.synced_objects, 0);
        assert_eq!(overview.backups.len(), 0);
        assert_eq!(overview.latest_backup_created_at, None);
    }

    #[tokio::test]
    async fn backup_import_rejects_server_sequence_collision_and_rolls_back() {
        let source = SyncServerService::new(
            SqliteSyncServerStore::connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        let source_device = source
            .register_device("source".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        source
            .push_events(
                source_device,
                vec![sample_synced_event(source_device, "source_event")],
            )
            .await
            .unwrap();
        source.ack_events(source_device, 1).await.unwrap();
        source
            .push_objects(
                source_device,
                vec![sample_synced_object(
                    source_device,
                    RunId::new(),
                    "artifact-from-backup",
                    "backup object",
                    1,
                )],
            )
            .await
            .unwrap();
        let backup = source.export_backup().await.unwrap().payload_json;
        let payload: BackupPayload = serde_json::from_value(backup.clone()).unwrap();
        let imported_event_id = payload.events[0].event_id;

        let target = SyncServerService::new(
            SqliteSyncServerStore::connect("sqlite::memory:")
                .await
                .unwrap(),
        );
        let target_device = target
            .register_device("target".to_string(), None)
            .await
            .unwrap()
            .device
            .device_id;
        target
            .push_events(
                target_device,
                vec![sample_synced_event(target_device, "existing_event")],
            )
            .await
            .unwrap();
        target.ack_events(target_device, 0).await.unwrap();

        assert_ne!(
            imported_event_id,
            target.list_events().await.unwrap()[0].event_id
        );
        assert!(target.import_backup(backup).await.is_err());

        let overview = target.server_overview().await.unwrap();
        assert_eq!(overview.devices, 1);
        assert_eq!(overview.synced_events, 1);
        assert_eq!(overview.synced_objects, 0);
        assert_eq!(overview.backups.len(), 0);
        assert_eq!(overview.latest_server_sequence, 1);
        assert_eq!(overview.latest_backup_created_at, None);
        let cursors = target.list_cursors().await.unwrap();
        assert_eq!(cursors.len(), 1);
        assert_eq!(cursors[0].device_id, target_device);
        assert_eq!(cursors[0].last_server_sequence, 0);
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
