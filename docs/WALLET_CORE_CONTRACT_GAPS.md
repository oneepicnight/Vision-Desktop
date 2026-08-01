# Wallet Core Contract Gaps

## Purpose

Vision Desktop must not create permanent addresses or sign transactions from historical examples that disagree with the supported Core implementation. This document records the compatibility evidence inspected before wallet custody work begins.

## Bundled Desktop baseline

The bundled manifest identifies:

- Core tag `vision-core-alpha-rc2`;
- source commit `6a065df8206b50874029a27ee2b54dffae5e3cdd`;
- consensus version `3`;
- P2P protocol version `4`.

The existing Desktop compatibility document lists `/balance/:address`, `/nonce/:address`, `/transaction/:txid`, and `POST /transactions`. Real Core launch remains blocked because the frozen binary cannot bind its private HTTP API to loopback only.

## Conflicting historical implementations

The legacy browser wallet is not a compatible signing specification:

- it constructs a custom binary unsigned payload;
- it omits fields present in the available newer Core transaction structure;
- it uses JavaScript numbers for blockchain amounts;
- it submits to historical route shapes;
- its mnemonic derivation is explicitly described as demo behavior.

The separately available newer Core source instead signs the JSON serialization of a transaction after clearing `sig`, verifies Ed25519 signatures, and includes `access_list` plus additional fee fields. It exposes `/tx/:hash` and `/submit_tx` aliases rather than proving the frozen RC2 `/transaction/:txid` and `/transactions` contract. Its balance response also differs from the Desktop assumption about exact amount strings.

The newer source is evidence of drift, not authorization to change the frozen Desktop contract.

## Verified RC2 account identity contract

The exact supported source revision was retrieved read-only from the authoritative `oneepicnight/Vision-Core` repository at commit `6a065df8206b50874029a27ee2b54dffae5e3cdd`, the peeled commit of both `vision-core-alpha-rc2` and `vision-core-consensus-v1.0.3`. It confirms:

- the wallet secret input is a 32-byte Ed25519 signing seed;
- the public key is the corresponding 32-byte Ed25519 verifying key;
- the account address is the public key encoded as exactly 64 lowercase hexadecimal characters;
- balances and nonces are keyed by that address.

Vision Desktop now has a fixed cross-implementation vector for seed byte `0x07` repeated 32 times. It must derive public key and address `ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c`. The expected value was independently reproduced with a separate Ed25519 implementation and is enforced by Rust tests. A second test encrypts that same seed into the portable recovery artifact, restores it, and requires the identical public key and address.

This satisfies the Desktop `KeyDerivation` and `AddressEncoding` gates. It does not authorize transaction signing, submission, or a mnemonic scheme.

## Verified RC2 transaction contract

The same exact supported source revision confirms the complete RC2 transaction envelope and signing algorithm:

- field order is `nonce`, `sender_pubkey`, `module`, `method`, `args`, `tip`, `fee_limit`, `sig`;
- canonical unsigned bytes are produced by clearing `sig` and serializing the complete envelope with bincode `1.3.3`;
- the transaction identifier is lowercase hexadecimal BLAKE3 of those unsigned bytes;
- `sig` is the lowercase hexadecimal Ed25519 signature over those same unsigned bytes;
- a cash transfer is exactly module `cash`, method `transfer`, with JSON arguments `{ "to": <address>, "amount": <u128> }`;
- the RC2 minimum cash-transfer fee limit is `201` raw units.

Vision Desktop now enforces the exact payload and transaction-identifier sample embedded in the supported Core tests. The Core sample produces transaction ID `a7fc34bf3332fec96623ea7f5ddb638aaad51f039091d2d5bf94adb76a26f0dd`.

A separate fixed signing vector was independently generated for seed byte `0x07` repeated 32 times, recipient byte `0x22` repeated 32 times, amount `42`, nonce `1`, tip `2`, and fee limit `201`. Desktop must produce public key `ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c` and signature `9e6e02196b7dd976f71fcb34c2e420a4cf1b70731e96dcffbe7223969ae760a7eee386e0490d8dbe9a0bdb3056bbfdb35b17e98b189b1288d6ce813df9c82008`. Rust tests verify the canonical bytes, transaction identifier, signature bytes, and Ed25519 verification.

This satisfies the Desktop `TransactionSerialization` and `SignatureVector` gates. The implementation is Rust-only, is not registered as a Tauri command, and cannot sign or submit a user transaction.

## Verified RC2 amount, nonce, and fee contract

The exact supported source marks the native-token denomination as 9 decimal places: one displayed Vision token is `1,000,000,000` raw units. Balances and transfer amounts use unsigned `u128` raw units. Desktop now parses and formats these amounts with checked integer arithmetic only. It rejects signs, separators, exponent notation, whitespace, more than 9 fractional digits, and overflow; no floating-point value enters wallet arithmetic.

`GET /balance/:address` returns `{ address, exists, balance }` and `GET /nonce/:address` returns `{ address, exists, nonce }`. Desktop now decodes those exact typed objects, preserves balance and nonce as decimal strings at the frontend boundary, and rejects inconsistent echoed addresses or existence flags.

The nonce endpoint exposes the sender's next canonical nonce, with `0` for a new account. The safe Desktop draft uses that exact nonce. It does not speculate about later nonces, automatically replace a pending transaction, or silently re-sign after a nonce rejection.

For `cash::transfer`, the charged fee is exactly `1 + tip` raw units, while `fee_limit` is the maximum authorized fee and must be at least `201`. The first safe Desktop policy uses tip `0` and fee limit `201`, rejects arithmetic overflow, and refuses to sign when the calculated fee exceeds the authorized limit. Any future custom-tip or replacement workflow requires separate explicit user review.

This satisfies the Desktop `AmountDenomination` and `FeeAndNonceRules` gates.

## Verified RC2 submission response contract

The exact route is `POST /transactions` with the canonical signed transaction JSON object. Core returns:

- HTTP `200`, status `accepted`, the canonical `tx_id`, the current nonce, and an `accept` or `replace` decision;
- HTTP `422`, status `rejected`, the canonical `tx_id`, current nonce, and a typed error code/message;
- HTTP `400`, status `malformed_request`, with no transaction identifier or nonce.

The Rust-only Desktop response parser requires the returned transaction identifier and accepted nonce to match the reviewed transaction. It rejects unknown shapes and error codes, refuses an unapproved replacement even when Core returns HTTP `200`, and never treats an HTTP success alone as proof that a transaction was mined or final. No network submission command is registered yet.

This satisfies the Desktop `SubmissionResponse` gate.

## Required approved vectors

User-facing signing and submission remain disabled until the supported release and Desktop integration provide confirmed contracts for:

1. pending transaction lookup, mined receipts, reorg handling, and finality states;
2. loopback-only private API operation.

The current recovery contract is the versioned encrypted portable artifact, not a recovery phrase. Any future mnemonic feature requires a separately approved phrase, normalization, checksum, and phrase-to-seed contract before it can be implemented.

## Desktop implementation rule

Vision Desktop may implement the encrypted vault and verified transaction primitives independently, but it must keep create, restore, user-facing sign, and send unavailable until every remaining compatibility and security gate is approved. No Vision-Core behavior will be inferred or duplicated to make the UI appear complete.
