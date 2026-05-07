//! Tipos **serializables por DustDDS** para intercambio entre procesos.
//! - `#[derive(DdsType)]` genera el soporte de tipo (TypeSupport).
//! - `#[dust_dds(key)]` marca el/los campos clave de instancia.

use dust_dds::topic_definition::type_support::DdsType;
use serde::Serialize;

/// Identifica el origen del paquete capturado.
/// No es estrictamente NMEA, pero es útil para trazabilidad: host/puerto/nombre.
#[derive(Debug, Clone, DdsType, Serialize)]
pub struct Source {
    pub host: String,
    pub port: u16,
    pub name: String,
}

/// Representa **una línea NMEA cruda** (sin `\r\n`) con metadatos:
/// - `id`: secuencia local (clave de instancia en DDS).
/// - `talker`: prefijo de 2 letras (p.ej. "GP").
/// - `sentence`: mnemónico de 3 letras (p.ej. "RMC").
/// - `checksum_ok`: reservado para validación futura (v1=false).
/// - `ts_unix_ms`: timestamp en milisegundos.
/// - `src`: de dónde vino (IP/puerto).
/// - `raw`: la línea completa a imprimir o parsear.
#[derive(Debug, Clone, DdsType, Serialize)]
pub struct RawSentence {
    #[dust_dds(key)]
    pub id: u64,
    pub talker: String,    // "GP","GN","AI",…
    pub sentence: String,  // "RMC","GGA","VDM",…
    pub checksum_ok: bool, // por ahora false
    pub ts_unix_ms: i64,   // epoch ms
    pub src: Source,
    pub raw: String, // línea sin CRLF
}
