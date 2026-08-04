use super::{
    recovery::{PortableRecoveryCredential, RecoveryArtifactError},
    secrets::WalletPassword,
};
use serde::de::{self, Deserialize, Deserializer, Visitor};
use std::fmt;
use zeroize::Zeroizing;

const MAX_SECRET_INPUT_BYTES: usize = 1024;

/// One bounded secret received by the future Rust wallet command boundary.
///
/// This type intentionally implements neither `Serialize`, `Clone`, `Display`, nor `Debug`.
/// Its owned UTF-8 buffer is zeroized on drop, including deserialization failures after ownership
/// transfers to this type.
pub(in crate::wallet) struct SecretInput(Zeroizing<String>);

impl SecretInput {
    pub(in crate::wallet) fn into_wallet_password(mut self) -> WalletPassword {
        WalletPassword::new(std::mem::take(&mut *self.0))
    }

    pub(in crate::wallet) fn into_recovery_credential(
        self,
    ) -> Result<PortableRecoveryCredential, RecoveryArtifactError> {
        PortableRecoveryCredential::parse(self.0.as_str())
    }

    #[cfg(test)]
    fn expose_for_test(&self) -> &str {
        self.0.as_str()
    }
}

impl<'de> Deserialize<'de> for SecretInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(SecretInputVisitor)
    }
}

struct SecretInputVisitor;

impl Visitor<'_> for SecretInputVisitor {
    type Value = SecretInput;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bounded wallet secret")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        validate_length(value.len())?;
        Ok(SecretInput(Zeroizing::new(value.to_owned())))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if let Err(error) = validate_length::<E>(value.len()) {
            drop(Zeroizing::new(value));
            return Err(error);
        }
        Ok(SecretInput(Zeroizing::new(value)))
    }
}

fn validate_length<E: de::Error>(length: usize) -> Result<(), E> {
    if length > MAX_SECRET_INPUT_BYTES {
        Err(E::custom("wallet secret exceeds the accepted size"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_string_inputs_within_the_byte_limit() {
        let input: SecretInput = serde_json::from_str("\"correct horse battery staple\"").unwrap();
        assert_eq!(input.expose_for_test(), "correct horse battery staple");

        assert!(serde_json::from_str::<SecretInput>("42").is_err());
        assert!(serde_json::from_str::<SecretInput>("null").is_err());
        let oversized = serde_json::to_string(&"x".repeat(MAX_SECRET_INPUT_BYTES + 1)).unwrap();
        assert!(serde_json::from_str::<SecretInput>(&oversized).is_err());
        let oversized_utf8 = serde_json::to_string(&"é".repeat(513)).unwrap();
        assert!(serde_json::from_str::<SecretInput>(&oversized_utf8).is_err());
    }

    #[test]
    fn conversion_moves_the_owned_value_into_the_password_wrapper() {
        let input: SecretInput = serde_json::from_str("\"local wallet password\"").unwrap();
        let password = input.into_wallet_password();

        assert_eq!(format!("{password:?}"), "WalletPassword([REDACTED])");
        assert!(password.with_exposed(|bytes| bytes == b"local wallet password"));
    }
}
