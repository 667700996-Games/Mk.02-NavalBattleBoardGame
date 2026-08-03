use mk01_server::{AppState, build_router, config::Settings};
use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    let settings = Settings::from_env().unwrap_or_else(|error| {
        eprintln!("configuration error: {error}");
        std::process::exit(2);
    });
    init_tracing();
    let state = AppState::new(settings.clone()).await.unwrap_or_else(|error| {
        tracing::error!(%error, "server initialization failed");
        std::process::exit(1);
    });
    let app = build_router(state);
    let listener = TcpListener::bind(settings.bind_addr).await.unwrap_or_else(|error| {
        tracing::error!(%error, address = %settings.bind_addr, "failed to bind server");
        std::process::exit(1);
    });
    tracing::info!(address = %settings.bind_addr, "Mk.01 command server online");
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await.unwrap();
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("mk01_server=info,tower_http=info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json())
        .init();
}

async fn shutdown_signal() {
    let ctrl_c = async { tokio::signal::ctrl_c().await.expect("ctrl-c handler") };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("signal handler").recv().await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
