use mneme_server::{routes::build_router, SqliteSyncServerStore, SyncServerService};
use std::{fs, net::SocketAddr, path::Path};
use tokio::net::TcpListener;
use tracing::info;

const DEFAULT_DATABASE_URL: &str = "sqlite://.mneme/server.db?mode=rwc";
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3866";

fn database_url_from_env() -> String {
    std::env::var("MNEME_SERVER_DB_URL").unwrap_or_else(|_| DEFAULT_DATABASE_URL.to_string())
}

fn bind_addr_from_env() -> Result<SocketAddr, std::net::AddrParseError> {
    std::env::var("MNEME_SERVER_BIND_ADDR")
        .unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string())
        .parse()
}

fn ensure_sqlite_parent_directory(database_url: &str) -> std::io::Result<()> {
    if database_url == "sqlite::memory:" {
        return Ok(());
    }

    let Some(path_with_query) = database_url.strip_prefix("sqlite://") else {
        return Ok(());
    };
    let database_path = path_with_query.split('?').next().unwrap_or(path_with_query);
    let Some(parent) = Path::new(database_path).parent() else {
        return Ok(());
    };

    if parent.as_os_str().is_empty() {
        return Ok(());
    }

    fs::create_dir_all(parent)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mneme_server=info,tower_http=info".into()),
        )
        .init();

    let database_url = database_url_from_env();
    ensure_sqlite_parent_directory(&database_url)?;
    let store = SqliteSyncServerStore::connect(&database_url).await?;
    let service = SyncServerService::new(store);
    let app = build_router(service);
    let addr = bind_addr_from_env()?;
    let listener = TcpListener::bind(addr).await?;

    info!(%addr, database_url = %database_url, "mneme sync server listening");
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_url_defaults_to_local_server_database() {
        assert_eq!(DEFAULT_DATABASE_URL, "sqlite://.mneme/server.db?mode=rwc");
    }

    #[test]
    fn bind_addr_defaults_to_local_sync_server_port() {
        assert_eq!(DEFAULT_BIND_ADDR, "127.0.0.1:3866");
    }

    #[test]
    fn ensure_sqlite_parent_directory_ignores_memory_database() {
        ensure_sqlite_parent_directory("sqlite::memory:").unwrap();
    }
}
