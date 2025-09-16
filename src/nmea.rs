
pub fn split_crlf_lines(buf: &[u8]) -> Vec<String> {
    // Convert lossily to avoid panics on odd bytes; NMEA should be ASCII
    let s = String::from_utf8_lossy(buf);

    // Normalize line endings: split on "\r\n" primarily, but also guard
    // against stray '\n' or '\r' (some senders vary).
    let mut out = Vec::new();
    for chunk in s.split("\r\n") {
        // Each chunk might still carry lone \r or \n if the source mixed separators
        let cleaned = chunk.trim_matches(['\r', '\n']);
        out.push(cleaned.to_string());
    }

    out
}
