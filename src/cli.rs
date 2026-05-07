//! Definición de la interfaz de línea de comandos (CLI) con `clap`.
//! Mantén aquí **nombres y descripciones** porque la ayuda (`--help`) se
//! genera automáticamente a partir de estos atributos.

use clap::{Parser, ValueEnum};

/// NMEA UDP capture (v1)
#[derive(Debug, Parser, Clone)]
#[command(name = "nmea-capture")]
#[command(about = "Capture NMEA 0183 sentences over UDP and print them")]
pub struct Cli {
    /// Dirección de enlace UDP (IP:PORT) donde escuchará el socket.
    /// Para NemaStudio: `0.0.0.0:1100` (recibe desde localhost y la LAN).
    #[arg(long, default_value = "0.0.0.0:1100")]
    pub bind: String,

    /// Modo de recepción. En v1 solo se usa `Unicast`.
    /// Se deja `Multicast` para futuras extensiones (join a grupo IGMP).
    #[arg(long, value_enum, default_value_t = Mode::Unicast)]
    pub mode: Mode,

    /// Si está presente, antepone timestamp en la impresión de cada línea.
    /// Útil cuando no se usa `--json`.
    #[arg(long)]
    pub timestamp: bool,

    /// Verbosidad de logs:
    /// - sin `-v` → info
    /// - `-v` → debug
    /// - `-vv` → trace
    #[arg(short = 'v', action = clap::ArgAction::Count)]
    verbose: u8,

    /// `-q` (quiet) domina y fija `warn`.
    #[arg(short = 'q', action)]
    quiet: bool,

    /// Habilita publicación a DustDDS:
    /// - Crea participante, tópico `NMEA/Raw` y publicador/escritor.
    /// - Cada línea capturada se envía como `RawSentence`.
    #[arg(long)]
    pub dds: bool,

    /// DomId de DDS (por defecto 0). PUBLICADOR y SUSCRIPTOR deben coincidir.
    #[arg(long, default_value_t = 0)]
    pub dds_domain: u16,

    /// Nombre del tópico de DDS donde se publican las frases crudas NMEA.
    /// Debe coincidir con el lector del subscriber.
    #[arg(long, default_value = "NMEA/Raw")]
    pub dds_topic_raw: String,

    /// Salida por consola en **NDJSON** (un JSON por línea).
    /// Facilita piping: `... | jq`.
    #[arg(long)]
    pub json: bool,

    /// Versión "bonita" multilínea del JSON (no recomendada para piping).
    #[arg(long, requires = "json")]
    pub json_pretty: bool,
}

/// Método de ayuda para traducir `-v/-q` a filtro de `tracing`.
/// Se usa en `main.rs` para inicializar `EnvFilter`.
#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum Mode {
    Unicast,
    Multicast,
}

impl Cli {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }

    pub fn log_level(&self) -> String {
        if self.quiet {
            "warn".into()
        } else {
            match self.verbose {
                0 => "info".into(),
                1 => "debug".into(),
                _ => "trace".into(),
            }
        }
    }
}
