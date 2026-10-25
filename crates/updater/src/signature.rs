//! The signature of `SHA256SUMS` (SPEC §12.3): the SHA-256 catches a corrupt
//! download, minisign catches a release replaced by whoever got into the
//! GitHub account.
//!
//! The public key is embedded in the executable at compile time: no
//! configuration can change it, so an update cannot be pointed at a key of
//! someone else's. A build made without a key (a development one) cannot tell
//! a good release from a replaced one and says so in the log.

use crate::Error;

/// The public key releases are signed with, passed to the build as
/// `GLYPHPAD_MINISIGN_PUBKEY` by the release workflow. Empty in a build made
/// without one.
pub const PUBLIC_KEY: &str = match option_env!("GLYPHPAD_MINISIGN_PUBKEY") {
    Some(key) => key,
    None => "",
};

/// Whether this build can check a release's signature.
pub fn is_configured() -> bool {
    !PUBLIC_KEY.trim().is_empty()
}

/// Checks the text of `SHA256SUMS.minisig` over the text of `SHA256SUMS`.
pub fn verify(checksums: &str, signature: &str) -> Result<(), Error> {
    verify_with(PUBLIC_KEY, checksums, signature)
}

/// The same, with the key given: tests use it, and so does anything that has
/// a key other than the embedded one.
pub fn verify_with(key: &str, checksums: &str, signature: &str) -> Result<(), Error> {
    if key.trim().is_empty() {
        return Ok(());
    }
    let key = minisign_verify::PublicKey::from_base64(key.trim())
        .map_err(|e| Error::InvalidSignature(format!("the public key cannot be read: {e}")))?;
    let signature = minisign_verify::Signature::decode(signature.trim())
        .map_err(|e| Error::InvalidSignature(format!("the signature cannot be read: {e}")))?;
    key.verify(checksums.as_bytes(), &signature, false)
        .map_err(|e| Error::InvalidSignature(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_without_a_key_accepts_what_it_cannot_check() {
        assert!(verify_with("", "abc  file.zip\n", "").is_ok());
    }

    #[test]
    fn a_signature_that_cannot_be_read_is_refused() {
        let e = verify_with("not a key", "abc  file.zip\n", "not a signature").unwrap_err();
        assert!(matches!(e, Error::InvalidSignature(_)), "{e}");
    }

    #[test]
    fn this_build_says_whether_it_can_check_signatures() {
        // Development builds have no key; the release workflow passes one.
        assert_eq!(is_configured(), !PUBLIC_KEY.trim().is_empty());
    }
}
