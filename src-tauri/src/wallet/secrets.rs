use secrecy::{ExposeSecret, SecretBox, SecretString};
use std::fmt;
use zeroize::Zeroizing;

const WALLET_SEED_BYTES: usize = 32;

/// A 256-bit wallet seed held only inside the Rust custody boundary.
///
/// This type intentionally does not implement `Clone`, `Serialize`,
/// `Deserialize`, or `Display`. `SecretBox` restricts access to an explicit
/// exposure method and zeroizes the contained bytes when dropped.
pub struct WalletSeed(SecretBox<[u8; WALLET_SEED_BYTES]>);

impl WalletSeed {
    /// Runs a narrowly scoped operation with the seed without returning a
    /// borrowed reference that could outlive this wrapper.
    pub(in crate::wallet) fn with_exposed<R>(
        &self,
        operation: impl FnOnce(&[u8; WALLET_SEED_BYTES]) -> R,
    ) -> R {
        operation(self.0.expose_secret())
    }

    pub(in crate::wallet) fn generate() -> Result<Self, getrandom::Error> {
        let mut random_result = Ok(());
        let seed = SecretBox::<[u8; WALLET_SEED_BYTES]>::init_with_mut(|bytes| {
            random_result = getrandom::fill(bytes);
        });
        random_result?;
        Ok(Self(seed))
    }

    pub(in crate::wallet) fn from_zeroizing_vec(bytes: Zeroizing<Vec<u8>>) -> Option<Self> {
        if bytes.len() != WALLET_SEED_BYTES {
            return None;
        }
        Some(Self(SecretBox::<[u8; WALLET_SEED_BYTES]>::init_with_mut(
            |seed| {
                seed.copy_from_slice(&bytes);
            },
        )))
    }

    #[cfg(test)]
    pub(in crate::wallet) fn for_test(fill: u8) -> Self {
        Self(SecretBox::new(Box::new([fill; WALLET_SEED_BYTES])))
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
        let seed = WalletSeed::for_test(0xab);
        let debug = format!("{seed:?}");

        assert_eq!(debug, "WalletSeed([REDACTED])");
        assert!(!debug.contains("ab"));
    }

    #[test]
    fn seed_access_requires_an_explicit_scoped_operation() {
        let seed = WalletSeed::for_test(7);
        let checksum =
            seed.with_exposed(|bytes| bytes.iter().map(|value| u32::from(*value)).sum::<u32>());

        assert_eq!(checksum, 224);
    }

    #[test]
    fn decrypted_seed_enters_secret_box_without_an_ordinary_array() {
        let bytes = Zeroizing::new(vec![0x5a; WALLET_SEED_BYTES]);
        let seed = WalletSeed::from_zeroizing_vec(bytes).unwrap();

        assert!(seed.with_exposed(|restored| restored == &[0x5a; WALLET_SEED_BYTES]));
    }

    #[test]
    fn decrypted_seed_requires_the_exact_seed_length() {
        assert!(WalletSeed::from_zeroizing_vec(Zeroizing::new(vec![7; 31])).is_none());
        assert!(WalletSeed::from_zeroizing_vec(Zeroizing::new(vec![7; 33])).is_none());
    }

    #[test]
    fn password_debug_output_is_redacted() {
        let password = WalletPassword::new("do-not-print-this-password".to_string());

        assert_eq!(format!("{password:?}"), "WalletPassword([REDACTED])");
    }
}
