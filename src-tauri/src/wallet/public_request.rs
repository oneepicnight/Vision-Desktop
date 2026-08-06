//! Public-only schemas for a future reviewed Wallet command boundary.
//!
//! These types intentionally contain no secret, path, expiry, or caller-selected window label.
//! Nothing in this module is a Tauri command or is registered with the application.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "public wallet schemas remain unreachable until a separate activation review"
    )
)]

use serde::{de, Deserialize, Deserializer};
use std::fmt;

const MAX_WALLET_ID_BYTES: usize = 64;
const MAX_WALLET_LABEL_BYTES: usize = 64;
const RECOVERY_SELECTION_HANDLE_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum PublicRequestError {
    InvalidRequest,
}

impl PublicRequestError {
    pub(in crate::wallet) const fn code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
        }
    }
}

#[derive(Debug)]
pub(in crate::wallet) struct WalletId(String);

impl WalletId {
    pub(in crate::wallet) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl<'de> Deserialize<'de> for WalletId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = BoundedString::<MAX_WALLET_ID_BYTES>::deserialize(deserializer)?.0;
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(de::Error::custom("invalid wallet identifier"));
        }
        Ok(Self(value))
    }
}

#[derive(Debug)]
pub(in crate::wallet) struct WalletLabel(String);

impl WalletLabel {
    pub(in crate::wallet) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl<'de> Deserialize<'de> for WalletLabel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = BoundedString::<MAX_WALLET_LABEL_BYTES>::deserialize(deserializer)?.0;
        if value.trim() != value || value.chars().any(char::is_control) {
            return Err(de::Error::custom("invalid wallet label"));
        }
        Ok(Self(value))
    }
}

#[derive(Debug)]
pub(in crate::wallet) struct RecoverySelectionHandle(String);

impl RecoverySelectionHandle {
    pub(in crate::wallet) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl<'de> Deserialize<'de> for RecoverySelectionHandle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = BoundedString::<RECOVERY_SELECTION_HANDLE_BYTES>::deserialize(deserializer)?.0;
        if value.len() != RECOVERY_SELECTION_HANDLE_BYTES
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(de::Error::custom("invalid recovery selection handle"));
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::wallet) struct WalletCreateRequest {
    pub(in crate::wallet) wallet_id: WalletId,
    pub(in crate::wallet) label: WalletLabel,
    pub(in crate::wallet) recovery_destination_handle: RecoverySelectionHandle,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::wallet) struct WalletRestoreRequest {
    pub(in crate::wallet) wallet_id: WalletId,
    pub(in crate::wallet) label: WalletLabel,
    pub(in crate::wallet) recovery_source_handle: RecoverySelectionHandle,
}

struct BoundedString<const MAX: usize>(String);

impl<'de, const MAX: usize> Deserialize<'de> for BoundedString<MAX> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(BoundedStringVisitor::<MAX>)
    }
}

struct BoundedStringVisitor<const MAX: usize>;

impl<const MAX: usize> de::Visitor<'_> for BoundedStringVisitor<MAX> {
    type Value = BoundedString<MAX>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bounded public wallet string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        validate_public_length::<MAX, E>(value)?;
        Ok(BoundedString(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        validate_public_length::<MAX, E>(value.as_str())?;
        Ok(BoundedString(value))
    }
}

fn validate_public_length<const MAX: usize, E: de::Error>(value: &str) -> Result<(), E> {
    if value.is_empty() || value.len() > MAX {
        Err(E::custom("invalid bounded public wallet value"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HANDLE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn parse_create(value: &str) -> Result<WalletCreateRequest, PublicRequestError> {
        serde_json::from_str(value).map_err(|_| PublicRequestError::InvalidRequest)
    }

    #[test]
    fn accepts_only_canonical_public_create_metadata() {
        let request = parse_create(&format!(
            r#"{{"wallet_id":"operator_1","label":"Operator Wallet","recovery_destination_handle":"{HANDLE}"}}"#
        ))
        .unwrap();
        assert_eq!(request.wallet_id.as_str(), "operator_1");
        assert_eq!(request.label.as_str(), "Operator Wallet");
        assert_eq!(request.recovery_destination_handle.as_str(), HANDLE);
        assert_eq!(PublicRequestError::InvalidRequest.code(), "invalid_request");
    }

    #[test]
    fn rejects_unknown_duplicate_and_secret_fields_with_one_fixed_error() {
        for request in [
            format!(
                r#"{{"wallet_id":"a","label":"A","recovery_destination_handle":"{HANDLE}","extra":true}}"#
            ),
            format!(
                r#"{{"wallet_id":"a","wallet_id":"b","label":"A","recovery_destination_handle":"{HANDLE}"}}"#
            ),
            format!(
                r#"{{"wallet_id":"a","label":"A","recovery_destination_handle":"{HANDLE}","password":"forbidden"}}"#
            ),
        ] {
            assert_eq!(
                parse_create(request.as_str()).unwrap_err(),
                PublicRequestError::InvalidRequest
            );
        }
    }

    #[test]
    fn rejects_noncanonical_identifiers_labels_and_handles() {
        for (wallet_id, label, handle) in [
            ("bad id", "Good", HANDLE.to_owned()),
            ("good", " padded", HANDLE.to_owned()),
            ("good", "bad\nlabel", HANDLE.to_owned()),
            ("good", "Good", HANDLE.to_ascii_uppercase()),
            ("good", "Good", "a".repeat(63)),
        ] {
            let request = format!(
                r#"{{"wallet_id":{wallet_id:?},"label":{label:?},"recovery_destination_handle":{handle:?}}}"#
            );
            assert_eq!(
                parse_create(request.as_str()).unwrap_err(),
                PublicRequestError::InvalidRequest
            );
        }
    }

    #[test]
    fn restore_schema_contains_no_window_or_secret_field() {
        let request: WalletRestoreRequest = serde_json::from_str(&format!(
            r#"{{"wallet_id":"restored","label":"Restored","recovery_source_handle":"{HANDLE}"}}"#
        ))
        .unwrap();
        assert_eq!(request.wallet_id.as_str(), "restored");
        assert_eq!(request.label.as_str(), "Restored");
        assert_eq!(request.recovery_source_handle.as_str(), HANDLE);
    }
}
