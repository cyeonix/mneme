use crate::storage::SqliteSyncServerStore;
use mneme_kernel::{DeviceId, DeviceRecord, SyncCursorRecord};
use std::str::FromStr;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

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
        let result = sqlx::query(
            "update sync_cursors set last_server_sequence = ?1, updated_at = ?2 where device_id = ?3",
        )
        .bind(last_seen_server_sequence)
        .bind(&now_text)
        .bind(device_id.to_string())
        .execute(self.store.pool())
        .await
        .map_err(storage_error)?;

        if result.rows_affected() == 0 {
            return Err(SyncServerError::UnknownDevice);
        }

        let device_seen_result =
            sqlx::query("update devices set last_seen_at = ?1 where device_id = ?2")
                .bind(&now_text)
                .bind(device_id.to_string())
                .execute(self.store.pool())
                .await
                .map_err(storage_error)?;

        if device_seen_result.rows_affected() == 0 {
            return Err(SyncServerError::UnknownDevice);
        }

        Ok(SyncCursorRecord {
            device_id,
            last_server_sequence: last_seen_server_sequence,
            updated_at: now,
        })
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

fn non_blank(value: String) -> Result<String, SyncServerError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(SyncServerError::InvalidInput);
    }
    Ok(trimmed.to_string())
}

fn format_datetime(value: OffsetDateTime) -> Result<String, SyncServerError> {
    value.format(&Rfc3339).map_err(storage_error)
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
