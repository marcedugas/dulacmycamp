use dulacmycamp_api::{Config, build_state, router};
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    fmt()
        .with_env_filter(
            EnvFilter::try_from_env("LOG_LEVEL")
                .unwrap_or_else(|_| EnvFilter::new("info,dulacmycamp_api=debug")),
        )
        .init();

    let cfg = Config::from_env()?;
    let addr = cfg.bind_addr.clone();
    tracing::info!(%addr, frontend = %cfg.frontend_url, "starting dulacmycamp-api");

    let state = build_state(cfg).await?;
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on {}", listener.local_addr()?);

    // `into_make_service_with_connect_info` makes the peer address available
    // to handlers, which the rate limiter falls back to when there is no
    // X-Forwarded-For header (i.e. running without a proxy in front).
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
