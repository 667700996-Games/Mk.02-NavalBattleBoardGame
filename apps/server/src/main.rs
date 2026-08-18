use std::net::SocketAddr;

use mk01_server::{
    AppState, build_router,
    config::{Settings, StorageMode},
    store::PostgresRedisStore,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    let arguments: Vec<_> = std::env::args().collect();
    let settings = Settings::from_env().unwrap_or_else(|error| {
        eprintln!("configuration error: {error}");
        std::process::exit(2);
    });
    if arguments.iter().any(|argument| argument == "--healthcheck") {
        std::process::exit(if healthcheck(settings.bind_addr.port()).await {
            0
        } else {
            1
        });
    }
    init_tracing();
    if arguments
        .iter()
        .any(|argument| argument == "--migrate-only")
    {
        require_postgres(&settings);
        PostgresRedisStore::migrate_database(&settings.database_url)
            .await
            .unwrap_or_else(|error| {
                tracing::error!(%error, "database migration command failed");
                std::process::exit(1);
            });
        tracing::info!("database migrations applied");
        return;
    }
    if arguments
        .iter()
        .any(|argument| argument == "--verify-restore")
    {
        require_postgres(&settings);
        let report = PostgresRedisStore::verify_database(&settings.database_url)
            .await
            .unwrap_or_else(|error| {
                tracing::error!(%error, "restored database verification failed");
                std::process::exit(1);
            });
        println!(
            "{}",
            serde_json::to_string(&report).expect("verification report must serialize")
        );
        return;
    }
    let state = AppState::new(settings.clone())
        .await
        .unwrap_or_else(|error| {
            tracing::error!(%error, "server initialization failed");
            std::process::exit(1);
        });
    let app = build_router(state);
    let listener = TcpListener::bind(settings.bind_addr)
        .await
        .unwrap_or_else(|error| {
            tracing::error!(%error, address = %settings.bind_addr, "failed to bind server");
            std::process::exit(1);
        });
    tracing::info!(address = %settings.bind_addr, "Mk.01 command server online");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .unwrap();
}

fn require_postgres(settings: &Settings) {
    if settings.storage_mode != StorageMode::Postgres {
        eprintln!("database maintenance commands require STORAGE_MODE=postgres");
        std::process::exit(2);
    }
}

async fn healthcheck(port: u16) -> bool {
    let check = async move {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.ok()?;
        stream
            .write_all(b"GET /api/ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .ok()?;
        let mut response = Vec::with_capacity(512);
        stream.read_to_end(&mut response).await.ok()?;
        String::from_utf8(response)
            .ok()?
            .starts_with("HTTP/1.1 200")
            .then_some(())
    };
    tokio::time::timeout(std::time::Duration::from_secs(3), check)
        .await
        .ok()
        .flatten()
        .is_some()
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("mk01_server=info,tower_http=info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json())
        .init();
}

async fn shutdown_signal() {
    let ctrl_c = async { tokio::signal::ctrl_c().await.expect("ctrl-c handler") };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("signal handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
