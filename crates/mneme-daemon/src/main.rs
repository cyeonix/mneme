use mneme_api::routes::build_router_with_service;
use mneme_kernel::{RunService, SqliteRunService, SqliteStore};
use std::{fs, net::SocketAddr, path::Path};
use tokio::net::TcpListener;
use tracing::info;

const DEFAULT_DATABASE_URL: &str = "sqlite://.mneme/mneme.db?mode=rwc";

fn database_url_from_env() -> String {
    database_url_from_env_value(std::env::var("MNEME_DB_URL").ok())
}

fn database_url_from_env_value(value: Option<String>) -> String {
    value.unwrap_or_else(|| DEFAULT_DATABASE_URL.to_string())
}

fn rebuild_runs_on_start() -> bool {
    rebuild_runs_on_start_value(std::env::var("MNEME_REBUILD_RUNS_ON_START").ok())
}

fn rebuild_runs_on_start_value(value: Option<String>) -> bool {
    matches!(value.as_deref(), Some("1") | Some("true") | Some("yes"))
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
                .unwrap_or_else(|_| "mneme_daemon=info,tower_http=info".into()),
        )
        .init();

    // 默认只绑定本机地址，避免第一阶段误暴露到局域网。
    let addr: SocketAddr = std::env::var("MNEME_BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3766".to_string())
        .parse()?;

    let database_url = database_url_from_env();
    ensure_sqlite_parent_directory(&database_url)?;
    let store = SqliteStore::connect(&database_url).await?;
    let service = SqliteRunService::new(store);
    if rebuild_runs_on_start() {
        let rebuilt = service.rebuild_run_projections().await?;
        info!(
            run_count = rebuilt.len(),
            "rebuilt run projections from operation ledger"
        );
    }
    let app = build_router_with_service(service.shared());
    let listener = TcpListener::bind(addr).await?;

    info!(%addr, database_url = %database_url, "mneme daemon listening");
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_runs_on_start_defaults_to_false() {
        assert!(!rebuild_runs_on_start_value(None));
    }

    #[test]
    fn rebuild_runs_on_start_accepts_one_true_and_yes() {
        assert!(rebuild_runs_on_start_value(Some("1".to_string())));
        assert!(rebuild_runs_on_start_value(Some("true".to_string())));
        assert!(rebuild_runs_on_start_value(Some("yes".to_string())));
    }

    #[test]
    fn rebuild_runs_on_start_rejects_other_values() {
        assert!(!rebuild_runs_on_start_value(Some("0".to_string())));
        assert!(!rebuild_runs_on_start_value(Some("false".to_string())));
    }

    #[test]
    fn default_database_url_uses_local_mneme_directory() {
        assert_eq!(
            database_url_from_env_value(None),
            "sqlite://.mneme/mneme.db?mode=rwc"
        );
    }

    #[test]
    fn database_url_from_env_uses_override() {
        assert_eq!(
            database_url_from_env_value(Some("sqlite://.mneme-test/mneme.db?mode=rwc".to_string())),
            "sqlite://.mneme-test/mneme.db?mode=rwc"
        );
    }

    #[test]
    fn ensure_sqlite_parent_directory_creates_relative_parent() {
        let base = std::env::temp_dir().join(format!("mneme-daemon-test-{}", std::process::id()));
        let database_url = format!("sqlite://{}/.mneme-test/mneme.db?mode=rwc", base.display());
        let expected_parent = base.join(".mneme-test");

        if base.exists() {
            fs::remove_dir_all(&base).expect("remove stale temp dir");
        }
        ensure_sqlite_parent_directory(&database_url).expect("create sqlite parent directory");

        assert!(expected_parent.is_dir());

        fs::remove_dir_all(&base).expect("remove temp dir");
    }

    #[test]
    fn ensure_sqlite_parent_directory_skips_memory_database() {
        ensure_sqlite_parent_directory("sqlite::memory:")
            .expect("memory sqlite url does not need dir");
    }
}
