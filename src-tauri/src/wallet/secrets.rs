use secrecy::{ExposeSecret, SecretBox, SecretString};
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
    pub(in crate::wallet) fn with_exposed<R>(&self, operation: impl FnOnce(&[u8; 32]) -> R) -> R {
        operation(self.0.expose_secret())
    }

    pub(in crate::wallet) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }
}

/// A user-supplied wallet password held only inside the Rust custody boundary.
///
/// This type deliberately has no serialization or display implementation.
pub struct WalletPassword(SecretString);

impl WalletPassword {
    pub(in crate::wallet) fn new(password: String) -> Self {
        Self(SecretString::from(password))
    }

    pub(in crate::wallet) fn with_exposed<R>(&self, operation: impl FnOnce(&[u8]) -> R) -> R {
        operation(self.0.expose_secret().as_bytes())
    }
}

impl fmt::Debug for WalletPassword {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WalletPassword([REDACTED])")
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
        let seed = WalletSeed::from_bytes([0xab; 32]);
        let debug = format!("{seed:?}");

        assert_eq!(debug, "WalletSeed([REDACTED])");
        assert!(!debug.contains("ab"));
    }

    #[test]
    fn seed_access_requires_an_explicit_scoped_operation() {
        let seed = WalletSeed::from_bytes([7; 32]);
        let checksum =
            seed.with_exposed(|bytes| bytes.iter().map(|value| u32::from(*value)).sum::<u32>());

        assert_eq!(checksum, 224);
    }

    #[test]
    fn password_debug_output_is_redacted() {
        let password = WalletPassword::new("do-not-print-this-password".to_string());

        assert_eq!(format!("{password:?}"), "WalletPassword([REDACTED])");
    }
}
