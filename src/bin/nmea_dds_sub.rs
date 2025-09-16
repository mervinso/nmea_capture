//! Segundo binario: **suscriptor DDS** de `NMEA/Raw`.
/*!
 Flujo:
   - Se suscribe a `RawSentence`.
   - Imprime cada muestra como NDJSON (`type="raw"`).
   - Si `sentence` es `"RMC"` o `"GGA"`, parsea el campo `raw` y
     publica objetos tipados en `NMEA/RMC` o `NMEA/GGA`.
  Notas:
   - El parseo es **tolerante**; convierte vacíos a `NaN` y usa `Option` implícita
     (vía `NaN`→`null` en JSON) para campos no presentes.
   - Todo corre con Tokio y APIs `*_async` de DustDDS.
*/

use clap::Parser;
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};
use tracing::info;

use dust_dds::{
    dds_async::domain_participant_factory::DomainParticipantFactoryAsync,
    infrastructure::{
        error::{DdsError, DdsResult},
        qos::QosKind,
        status::NO_STATUS,
    },
    topic_definition::type_support::DdsType,
};

//// Tipo tipado para **RMC** (Recommended Minimum Specific GNSS Data).
/// `#[dust_dds(key)] seq` actúa como clave de instancia en DDS.
/// Guardamos además `ts_unix_ms` del momento de recepción del raw.
#[path = "../dds_types.rs"]
mod dds_types;
use dds_types::RawSentence;

/// Tipo tipado para **GGA** (Global Positioning System Fix Data).
/// Incluye calidad de fix, nº de satélites y alturas. `seq` es la clave DDS.
#[derive(Debug, Clone, DdsType)]
pub struct Rmc {
    #[dust_dds(key)]
    pub seq: u64,
    pub utc_time: String,
    pub utc_date: String,
    pub status: char,
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub sog_knots: f32,
    pub cog_deg: f32,
    pub mag_var: f32,
    pub mag_var_ew: char,
    pub mode: String,
    pub ts_unix_ms: i64,
}

#[derive(Debug, Clone, DdsType)]
pub struct Gga {
    #[dust_dds(key)]
    pub seq: u64,
    pub utc_time: String,
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub fix_quality: u16,
    pub num_sats: u16,
    pub hdop: f32,
    pub altitude_m: f32,
    pub geoid_sep_m: f32,
    pub ts_unix_ms: i64,
}

/// CLI del subscriber:
/// - `--domain`: DomainId compartido con el publicador.
/// - `--topic-raw`: tópico del que se lee (`RawSentence`).
/// - `--topic-rmc`, `--topic-gga`: tópicos a los que se republica parseado.
#[derive(Debug, Parser)]
#[command(name = "nmea-dds-sub", about = "Subscribe NMEA/Raw, print NDJSON, republish RMC/GGA")]
struct Cli {
    #[arg(long, default_value_t = 0)]
    domain: i32,
    #[arg(long, default_value = "NMEA/Raw")]
    topic_raw: String,
    #[arg(long, default_value = "NMEA/RMC")]
    topic_rmc: String,
    #[arg(long, default_value = "NMEA/GGA")]
    topic_gga: String,
}

/// Igual que en el capturador: inicialización segura de WinSock una única vez.
/// Previene el error "WSAStartup failed" en sistemas donde aún no se inició.
#[cfg(windows)]
fn ensure_winsock() {
    use std::sync::Once;
    use windows_sys::Win32::Networking::WinSock::{WSAStartup, WSADATA};
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        let mut data = std::mem::MaybeUninit::<WSADATA>::uninit();
        let rc = WSAStartup(0x0202u16, data.as_mut_ptr());
        assert_eq!(rc, 0, "WSAStartup failed: {}", rc);
    });
}

/// Convierte `f64::NAN` → `null` al serializar a JSON.
/// Así evitamos meter números inválidos en el NDJSON.
/// Lo mismo para `f32`.
fn f64_or_null(v: f64) -> Value {
    if v.is_nan() { Value::Null } else { json!(v) }
}
fn f32_or_null(v: f32) -> Value {
    if v.is_nan() { Value::Null } else { json!(v) }
}

/// Obtiene el **vector de campos** (trozos separados por comas) que vienen
/// **después** de `"$..RMC,"` / `"$..GGA,"`, ignorando el `*checksum` final.
/// Devuelve `None` si la línea no contiene al menos una coma.
// Corta lo que viene después de la primera coma tras $**RMC / $**GGA
fn fields_after_first_comma(line: &str) -> Option<Vec<&str>> {
    let body = line.split_once('*').map(|(b, _)| b).unwrap_or(line);
    let body = body.trim_start_matches(['$', '!']);
    let (_, rest) = body.split_once(',')?; // elimina talker+mnemonic y coma
    Some(rest.split(',').collect())
}

/// Divide la cadena `ddmm.mmmm` en:
/// - grados (`dd` o `ddd` según lat/lon)
/// - minutos con decimales (`mm.mmmm`)
/// Si no hay punto decimal, devuelve `NaN` para minutos.
fn split_deg_min(v: &str) -> (u32, f64) {
    if let Some(idx) = v.find('.') {
        let mm_start = idx.saturating_sub(2);
        let (d, m) = v.split_at(mm_start);
        (d.parse().unwrap_or(0), m.parse().unwrap_or(f64::NAN))
    } else {
        (0, f64::NAN)
    }
}

/// Convierte latitud NMEA (`ddmm.mmmm` + hemisferio `N/S`) → grados decimales.
/// Aplica signo negativo si hemisferio = 'S'.
fn parse_lat(v: &str, hemi: &str) -> f64 {
    if v.is_empty() { return f64::NAN; }
    let (deg, min) = split_deg_min(v);
    let mut d = deg as f64 + min / 60.0;
    if matches!(hemi.chars().next(), Some('S')) { d = -d; }
    d
}

/// Parseo **tolerante** de una línea RMC:
/// Campos esperados (mínimo 10): time,status,lat,NS,lon,EW,sog,cog,date,magvar,[magE/W],[mode]
/// - Convierte ausencias a `NaN`/`'\0'`/`""`.
/// - Incrementa `seq` y devuelve `Rmc` listo para publicar.
fn parse_lon(v: &str, hemi: &str) -> f64 {
    if v.is_empty() { return f64::NAN; }
    let (deg, min) = split_deg_min(v);
    let mut d = deg as f64 + min / 60.0;
    if matches!(hemi.chars().next(), Some('W')) { d = -d; }
    d
}

/// Parseo **tolerante** de una línea GGA:
/// Campos esperados (mínimo 11): time,lat,NS,lon,EW,fix,numsats,hdop,alt,M,geoid,M,...
/// - Convierte ausencias a `NaN`/0.
/// - Incrementa `seq` y devuelve `Gga`.
fn parse_rmc(line: &str, ts_ms: i64, seq: &mut u64) -> Option<Rmc> {
    let f = fields_after_first_comma(line)?;
    // time,status,lat,NS,lon,EW,sog,cog,date,magvar,magE/W,mode?
    if f.len() < 10 { return None; }

    let utc_time  = f[0].to_string();
    let status    = f.get(1).and_then(|s| s.chars().next()).unwrap_or('V');
    let lat_deg   = parse_lat(f.get(2).copied().unwrap_or(""), f.get(3).copied().unwrap_or(""));
    let lon_deg   = parse_lon(f.get(4).copied().unwrap_or(""), f.get(5).copied().unwrap_or(""));
    let sog_knots = f.get(6).and_then(|s| s.parse::<f32>().ok()).unwrap_or(f32::NAN);
    let cog_deg   = f.get(7).and_then(|s| s.parse::<f32>().ok()).unwrap_or(f32::NAN);
    let utc_date  = f.get(8).copied().unwrap_or("").to_string();
    let mag_var   = f.get(9).and_then(|s| s.parse::<f32>().ok()).unwrap_or(f32::NAN);
    let mag_var_ew= f.get(10).and_then(|s| s.chars().next()).unwrap_or('\0');
    let mode_idx  = if f.len() > 12 { 12 } else { 11 };
    let mode      = f.get(mode_idx).copied().unwrap_or("").to_string();

    *seq += 1;
    Some(Rmc {
        seq: *seq,
        utc_time,
        utc_date,
        status,
        lat_deg,
        lon_deg,
        sog_knots,
        cog_deg,
        mag_var,
        mag_var_ew,
        mode,
        ts_unix_ms: ts_ms,
    })
}

fn parse_gga(line: &str, ts_ms: i64, seq: &mut u64) -> Option<Gga> {
    let f = fields_after_first_comma(line)?;
    // time,lat,NS,lon,EW,fix,numsats,hdop,alt,M,geoid,M,…
    if f.len() < 11 { return None; }

    let utc_time    = f[0].to_string();
    let lat_deg     = parse_lat(f[1], f.get(2).copied().unwrap_or(""));
    let lon_deg     = parse_lon(f[3], f.get(4).copied().unwrap_or(""));
    let fix_quality = f.get(5).and_then(|s| s.parse::<u16>().ok()).unwrap_or(0);
    let num_sats    = f.get(6).and_then(|s| s.parse::<u16>().ok()).unwrap_or(0);
    let hdop        = f.get(7).and_then(|s| s.parse::<f32>().ok()).unwrap_or(f32::NAN);
    let altitude_m  = f.get(8).and_then(|s| s.parse::<f32>().ok()).unwrap_or(f32::NAN);
    let geoid_sep_m = f.get(10).and_then(|s| s.parse::<f32>().ok()).unwrap_or(f32::NAN);

    *seq += 1;
    Some(Gga {
        seq: *seq,
        utc_time,
        lat_deg,
        lon_deg,
        fix_quality,
        num_sats,
        hdop,
        altitude_m,
        geoid_sep_m,
        ts_unix_ms: ts_ms,
    })
}

/// 1) Inicializa logs (`RUST_LOG` o `info` por defecto).
/// 2) Crea participante y los **tres** tópicos: Raw (reader) + RMC/GGA (writers).
/// 3) Bucle principal:
///    - `read_next_sample().await` (maneja `NoData` con sleep corto).
///    - Imprime NDJSON `{"type":"raw",...}`.
///    - Si `sentence` es `"RMC"` o `"GGA"`, parsea y **publica** tipado + imprime NDJSON.
/// 4) No se hace `join` a multicast (se deja a DustDDS), así que los logs 10049
///    por interfaces APIPA son informativos y no afectan el flujo unicast.
#[tokio::main]
async fn main() -> DdsResult<()> {
    #[cfg(windows)]
    ensure_winsock();

    use tracing_subscriber::{fmt, EnvFilter};
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .compact()
        .init();

    let args = Cli::parse();

    let dpf = DomainParticipantFactoryAsync::get_instance();
    let participant =
        dpf.create_participant(args.domain, QosKind::Default, None, NO_STATUS).await?;

    // Topics
    let topic_raw = participant
        .create_topic::<RawSentence>(&args.topic_raw, "RawSentence", QosKind::Default, None, NO_STATUS)
        .await?;
    let topic_rmc = participant
        .create_topic::<Rmc>(&args.topic_rmc, "Rmc", QosKind::Default, None, NO_STATUS)
        .await?;
    let topic_gga = participant
        .create_topic::<Gga>(&args.topic_gga, "Gga", QosKind::Default, None, NO_STATUS)
        .await?;

    // Sub + reader
    let subscriber = participant.create_subscriber(QosKind::Default, None, NO_STATUS).await?;
    let reader = subscriber
        .create_datareader::<RawSentence>(&topic_raw, QosKind::Default, None, NO_STATUS)
        .await?;

    // Pub + writers
    let publisher = participant.create_publisher(QosKind::Default, None, NO_STATUS).await?;
    let writer_rmc = publisher
        .create_datawriter::<Rmc>(&topic_rmc, QosKind::Default, None, NO_STATUS)
        .await?;
    let writer_gga = publisher
        .create_datawriter::<Gga>(&topic_gga, QosKind::Default, None, NO_STATUS)
        .await?;

    let mut seq_rmc: u64 = 0;
    let mut seq_gga: u64 = 0;

    loop {
        match reader.read_next_sample().await {
            Ok(sample) => {
                let data = sample.data()?;

                // 1) RAW → NDJSON
                println!(
                    "{}",
                    serde_json::to_string(&json!({
                        "type": "raw",
                        "data": data
                    }))
                        .unwrap()
                );

                // 2) parse y republica tipado + NDJSON
                match data.sentence.as_str() {
                    "RMC" => {
                        if let Some(r) = parse_rmc(&data.raw, data.ts_unix_ms, &mut seq_rmc) {
                            let _ = writer_rmc.write(&r, None).await;

                            let obj = json!({
                                "type": "rmc",
                                "data": {
                                    "seq": r.seq,
                                    "utc_time": r.utc_time,
                                    "utc_date": r.utc_date,
                                    "status": r.status,
                                    "lat_deg": f64_or_null(r.lat_deg),
                                    "lon_deg": f64_or_null(r.lon_deg),
                                    "sog_knots": f32_or_null(r.sog_knots),
                                    "cog_deg": f32_or_null(r.cog_deg),
                                    "mag_var": f32_or_null(r.mag_var),
                                    "mag_var_ew": r.mag_var_ew,
                                    "mode": r.mode,
                                    "ts_unix_ms": r.ts_unix_ms
                                }
                            });
                            println!("{}", serde_json::to_string(&obj).unwrap());
                            info!(?r, "republished RMC");
                        }
                    }
                    "GGA" => {
                        if let Some(g) = parse_gga(&data.raw, data.ts_unix_ms, &mut seq_gga) {
                            let _ = writer_gga.write(&g, None).await;

                            let obj = json!({
                                "type": "gga",
                                "data": {
                                    "seq": g.seq,
                                    "utc_time": g.utc_time,
                                    "lat_deg": f64_or_null(g.lat_deg),
                                    "lon_deg": f64_or_null(g.lon_deg),
                                    "fix_quality": g.fix_quality,
                                    "num_sats": g.num_sats,
                                    "hdop": f32_or_null(g.hdop),
                                    "altitude_m": f32_or_null(g.altitude_m),
                                    "geoid_sep_m": f32_or_null(g.geoid_sep_m),
                                    "ts_unix_ms": g.ts_unix_ms
                                }
                            });
                            println!("{}", serde_json::to_string(&obj).unwrap());
                            info!(?g, "republished GGA");
                        }
                    }
                    _ => {}
                }
            }
            Err(DdsError::NoData) => sleep(Duration::from_millis(100)).await,
            Err(e) => {
                eprintln!(
                    "{}",
                    serde_json::to_string(&json!({"type":"error","message": format!("{e:?}")}))
                        .unwrap()
                );
                sleep(Duration::from_millis(200)).await;
            }
        }
    }
}
