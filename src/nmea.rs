//! Divide un buffer (posible múltiple de líneas) por separadores CRLF.
//! - Convierte a UTF-8 de forma tolerante (`from_utf8_lossy`) porque NMEA es ASCII.
//! - Corta `\r\n` y también sanea `\r`/`\n` solitarios.
//! - Devuelve un `Vec<String>` con **líneas sin terminadores**.
//! Nota: no valida checksum ni formato; esa responsabilidad vive en el suscriptor.

pub fn split_crlf_lines(buf: &[u8]) -> Vec<String> {
    // Convert lossily to avoid panics on odd bytes; NMEA should be ASCII
    let s = String::from_utf8_lossy(buf);

    s.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::split_crlf_lines;

    #[test]
    fn splits_crlf_lf_and_cr_line_endings() {
        let input = b"$GPRMC,1*00\r\n$GPGGA,2*00\n$GPGLL,3*00\r$GPVTG,4*00";

        assert_eq!(
            split_crlf_lines(input),
            vec!["$GPRMC,1*00", "$GPGGA,2*00", "$GPGLL,3*00", "$GPVTG,4*00",]
        );
    }
}
