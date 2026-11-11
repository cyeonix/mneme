pub trait SyncServerStore: Clone + Send + Sync + 'static {
    fn pool(&self) -> &sqlx::SqlitePool;
}

#[derive(Clone)]
pub struct SqliteSyncServerStore {
    pool: sqlx::SqlitePool,
}

impl SqliteSyncServerStore {
    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = sqlx::SqlitePool::connect(database_url).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    pub async fn has_device(&self, device_id: &str) -> Result<bool, sqlx::Error> {
        let count =
            sqlx::query_scalar::<_, i64>("select count(1) from devices where device_id = ?1")
                .bind(device_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(count > 0)
    }

    pub async fn latest_server_sequence(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, Option<i64>>("select max(server_sequence) from synced_events")
            .fetch_one(&self.pool)
            .await
            .map(|value| value.unwrap_or(0))
    }
}

impl SyncServerStore for SqliteSyncServerStore {
    fn pool(&self) -> &sqlx::SqlitePool {
        self.pool()
    }
}
