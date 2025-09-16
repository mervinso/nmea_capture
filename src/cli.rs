use clap::{Parser, ValueEnum};

/// NMEA UDP capture (v1)
#[derive(Debug, Parser, Clone)]
#[command(name = "nmea-capture")]
#[command(about = "Capture NMEA 0183 sentences over UDP and print them")]
pub struct Cli {
    /// Bind address to listen on (IP:PORT)
    /// For your setup with NemaStudio: 0.0.0.0:1100
    #[arg(long, default_value = "0.0.0.0:1100")]
    pub bind: String,

    /// UDP mode (kept for future multicast; v1 uses unicast)
    #[arg(long, value_enum, default_value_t = Mode::Unicast)]
    pub mode: Mode,

    /// Add timestamp before each printed line (off by default)
    #[arg(long)]
    pub timestamp: bool,

    /// Verbose (-v, -vv) or quiet (-q) logging
    #[arg(short = 'v', action = clap::ArgAction::Count)]
    verbose: u8,

    /// Quiet mode (overrides -v)
    #[arg(short = 'q', action)]
    quiet: bool,

    /// Publicar también a DDS (NMEA/Raw)
    #[arg(long)]
    pub dds: bool,

    /// Domain ID para DDS
    #[arg(long, default_value_t = 0)]
    pub dds_domain: u16,

    /// Tópico DDS para Raw
    #[arg(long, default_value = "NMEA/Raw")]
    pub dds_topic_raw: String,

    /// Salida por consola en JSON (NDJSON: una línea por objeto)
    #[arg(long)]
    pub json: bool,

    /// JSON bonito (multi-linea). Requiere --json.
    #[arg(long, requires = "json")]
    pub json_pretty: bool,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum Mode {
    Unicast,
    Multicast,
}

impl Cli {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }

    /// Compute tracing level string for EnvFilter
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
