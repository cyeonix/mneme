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
}

impl SyncServerStore for SqliteSyncServerStore {
    fn pool(&self) -> &sqlx::SqlitePool {
        self.pool()
    }
}
