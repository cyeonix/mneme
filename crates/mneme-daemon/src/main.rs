use mneme_api::routes::{
    build_router_with_all_services, SharedArtifactService, SharedChunkService,
    SharedContextService, SharedKnowledgeService, SharedMemoryService, SharedRetrievalService,
};
use mneme_kernel::{
    ChunkTextSplitter, InMemoryArtifactContentStore, OpenAiCompatibleProviderWorker,
    OpenAiProviderConfig, ReqwestOpenAiHttpClient, RunService, SqliteArtifactService,
    SqliteChunkService, SqliteContextService, SqliteKnowledgeService, SqliteMemoryService,
    SqliteRetrievalService, SqliteRunService, SqliteStore, SqliteWorkerRuntimeService, WorkerId,
};
use std::{fs, net::SocketAddr, path::Path, sync::Arc};
use tokio::net::TcpListener;
use tracing::info;

const DEFAULT_DATABASE_URL: &str = "sqlite://.mneme/mneme.db?mode=rwc";
const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";
const DEFAULT_OPENAI_TIMEOUT_SECS: u64 = 30;

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

fn openai_provider_config_from_env() -> OpenAiProviderConfig {
    openai_provider_config_from_env_values(
        std::env::var("MNEME_OPENAI_BASE_URL").ok(),
        std::env::var("MNEME_OPENAI_API_KEY").ok(),
        std::env::var("MNEME_OPENAI_MODEL").ok(),
        std::env::var("MNEME_OPENAI_TIMEOUT_SECS").ok(),
    )
}

fn openai_provider_config_from_env_values(
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    timeout_secs: Option<String>,
) -> OpenAiProviderConfig {
    OpenAiProviderConfig {
        base_url: trimmed_or_default(base_url, DEFAULT_OPENAI_BASE_URL),
        api_key: api_key.and_then(|key| {
            let trimmed = key.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }),
        model: trimmed_or_default(model, DEFAULT_OPENAI_MODEL),
        timeout_secs: timeout_secs
            .as_deref()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_OPENAI_TIMEOUT_SECS),
    }
}

fn trimmed_or_default(value: Option<String>, default: &str) -> String {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(default)
        .to_string()
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
    let service = SqliteRunService::new(store.clone());
    let artifact_service =
        SqliteArtifactService::new(store.clone(), InMemoryArtifactContentStore::new().shared());
    let context_service = SqliteContextService::new(store.clone());
    let knowledge_service = SqliteKnowledgeService::new(store.clone());
    let chunk_service = SqliteChunkService::new(store.clone(), ChunkTextSplitter::new(4096, 256));
    let memory_service = SqliteMemoryService::new(store.clone());
    let retrieval_service = SqliteRetrievalService::new(store.clone());
    let openai_provider_config = openai_provider_config_from_env();
    let openai_model = openai_provider_config.model.clone();
    let provider = Arc::new(OpenAiCompatibleProviderWorker::new(
        WorkerId::new(),
        openai_provider_config,
        ReqwestOpenAiHttpClient::new().shared(),
    ));
    let worker_runtime_service =
        SqliteWorkerRuntimeService::new(store, provider, openai_model).shared();
    if rebuild_runs_on_start() {
        let rebuilt = service.rebuild_run_projections().await?;
        info!(
            run_count = rebuilt.len(),
            "rebuilt run projections from operation ledger"
        );
    }
    let app = build_router_with_all_services(
        service.shared(),
        SharedArtifactService::Sqlite(artifact_service),
        SharedContextService::Sqlite(context_service),
        SharedKnowledgeService::Sqlite(knowledge_service),
        SharedChunkService::Sqlite(chunk_service),
        SharedMemoryService::Sqlite(memory_service),
        SharedRetrievalService::Sqlite(retrieval_service),
        worker_runtime_service,
    );
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
    fn openai_provider_config_uses_defaults_without_api_key() {
        let config = openai_provider_config_from_env_values(None, None, None, None);
        assert_eq!(config.base_url, "https://api.openai.com/v1");
        assert_eq!(config.model, "gpt-4o-mini");
        assert_eq!(config.timeout_secs, 30);
        assert!(config.api_key.is_none());
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
