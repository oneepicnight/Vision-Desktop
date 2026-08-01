use secrecy::{ExposeSecret, SecretBox};
use std::fmt;

/// A 256-bit wallet seed held only inside the Rust custody boundary.
///
/// This type intentionally does not implement `Clone`, `Serialize`,
/// `Deserialize`, or `Display`. `SecretBox` restricts access to an explicit
/// exposure method and zeroizes the contained bytes when dropped.
pub struct WalletSeed(SecretBox<[u8; 32]>);

impl WalletSeed {
    /// Runs a narrowly scoped operation with the seed without returning a
    /// borrowed reference that could outlive this wrapper.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "activated by the encrypted vault implementation")
    )]
    pub(in crate::wallet) fn with_exposed<R>(&self, operation: impl FnOnce(&[u8; 32]) -> R) -> R {
        operation(self.0.expose_secret())
    }

    #[cfg(test)]
    fn from_test_bytes(bytes: [u8; 32]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }
}

impl fmt::Debug for WalletSeed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WalletSeed([REDACTED])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_never_contains_seed_bytes() {
        let seed = WalletSeed::from_test_bytes([0xab; 32]);
        let debug = format!("{seed:?}");

        assert_eq!(debug, "WalletSeed([REDACTED])");
        assert!(!debug.contains("ab"));
    }

    #[test]
    fn seed_access_requires_an_explicit_scoped_operation() {
        let seed = WalletSeed::from_test_bytes([7; 32]);
        let checksum =
            seed.with_exposed(|bytes| bytes.iter().map(|value| u32::from(*value)).sum::<u32>());

        assert_eq!(checksum, 224);
    }
}
