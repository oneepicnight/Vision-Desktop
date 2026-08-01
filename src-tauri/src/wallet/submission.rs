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
}
