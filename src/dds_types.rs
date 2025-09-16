use dust_dds::topic_definition::type_support::DdsType;
use serde::Serialize;

#[derive(Debug, Clone, DdsType, Serialize)]
pub struct Source {
    pub host: String,
    pub port: u16,
    pub name: String,
}

#[derive(Debug, Clone, DdsType, Serialize)]
pub struct RawSentence {
    #[dust_dds(key)]
    pub id: u64,
    pub talker: String,     // "GP","GN","AI",…
    pub sentence: String,   // "RMC","GGA","VDM",…
    pub checksum_ok: bool,  // por ahora false
    pub ts_unix_ms: i64,    // epoch ms
    pub src: Source,
    pub raw: String,        // línea sin CRLF
}
