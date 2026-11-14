//! Random secrets and their stored hashes.
//!
//! Only hashes of agent tokens, enrollment codes and session tokens are
//! stored, so a leaked database does not let anyone act as an agent or as
//! the parent.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

/// Crockford's base32: no `I`, `L`, `O` or `U`, which are easy to misread.
const CODE_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const CODE_LENGTH: usize = 8;

/// A 256-bit random token in hex.
pub(crate) fn token() -> Result<String, getrandom::Error> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)?;
    Ok(hex(&bytes))
}

/// A short code for typing on another machine, such as `K7QM-2XRP`.
pub(crate) fn enrollment_code() -> Result<String, getrandom::Error> {
    let mut bytes = [0_u8; CODE_LENGTH];
    getrandom::fill(&mut bytes)?;
    let symbols: String = bytes
        .iter()
        .map(|byte| char::from(CODE_ALPHABET[usize::from(byte % 32)]))
        .collect();
    Ok(format!("{}-{}", &symbols[..4], &symbols[4..]))
}

/// The canonical form of a typed code: case and separators do not matter,
/// and letters that look like digits are read as those digits.
pub(crate) fn normalize_code(code: &str) -> String {
    code.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|symbol| match symbol.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect()
}

/// The stored hash of a secret.
pub(crate) fn hash(secret: &str) -> String {
    hex(&Sha256::digest(secret.as_bytes()))
}

/// Lowercase hex encoding.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
            // Writing to a `String` cannot fail.
            let _ = write!(text, "{byte:02x}");
            text
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_long_and_distinct() {
        let (a, b) = (token().unwrap(), token().unwrap());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    #[test]
    fn codes_use_the_unambiguous_alphabet() {
        let code = enrollment_code().unwrap();
        assert_eq!(code.len(), 9);
        assert_eq!(&code[4..5], "-");
        assert!(
            normalize_code(&code)
                .bytes()
                .all(|b| CODE_ALPHABET.contains(&b))
        );
    }

    #[test]
    fn typed_codes_are_normalized() {
        assert_eq!(normalize_code(" k7qm-2xrp "), "K7QM2XRP");
        assert_eq!(normalize_code("OIL0-1111"), "01101111");
        assert_eq!(
            hash(&normalize_code("k7qm2xrp")),
            hash(&normalize_code("K7QM-2XRP"))
        );
    }

    #[test]
    fn hex_encodes_bytes() {
        assert_eq!(hex(&[0, 15, 255]), "000fff");
    }
}
