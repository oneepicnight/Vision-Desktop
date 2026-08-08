#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "transaction submission remains unreachable until every wallet gate passes"
    )
)]

use serde::Deserialize;
use std::fmt;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(in crate::wallet) enum WalletSubmissionRejection {
    MalformedRequest,
    DuplicateCanonicalTxId,
    StaleNonce,
    NonceGap,
    DuplicateSenderNonce,
    TxTooLarge,
    MissingSenderPubkey,
    MissingSignature,
    UnsupportedModuleMethod,
    BadTransferArgs,
    InvalidTransferDestination,
    TransferAmountZero,
    TransferToSelf,
    FeeLimitTooLow,
    SenderPubkeyWrongLength,
    SenderPubkeyNotLowercaseHex,
    SignatureWrongLength,
    SignatureNotLowercaseHex,
    MalformedPublicKey,
    InvalidSignature,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::wallet) enum WalletSubmissionOutcome {
    Accepted {
        tx_id: String,
        current_nonce: u64,
    },
    Rejected {
        tx_id: String,
        current_nonce: u64,
        code: WalletSubmissionRejection,
    },
    MalformedRequest,
}

pub(in crate::wallet) enum PrivateSubmissionResponseDisposition {
    Accepted {
        transaction_id: String,
        nonce: u64,
    },
    DefinitiveRejected {
        http_status: u16,
        code: WalletSubmissionRejection,
        allowlist_digest_hex: String,
    },
    OutcomeUnknown,
}

/// Exact versioned non-mutating rejection policy. The independently reviewed production
/// allowlist is intentionally empty.
pub(in crate::wallet) struct SubmissionRejectionPolicy {
    allowed: &'static [(u16, WalletSubmissionRejection)],
}

const REVIEWED_NON_MUTATING_REJECTIONS: &[(u16, WalletSubmissionRejection)] = &[];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletSubmissionParseError {
    InvalidResponse,
    UnexpectedHttpStatus,
    TransactionIdMismatch,
    UnexpectedAcceptedNonce,
    ReplacementNotAuthorized,
}

impl fmt::Display for WalletSubmissionParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidResponse => "Core returned an invalid transaction response",
            Self::UnexpectedHttpStatus => "Core returned an unexpected transaction status",
            Self::TransactionIdMismatch => "Core returned a different transaction identifier",
            Self::UnexpectedAcceptedNonce => "Core accepted an unexpected account nonce",
            Self::ReplacementNotAuthorized => "Core reported an unapproved transaction replacement",
        })
    }
}

impl std::error::Error for WalletSubmissionParseError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireResponse {
    status: String,
    tx_id: Option<String>,
    current_nonce: Option<u64>,
    decision: Option<WireDecision>,
    error: Option<WireError>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireDecision {
    kind: String,
    evict_tx_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireError {
    code: WalletSubmissionRejection,
    message: String,
}

/// Parses the exact RC2 POST `/transactions` response and rejects ambiguity.
///
/// The first Desktop send flow never authorizes replacement transactions, so
/// a `replace` decision fails closed even when Core returns HTTP 200.
pub(in crate::wallet) fn parse_submission_response(
    http_status: u16,
    body: &[u8],
    expected_tx_id: &str,
    submitted_nonce: u64,
) -> Result<WalletSubmissionOutcome, WalletSubmissionParseError> {
    let wire: WireResponse =
        serde_json::from_slice(body).map_err(|_| WalletSubmissionParseError::InvalidResponse)?;

    match (http_status, wire.status.as_str()) {
        (200, "accepted") => {
            let tx_id = wire
                .tx_id
                .ok_or(WalletSubmissionParseError::InvalidResponse)?;
            if tx_id != expected_tx_id {
                return Err(WalletSubmissionParseError::TransactionIdMismatch);
            }
            let current_nonce = wire
                .current_nonce
                .ok_or(WalletSubmissionParseError::InvalidResponse)?;
            if current_nonce != submitted_nonce {
                return Err(WalletSubmissionParseError::UnexpectedAcceptedNonce);
            }
            let decision = wire
                .decision
                .ok_or(WalletSubmissionParseError::InvalidResponse)?;
            if decision.kind != "accept" || decision.evict_tx_id.is_some() {
                return Err(WalletSubmissionParseError::ReplacementNotAuthorized);
            }
            if wire.error.is_some() {
                return Err(WalletSubmissionParseError::InvalidResponse);
            }
            Ok(WalletSubmissionOutcome::Accepted {
                tx_id,
                current_nonce,
            })
        }
        (422, "rejected") => {
            let tx_id = wire
                .tx_id
                .ok_or(WalletSubmissionParseError::InvalidResponse)?;
            if tx_id != expected_tx_id {
                return Err(WalletSubmissionParseError::TransactionIdMismatch);
            }
            let current_nonce = wire
                .current_nonce
                .ok_or(WalletSubmissionParseError::InvalidResponse)?;
            let error = wire
                .error
                .ok_or(WalletSubmissionParseError::InvalidResponse)?;
            if error.message.is_empty() || wire.decision.is_some() {
                return Err(WalletSubmissionParseError::InvalidResponse);
            }
            Ok(WalletSubmissionOutcome::Rejected {
                tx_id,
                current_nonce,
                code: error.code,
            })
        }
        (400, "malformed_request") => {
            let error = wire
                .error
                .ok_or(WalletSubmissionParseError::InvalidResponse)?;
            if error.code != WalletSubmissionRejection::MalformedRequest
                || error.message.is_empty()
                || wire.tx_id.is_some()
                || wire.current_nonce.is_some()
                || wire.decision.is_some()
            {
                return Err(WalletSubmissionParseError::InvalidResponse);
            }
            Ok(WalletSubmissionOutcome::MalformedRequest)
        }
        (200 | 400 | 422, _) => Err(WalletSubmissionParseError::InvalidResponse),
        _ => Err(WalletSubmissionParseError::UnexpectedHttpStatus),
    }
}

impl SubmissionRejectionPolicy {
    pub(in crate::wallet) const fn production() -> Self {
        Self {
            allowed: REVIEWED_NON_MUTATING_REJECTIONS,
        }
    }

    #[cfg(test)]
    pub(in crate::wallet) const fn for_test(
        allowed: &'static [(u16, WalletSubmissionRejection)],
    ) -> Self {
        Self { allowed }
    }

    pub(in crate::wallet) fn digest_hex(&self) -> String {
        let mut hasher = blake3::Hasher::new_derive_key(
            "com.vision.desktop.wallet-submission-rejection-allowlist.v1",
        );
        for (status, code) in self.allowed {
            hasher.update(&status.to_le_bytes());
            hasher.update(code.as_str().as_bytes());
            hasher.update(&[0]);
        }
        hasher.finalize().to_hex().to_string()
    }

    fn contains(&self, status: u16, code: WalletSubmissionRejection) -> bool {
        self.allowed.contains(&(status, code))
    }
}

impl WalletSubmissionRejection {
    pub(in crate::wallet) const fn as_str(self) -> &'static str {
        match self {
            Self::MalformedRequest => "malformed_request",
            Self::DuplicateCanonicalTxId => "duplicate_canonical_tx_id",
            Self::NonceGap => "nonce_gap",
            Self::DuplicateSenderNonce => "duplicate_sender_nonce",
            Self::StaleNonce => "stale_nonce",
            Self::TxTooLarge => "tx_too_large",
            Self::MissingSenderPubkey => "missing_sender_pubkey",
            Self::MissingSignature => "missing_signature",
            Self::UnsupportedModuleMethod => "unsupported_module_method",
            Self::BadTransferArgs => "bad_transfer_args",
            Self::InvalidTransferDestination => "invalid_transfer_destination",
            Self::TransferAmountZero => "transfer_amount_zero",
            Self::TransferToSelf => "transfer_to_self",
            Self::InvalidSignature => "invalid_signature",
            Self::FeeLimitTooLow => "fee_limit_too_low",
            Self::SenderPubkeyWrongLength => "sender_pubkey_wrong_length",
            Self::SenderPubkeyNotLowercaseHex => "sender_pubkey_not_lowercase_hex",
            Self::SignatureWrongLength => "signature_wrong_length",
            Self::SignatureNotLowercaseHex => "signature_not_lowercase_hex",
            Self::MalformedPublicKey => "malformed_public_key",
        }
    }

    const fn is_duplicate(self) -> bool {
        matches!(
            self,
            Self::DuplicateCanonicalTxId | Self::DuplicateSenderNonce
        )
    }
}

pub(in crate::wallet) fn classify_submission_response(
    http_status: u16,
    body: &[u8],
    expected_tx_id: &str,
    expected_nonce: u64,
    policy: &SubmissionRejectionPolicy,
) -> PrivateSubmissionResponseDisposition {
    let Ok(outcome) = parse_submission_response(http_status, body, expected_tx_id, expected_nonce)
    else {
        return PrivateSubmissionResponseDisposition::OutcomeUnknown;
    };
    match outcome {
        WalletSubmissionOutcome::Accepted {
            tx_id,
            current_nonce,
        } => PrivateSubmissionResponseDisposition::Accepted {
            transaction_id: tx_id,
            nonce: current_nonce,
        },
        WalletSubmissionOutcome::Rejected { code, .. }
            if !code.is_duplicate() && policy.contains(http_status, code) =>
        {
            PrivateSubmissionResponseDisposition::DefinitiveRejected {
                http_status,
                code,
                allowlist_digest_hex: policy.digest_hex(),
            }
        }
        WalletSubmissionOutcome::Rejected { .. } | WalletSubmissionOutcome::MalformedRequest => {
            PrivateSubmissionResponseDisposition::OutcomeUnknown
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TX_ID: &str = "a7fc34bf3332fec96623ea7f5ddb638aaad51f039091d2d5bf94adb76a26f0dd";

    #[test]
    fn parses_exact_core_accepted_response() {
        let body = format!(
            "{{\"status\":\"accepted\",\"tx_id\":\"{TX_ID}\",\"current_nonce\":0,\"decision\":{{\"kind\":\"accept\"}}}}"
        );
        assert_eq!(
            parse_submission_response(200, body.as_bytes(), TX_ID, 0),
            Ok(WalletSubmissionOutcome::Accepted {
                tx_id: TX_ID.to_string(),
                current_nonce: 0,
            })
        );
    }

    #[test]
    fn parses_exact_core_rejection_and_malformed_responses() {
        let rejected = format!(
            "{{\"status\":\"rejected\",\"tx_id\":\"{TX_ID}\",\"current_nonce\":5,\"error\":{{\"code\":\"stale_nonce\",\"message\":\"transaction nonce is behind the sender's current canonical nonce\"}}}}"
        );
        assert_eq!(
            parse_submission_response(422, rejected.as_bytes(), TX_ID, 4),
            Ok(WalletSubmissionOutcome::Rejected {
                tx_id: TX_ID.to_string(),
                current_nonce: 5,
                code: WalletSubmissionRejection::StaleNonce,
            })
        );

        let malformed = b"{\"status\":\"malformed_request\",\"error\":{\"code\":\"malformed_request\",\"message\":\"request body must be a canonical signed Tx JSON object\"}}";
        assert_eq!(
            parse_submission_response(400, malformed, TX_ID, 0),
            Ok(WalletSubmissionOutcome::MalformedRequest)
        );
    }

    #[test]
    fn accepted_response_fails_closed_on_mismatch_or_replacement() {
        let wrong_id = b"{\"status\":\"accepted\",\"tx_id\":\"00\",\"current_nonce\":0,\"decision\":{\"kind\":\"accept\"}}";
        assert_eq!(
            parse_submission_response(200, wrong_id, TX_ID, 0),
            Err(WalletSubmissionParseError::TransactionIdMismatch)
        );

        let wrong_nonce = format!(
            "{{\"status\":\"accepted\",\"tx_id\":\"{TX_ID}\",\"current_nonce\":1,\"decision\":{{\"kind\":\"accept\"}}}}"
        );
        assert_eq!(
            parse_submission_response(200, wrong_nonce.as_bytes(), TX_ID, 0),
            Err(WalletSubmissionParseError::UnexpectedAcceptedNonce)
        );

        let replacement = format!(
            "{{\"status\":\"accepted\",\"tx_id\":\"{TX_ID}\",\"current_nonce\":0,\"decision\":{{\"kind\":\"replace\",\"evict_tx_id\":\"11\"}}}}"
        );
        assert_eq!(
            parse_submission_response(200, replacement.as_bytes(), TX_ID, 0),
            Err(WalletSubmissionParseError::ReplacementNotAuthorized)
        );
    }

    #[test]
    fn unknown_or_ambiguous_responses_fail_closed() {
        let unknown_error = format!(
            "{{\"status\":\"rejected\",\"tx_id\":\"{TX_ID}\",\"current_nonce\":0,\"error\":{{\"code\":\"future_error\",\"message\":\"unknown\"}}}}"
        );
        assert_eq!(
            parse_submission_response(422, unknown_error.as_bytes(), TX_ID, 0),
            Err(WalletSubmissionParseError::InvalidResponse)
        );
        assert_eq!(
            parse_submission_response(500, b"{}", TX_ID, 0),
            Err(WalletSubmissionParseError::InvalidResponse)
        );
    }

    #[test]
    fn production_rejection_allowlist_is_empty_and_all_typed_rejections_are_ambiguous() {
        let policy = SubmissionRejectionPolicy::production();
        let codes = [
            "duplicate_canonical_tx_id",
            "stale_nonce",
            "nonce_gap",
            "duplicate_sender_nonce",
            "tx_too_large",
            "missing_sender_pubkey",
            "missing_signature",
            "unsupported_module_method",
            "bad_transfer_args",
            "invalid_transfer_destination",
            "transfer_amount_zero",
            "transfer_to_self",
            "fee_limit_too_low",
            "sender_pubkey_wrong_length",
            "sender_pubkey_not_lowercase_hex",
            "signature_wrong_length",
            "signature_not_lowercase_hex",
            "malformed_public_key",
            "invalid_signature",
        ];
        for code in codes {
            let body = format!(
                "{{\"status\":\"rejected\",\"tx_id\":\"{TX_ID}\",\"current_nonce\":0,\"error\":{{\"code\":\"{code}\",\"message\":\"rejected\"}}}}"
            );
            assert!(matches!(
                classify_submission_response(422, body.as_bytes(), TX_ID, 0, &policy),
                PrivateSubmissionResponseDisposition::OutcomeUnknown
            ));
        }
        assert_eq!(policy.allowed.len(), 0);
    }

    #[test]
    fn only_nonduplicate_reviewed_codes_can_become_definitive_rejections() {
        const ALLOWED: &[(u16, WalletSubmissionRejection)] = &[
            (422, WalletSubmissionRejection::StaleNonce),
            (422, WalletSubmissionRejection::DuplicateCanonicalTxId),
            (422, WalletSubmissionRejection::DuplicateSenderNonce),
        ];
        let policy = SubmissionRejectionPolicy::for_test(ALLOWED);
        let body = format!(
            "{{\"status\":\"rejected\",\"tx_id\":\"{TX_ID}\",\"current_nonce\":1,\"error\":{{\"code\":\"stale_nonce\",\"message\":\"stale\"}}}}"
        );
        assert!(matches!(
            classify_submission_response(422, body.as_bytes(), TX_ID, 0, &policy),
            PrivateSubmissionResponseDisposition::DefinitiveRejected {
                http_status: 422,
                code: WalletSubmissionRejection::StaleNonce,
                ..
            }
        ));
        for code in ["duplicate_canonical_tx_id", "duplicate_sender_nonce"] {
            let body = format!(
                "{{\"status\":\"rejected\",\"tx_id\":\"{TX_ID}\",\"current_nonce\":0,\"error\":{{\"code\":\"{code}\",\"message\":\"duplicate\"}}}}"
            );
            assert!(matches!(
                classify_submission_response(422, body.as_bytes(), TX_ID, 0, &policy),
                PrivateSubmissionResponseDisposition::OutcomeUnknown
            ));
        }
    }
}
