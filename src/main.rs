mod cli;
mod app;
mod nmea;
mod net;
mod dds_types;

use anyhow::Result;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // Init logs with env override (RUST_LOG) and -v/-q from CLI
    let args = cli::Cli::parse();
    let level = args.log_level();
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .compact()
        .init();

    tracing::info!("Starting nmea-capture…");

    app::run(args).await?;

    tracing::info!("Bye!");
    Ok(())
}
