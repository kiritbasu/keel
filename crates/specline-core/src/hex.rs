//! Bytes as lowercase hex, in one place.
//!
//! There were two copies of this fold and there was about to be a third. The
//! third is what made it a module: `sha2` 0.11 returns a `hybrid-array` `Array`
//! rather than a `GenericArray`, and `Array` does not implement `LowerHex` — so
//! `format!("{:x}", Sha256::digest(…))` stopped compiling, in the line that
//! decides whether a downloaded release matches the digest in its manifest.
//!
//! A dependency was the other option. `hex` is small and well known, and it is
//! still a dependency added to avoid writing six lines that cannot go wrong in
//! an interesting way — which is the trade scale discipline says not to make.

/// Lowercase hex, two characters per byte.
///
/// Used for the daemon's API token and for comparing a release archive against
/// the checksum in its manifest. Both compare the *string*, so the case matters:
/// a manifest written by `sha256sum` is lowercase, and an uppercase encoding
/// here would fail every verification with a diff that looks like a corrupted
/// download.
pub fn encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            use std::fmt::Write as _;
            // Writing to a `String` cannot fail, and the alternative is an
            // `unwrap` in library code.
            let _ = write!(s, "{b:02x}");
            s
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn every_byte_is_two_characters() {
        assert_eq!(encode(&[0x00, 0x0f, 0xff]), "000fff");
        assert_eq!(encode(&[]), "");
    }

    /// Lowercase, and asserted rather than assumed. A manifest written by
    /// `sha256sum` is lowercase and the comparison is a string comparison, so
    /// an uppercase encoding here would reject every release with a message
    /// that reads like a corrupted download.
    #[test]
    fn the_output_is_lowercase() {
        let encoded = encode(&[0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(encoded, "deadbeef");
        assert_eq!(encoded, encoded.to_lowercase());
    }

    /// The property the fold is easy to get wrong: a byte below 16 keeps its
    /// leading zero. Without the `02` it silently shortens the string, and a
    /// digest one character short still looks like a digest.
    #[test]
    fn a_small_byte_keeps_its_leading_zero() {
        assert_eq!(encode(&[0x01, 0x02, 0x03]).len(), 6);
        assert_eq!(encode(&[0x05]), "05");
    }
}
