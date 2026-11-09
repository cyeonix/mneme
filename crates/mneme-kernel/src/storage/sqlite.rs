use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn connect(database_url: &str) -> Result<Self, SqliteStoreError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[derive(Debug, Error)]
pub enum SqliteStoreError {
    #[error("sqlite connection failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("sqlite migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sqlite_store_runs_migrations() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        let count: i64 = sqlx::query_scalar(
            "select count(*) from sqlite_master where type = 'table' and name = 'operation_events'",
        )
        .fetch_one(store.pool())
        .await
        .unwrap();

        assert_eq!(count, 1);
    }
}
