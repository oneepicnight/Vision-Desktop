use super::{
    recovery::{PortableRecoveryCredential, RecoveryArtifactError},
    secrets::WalletPassword,
};
use zeroize::{Zeroize, Zeroizing};

pub(in crate::wallet) const MAX_SECRET_INPUT_BYTES: usize = 1024;
pub(in crate::wallet) const MAX_SECRET_INPUT_UTF16_UNITS: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum SecretInputError {
    Empty,
    InvalidUtf16,
    Mismatch,
    TooLong,
}

/// One bounded secret created exclusively by a Rust-owned native ceremony.
///
/// This type deliberately has no Serde implementation and implements neither `Clone`, `Display`,
/// nor `Debug`. The UTF-8 allocation is created at its maximum size before conversion, never
/// grows, and remains zeroizing until its allocation is transferred into a secret owner.
pub(in crate::wallet) struct SecretInput {
    bytes: Zeroizing<Vec<u8>>,
    logical_len: usize,
}

impl SecretInput {
    /// Converts a bounded native UTF-16 buffer without an intermediate `String` or a growing
    /// allocation. The four-byte encoder scratch is erased after every scalar value.
    pub(in crate::wallet) fn from_native_utf16(units: &[u16]) -> Result<Self, SecretInputError> {
        if units.is_empty() {
            return Err(SecretInputError::Empty);
        }
        if units.len() > MAX_SECRET_INPUT_UTF16_UNITS {
            return Err(SecretInputError::TooLong);
        }

        let mut bytes = Zeroizing::new(vec![0_u8; MAX_SECRET_INPUT_BYTES]);
        let mut logical_len = 0_usize;
        for decoded in char::decode_utf16(units.iter().copied()) {
            let scalar = decoded.map_err(|_| SecretInputError::InvalidUtf16)?;
            let mut scratch = [0_u8; 4];
            let encoded = scalar.encode_utf8(&mut scratch).as_bytes();
            let next_len = logical_len
                .checked_add(encoded.len())
                .ok_or(SecretInputError::TooLong)?;
            if next_len > MAX_SECRET_INPUT_BYTES {
                scratch.zeroize();
                return Err(SecretInputError::TooLong);
            }
            bytes[logical_len..next_len].copy_from_slice(encoded);
            logical_len = next_len;
            scratch.zeroize();
        }

        if logical_len == 0 {
            return Err(SecretInputError::Empty);
        }
        Ok(Self { bytes, logical_len })
    }

    pub(in crate::wallet) fn into_wallet_password(mut self) -> WalletPassword {
        let bytes = Zeroizing::new(std::mem::take(&mut *self.bytes));
        WalletPassword::from_fixed_utf8(bytes, self.logical_len)
    }

    pub(in crate::wallet) fn byte_len(&self) -> usize {
        self.logical_len
    }

    /// Constant-time comparison over both complete fixed-capacity buffers. The confirmation is
    /// always erased; the primary is erased on mismatch and retained only as the returned owner.
    pub(in crate::wallet) fn confirm_with(
        mut self,
        mut confirmation: Self,
    ) -> Result<Self, SecretInputError> {
        let mut difference = self.logical_len ^ confirmation.logical_len;
        for (left, right) in self.bytes.iter().zip(confirmation.bytes.iter()) {
            difference |= usize::from(left ^ right);
        }
        confirmation.bytes.zeroize();
        confirmation.logical_len = 0;
        if difference == 0 {
            Ok(self)
        } else {
            self.bytes.zeroize();
            self.logical_len = 0;
            Err(SecretInputError::Mismatch)
        }
    }

    pub(in crate::wallet) fn into_recovery_credential(
        self,
    ) -> Result<PortableRecoveryCredential, RecoveryArtifactError> {
        let encoded = unsafe {
            // SAFETY: the same constructor invariant used by `into_wallet_password` applies.
            std::str::from_utf8_unchecked(&self.bytes[..self.logical_len])
        };
        PortableRecoveryCredential::parse(encoded)
    }

    #[cfg(test)]
    pub(in crate::wallet) fn for_test(value: &str) -> Self {
        let units = Zeroizing::new(value.encode_utf16().collect::<Vec<_>>());
        Self::from_native_utf16(units.as_slice()).expect("test secret must satisfy native bounds")
    }

    #[cfg(test)]
    fn expose_for_test(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.logical_len]).unwrap()
    }

    #[cfg(test)]
    fn allocation_for_test(&self) -> (usize, usize) {
        (self.bytes.len(), self.bytes.capacity())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_utf16_into_one_preallocated_bounded_buffer() {
        let units = Zeroizing::new(
            "correct horse battery staple"
                .encode_utf16()
                .collect::<Vec<_>>(),
        );
        let input = SecretInput::from_native_utf16(units.as_slice()).unwrap();

        assert_eq!(input.expose_for_test(), "correct horse battery staple");
        assert_eq!(
            input.allocation_for_test(),
            (MAX_SECRET_INPUT_BYTES, MAX_SECRET_INPUT_BYTES)
        );
    }

    #[test]
    fn rejects_empty_invalid_and_oversized_native_input() {
        assert_eq!(
            SecretInput::from_native_utf16(&[]).err(),
            Some(SecretInputError::Empty)
        );
        assert_eq!(
            SecretInput::from_native_utf16(&[0xd800]).err(),
            Some(SecretInputError::InvalidUtf16)
        );
        let oversized_units =
            Zeroizing::new(vec![u16::from(b'x'); MAX_SECRET_INPUT_UTF16_UNITS + 1]);
        assert_eq!(
            SecretInput::from_native_utf16(oversized_units.as_slice()).err(),
            Some(SecretInputError::TooLong)
        );
        let oversized_utf8 = Zeroizing::new(vec![0x0800_u16; 342]);
        assert_eq!(
            SecretInput::from_native_utf16(oversized_utf8.as_slice()).err(),
            Some(SecretInputError::TooLong)
        );
    }

    #[test]
    fn conversion_transfers_the_controlled_allocation_to_the_password_wrapper() {
        let input = SecretInput::for_test("local wallet password");
        let password = input.into_wallet_password();

        assert_eq!(format!("{password:?}"), "WalletPassword([REDACTED])");
        assert!(password.with_exposed(|bytes| bytes == b"local wallet password"));
        assert_eq!(
            password.allocation_for_test(),
            (MAX_SECRET_INPUT_BYTES, MAX_SECRET_INPUT_BYTES)
        );
    }

    #[test]
    fn secret_input_has_no_serde_deserializer() {
        let source = include_str!("secret_input.rs");
        assert!(!source.contains(concat!("impl<'de> ", "Deserialize")));
        assert!(!source.contains(concat!("serde_", "json")));
    }
}
