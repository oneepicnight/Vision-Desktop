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

## Required approved vectors

Signing remains disabled until the supported Core release provides fixed vectors for:

1. recovery phrase and normalization rules;
2. seed/private-key derivation;
3. public-key derivation and address encoding;
4. smallest-unit denomination and display precision;
5. every transaction field, default, ordering, and byte encoding;
6. canonical unsigned bytes and expected Ed25519 signature;
7. transaction identifier derivation;
8. nonce semantics and conflict handling;
9. fee estimation, minimum fee, and tip behavior;
10. submission request and explicit accepted/rejected responses;
11. pending transaction lookup, receipts, and finality states;
12. loopback-only private API operation.

## Desktop implementation rule

Vision Desktop may implement the encrypted vault independently, but it must keep create, restore, sign, and send unavailable until the derivation and transaction vectors are approved. No Vision-Core behavior will be inferred or duplicated to make the UI appear complete.
