//! Orquestación del binario `nmea-capture`:
//! - Socket UDP (productor).
//! - Canal MPSC como buffer (backpressure y desacople).
//! - Tarea consumidora que imprime (texto/JSON) y, si se pide, publica a DDS.

use crate::cli::Cli;
use crate::dds_types::{RawSentence, Source};
use crate::net::udp::UdpReceiver;
use crate::nmea::split_crlf_lines;

use anyhow::{Context, Result};
use tokio::{signal, sync::mpsc};
use tracing::{debug, info};
use serde_json::json;

// DDS
use dust_dds::{
    dds_async::domain_participant_factory::DomainParticipantFactoryAsync,
    infrastructure::{qos::QosKind, status::NO_STATUS},
};

/// En Windows, algunos crates de red exigen que WinSock esté inicializado.
/// Lo hacemos **una sola vez** y de forma segura.
/// `WSAStartup` corresponde a `MAKEWORD(2,2)`; `Once` garantiza idempotencia.
/// Si falla la inicialización, se aborta con un `assert_eq!`.
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

/// Arma el pipeline completo:
/// 1) Crea un canal **mpsc** con capacidad 1024 (buffer de backpressure).
/// 2) Spawnea la tarea **productora** que lee UDP y envía (Vec<u8>, SocketAddr).
/// 3) (Opcional) Inicializa **DustDDS** y un `DataWriter<RawSentence>`.
/// 4) Spawnea la tarea **consumidora** (impresor + publisher opcional).
/// 5) Espera `Ctrl+C` y realiza un cierre ordenado.
pub async fn run(cfg: Cli) -> Result<()> {
    // Canal productor UDP -> consumidor impresor
    let (tx, mut rx) = mpsc::channel::<(Vec<u8>, std::net::SocketAddr)>(1024);

    // Tarea **productora UDP**:
    // - Enlaza `UdpReceiver` al `bind`.
    // - En bucle, recibe datagramas y los manda por `tx`.
    // - Si no puede enlazar, aborta con `expect` (error visible en arranque).
    {
        let bind = cfg.bind.clone();
        let tx2 = tx.clone();
        tokio::spawn(async move {
            let mut rxr = UdpReceiver::bind(&bind)
                .await
                .expect("failed to bind UDP socket");
            rxr.run(tx2).await.expect("udp loop failed");
        });
    }

    info!("Listening on UDP {}", cfg.bind);
    let with_ts = cfg.timestamp;
    let json = cfg.json;
    let json_pretty = cfg.json_pretty;

    // Inicialización **opcional** de DDS:
    // - `DomainParticipantFactoryAsync::get_instance()`
    // - `create_participant(domain, QosKind::Default, ...)`
    // - `create_topic::<RawSentence>(name, "RawSentence", ...)`
    //   Nota: la cadena `"RawSentence"` debe coincidir con el nombre del tipo generado por `DdsType`.
    // - `create_publisher(...)`, `create_datawriter(...)`
    // Guardamos `writer` en `dds_writer` para usarlo dentro de la tarea consumidora.
    let mut dds_writer = None;
    if cfg.dds {
        #[cfg(windows)]
        ensure_winsock();

        let dpf = DomainParticipantFactoryAsync::get_instance();
        let participant = dpf
            .create_participant(cfg.dds_domain as i32, QosKind::Default, None, NO_STATUS)
            .await
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;

        let topic = participant
            .create_topic::<RawSentence>(&cfg.dds_topic_raw, "RawSentence", QosKind::Default, None, NO_STATUS)
            .await
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;

        let publisher = participant
            .create_publisher(QosKind::Default, None, NO_STATUS)
            .await
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;

        let writer = publisher
            .create_datawriter::<RawSentence>(&topic, QosKind::Default, None, NO_STATUS)
            .await
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;

        info!("DDS enabled: domain={} topic={}", cfg.dds_domain, cfg.dds_topic_raw);
        dds_writer = Some(writer);
    }

    // Tarea **caturador**:
    // - Recibe desde el canal, trocea el buffer en líneas NMEA (`split_crlf_lines`).
    // - Imprime:
    //   - Texto con timestamp si `--timestamp`
    //   - o **NDJSON** si `--json` (`--json-pretty` cambia formato).
    // - Si `dds_writer` existe, construye un `RawSentence` y lo publica con `write()`.
    // El `seq` local se usa como `id` en DDS (clave de instancia).
    let mut seq: u64 = 0;
    let printer = tokio::spawn(async move {
        while let Some((buf, peer)) = rx.recv().await {
            for line in split_crlf_lines(&buf) {
                if line.is_empty() {
                    continue;
                }

                let (talker, sentence) = split_talker_sentence(&line);
                let ts_iso = chrono::Local::now()
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

                // 1) imprimir por consola
                if json {
                    let obj = json!({
                        "ts": ts_iso,
                        "ts_unix_ms": chrono::Utc::now().timestamp_millis(),
                        "src": { "host": peer.ip().to_string(), "port": peer.port(), "name": "UDP" },
                        "talker": talker,
                        "sentence": sentence,
                        "raw": line,
                    });
                    if json_pretty {
                        println!("{}", serde_json::to_string_pretty(&obj).unwrap());
                    } else {
                        println!("{}", serde_json::to_string(&obj).unwrap());
                    }
                } else {
                    if with_ts {
                        println!("[{}] {}", ts_iso, line);
                    } else {
                        println!("{}", line);
                    }
                }

                // 2) publicar DDS si está activo
                if let Some(writer) = dds_writer.as_ref() {
                    let raw = RawSentence {
                        id: {
                            seq += 1;
                            seq
                        },
                        talker: talker.clone(),
                        sentence: sentence.clone(),
                        checksum_ok: false,
                        ts_unix_ms: chrono::Utc::now().timestamp_millis(),
                        src: Source {
                            host: peer.ip().to_string(),
                            port: peer.port(),
                            name: "UDP".to_string(),
                        },
                        raw: line.clone(),
                    };
                    let _ = writer.write(&raw, None).await;
                }
            }
        }
        debug!("Printer loop ended");
    });

    // Mecanismo de salida ordenada:
    // - `signal::ctrl_c().await` bloquea hasta CTRL+C.
    // - Al liberar `rx`, la tarea consumidora terminará el bucle.
    // - `printer.await` sincroniza el cierre y permite registrar "Printer loop ended".
    signal::ctrl_c().await.context("waiting for Ctrl+C failed")?;
    info!("Ctrl+C received, shutting down…");
    let _ = printer.await;
    Ok(())
}

/// Extrae `(talker, sentence)` de una línea NMEA:
/// - Si comienza por `$` o `!` y tiene al menos 6 chars: [1..3] y [3..6].
/// - En cualquier otro caso devuelve ("??","UNK") como valores de fallback.
/// Nota: no valida checksum; esta función es un **split rápido** para etiquetar.
fn split_talker_sentence(s: &str) -> (String, String) {
    if (s.starts_with('$') || s.starts_with('!')) && s.len() >= 6 {
        (s[1..3].to_string(), s[3..6].to_string())
    } else {
        ("??".into(), "UNK".into())
    }
}