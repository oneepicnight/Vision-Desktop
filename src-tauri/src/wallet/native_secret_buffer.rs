use super::secret_input::{SecretInput, SecretInputError, MAX_SECRET_INPUT_UTF16_UNITS};
use zeroize::{Zeroize, Zeroizing};

/// Fixed-allocation UTF-16 storage for native secret controls.
///
/// The backing vector is allocated at its maximum length before input begins. `logical_len` is the
/// only changing size value; no push, reserve, truncate, or replacement allocation is permitted.
pub(in crate::wallet) struct FixedSecretUtf16 {
    units: Zeroizing<Vec<u16>>,
    logical_len: usize,
}

impl FixedSecretUtf16 {
    pub(in crate::wallet) fn empty() -> Self {
        Self {
            units: Zeroizing::new(vec![0_u16; MAX_SECRET_INPUT_UTF16_UNITS]),
            logical_len: 0,
        }
    }

    pub(in crate::wallet) fn from_ascii(value: &[u8]) -> Result<Self, SecretInputError> {
        if value.is_empty() {
            return Err(SecretInputError::Empty);
        }
        if value.len() > MAX_SECRET_INPUT_UTF16_UNITS || !value.is_ascii() {
            return Err(SecretInputError::TooLong);
        }
        let mut secret = Self::empty();
        for (index, byte) in value.iter().copied().enumerate() {
            secret.units[index] = u16::from(byte);
        }
        secret.logical_len = value.len();
        Ok(secret)
    }

    pub(in crate::wallet) fn push_unit(&mut self, unit: u16) -> Result<(), SecretInputError> {
        if self.logical_len == self.units.len() {
            return Err(SecretInputError::TooLong);
        }
        self.units[self.logical_len] = unit;
        self.logical_len += 1;
        Ok(())
    }

    pub(in crate::wallet) fn pop_unit(&mut self) {
        if self.logical_len > 0 {
            self.logical_len -= 1;
            self.units[self.logical_len].zeroize();
        }
    }

    pub(in crate::wallet) fn as_units(&self) -> &[u16] {
        &self.units[..self.logical_len]
    }

    pub(in crate::wallet) fn is_empty(&self) -> bool {
        self.logical_len == 0
    }

    pub(in crate::wallet) fn wipe(&mut self) {
        self.units.zeroize();
        self.logical_len = 0;
    }

    /// Compares the complete fixed buffers and erases both operands before returning.
    pub(in crate::wallet) fn matches_and_wipe(&mut self, other: &mut Self) -> bool {
        let mut difference = self.logical_len ^ other.logical_len;
        for (left, right) in self.units.iter().zip(other.units.iter()) {
            difference |= usize::from(left ^ right);
        }
        self.wipe();
        other.wipe();
        difference == 0
    }

    pub(in crate::wallet) fn into_secret_input(mut self) -> Result<SecretInput, SecretInputError> {
        let result = SecretInput::from_native_utf16(self.as_units());
        self.wipe();
        result
    }

    #[cfg(test)]
    pub(in crate::wallet) fn allocation_for_test(&self) -> (usize, usize) {
        (self.units.len(), self.units.capacity())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_is_fixed_before_input_and_never_changes() {
        let mut secret = FixedSecretUtf16::empty();
        let original = secret.allocation_for_test();
        for unit in "correct horse battery staple".encode_utf16() {
            secret.push_unit(unit).unwrap();
            assert_eq!(secret.allocation_for_test(), original);
        }
        assert_eq!(
            original,
            (MAX_SECRET_INPUT_UTF16_UNITS, MAX_SECRET_INPUT_UTF16_UNITS)
        );
    }

    #[test]
    fn comparison_wipes_both_operands_on_match_and_mismatch() {
        let mut left = FixedSecretUtf16::from_ascii(b"matching").unwrap();
        let mut right = FixedSecretUtf16::from_ascii(b"matching").unwrap();
        assert!(left.matches_and_wipe(&mut right));
        assert!(left.is_empty());
        assert!(right.is_empty());

        let mut left = FixedSecretUtf16::from_ascii(b"first").unwrap();
        let mut right = FixedSecretUtf16::from_ascii(b"second").unwrap();
        assert!(!left.matches_and_wipe(&mut right));
        assert!(left.is_empty());
        assert!(right.is_empty());
    }

    #[test]
    fn controlled_buffer_converts_directly_to_native_secret_input() {
        let secret = FixedSecretUtf16::from_ascii(b"local wallet password").unwrap();
        let password = secret.into_secret_input().unwrap().into_wallet_password();
        assert!(password.with_exposed(|bytes| bytes == b"local wallet password"));
    }
}
