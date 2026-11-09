use mneme_api::build_router;
use mneme_kernel::InMemoryOperationLedger;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

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

    // 第一阶段使用内存账本；持久化 SQLite 会在下一阶段替换这里。
    let ledger = InMemoryOperationLedger::new();
    let app = build_router(ledger);
    let listener = TcpListener::bind(addr).await?;

    info!(%addr, "mneme daemon listening");
    axum::serve(listener, app).await?;

    Ok(())
}
