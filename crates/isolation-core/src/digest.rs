//! Empreintes sha256 : tokens des équipes et versions de jar (`build`).

use std::fmt::Write;

use sha2::{Digest, Sha256};

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(2 * bytes.len()), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// 16 premiers caractères hex du sha256 : identifie une version de jar.
pub fn short_hash(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes)[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_digests() {
        assert_eq!(hex(&[0, 0xab, 0xff]), "00abff");
        let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(sha256_hex(b""), empty);
        assert_eq!(short_hash(b""), empty[..16]);
    }
}
