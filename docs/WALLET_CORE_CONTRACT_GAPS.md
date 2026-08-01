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

## Required approved vectors

Signing remains disabled until the supported Core release provides fixed vectors or confirmed contracts for:

1. smallest-unit denomination and display precision;
2. every transaction field, default, ordering, and byte encoding;
3. canonical unsigned bytes and expected Ed25519 signature;
4. transaction identifier derivation;
5. nonce semantics and conflict handling;
6. fee estimation, minimum fee, and tip behavior;
7. submission request and explicit accepted/rejected responses;
8. pending transaction lookup, receipts, and finality states;
9. loopback-only private API operation.

The current recovery contract is the versioned encrypted portable artifact, not a recovery phrase. Any future mnemonic feature requires a separately approved phrase, normalization, checksum, and phrase-to-seed contract before it can be implemented.

## Desktop implementation rule

Vision Desktop may implement the encrypted vault independently, but it must keep create, restore, sign, and send unavailable until the derivation and transaction vectors are approved. No Vision-Core behavior will be inferred or duplicated to make the UI appear complete.
