//! Binario principal `nmea-capture`.
//! - Parseo de CLI.
//! - Configuración de logging con `tracing_subscriber` (respeta RUST_LOG y -v/-q).
//! - Delegación a `app::run` que contiene toda la lógica.

mod cli;
mod app;
mod nmea;
mod net;
mod dds_types;

use anyhow::Result;
use tracing_subscriber::{fmt, EnvFilter};


/// Punto de entrada asíncrono (Tokio multihilo).
/// - 1) Lee y normaliza flags de CLI.
/// - 2) Inicializa el subsistema de logs (`tracing`) con nivel calculado.
/// - 3) Llama a `app::run(args)` y reporta "Bye!" al finalizar (Ctrl+C).

#[tokio::main]
async fn main() -> Result<()> {
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
