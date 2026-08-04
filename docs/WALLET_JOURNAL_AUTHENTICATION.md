# Wallet Activity Journal Authentication

## Status

The private Rust wallet journal now uses schema version 2 and cryptographic tamper evidence. No
journal, signing, submission, or wallet lifecycle command is registered with Tauri, and React has
no access to the authenticator or wallet seed.

## Authentication anchor

`WalletJournalAuthenticator` is constructed from the unlocked `WalletSeed` and the expected wallet
identifier. It implements neither serialization, cloning, display, nor debug output. The seed is
the authentication anchor outside the journal file; no journal field can recreate signing or
authentication authority.

Rust derives a dedicated 32-byte authentication subkey from the wallet seed using BLAKE3
derive-key mode and the context:

`com.vision.desktop.wallet-activity-journal-authentication-key.v1`

The derived subkey enters `SecretBox` directly and is zeroized on drop. The authenticator does not
retain the wallet seed. Each event is then authenticated with keyed BLAKE3. The input is
domain-separated with:

`vision-desktop-wallet-activity-authentication-v1`

The authenticated payload has a fixed versioned field order and covers:

- schema and version;
- wallet identifier and sequence;
- transaction identifier and event timestamp;
- complete submitted-transfer or receipt-observation content;
- the preceding event authentication tag.

The first event is chained to an all-zero genesis tag. Tags are encoded as exact lowercase
64-character hexadecimal values. Verification compares decoded tags without data-dependent early
exit.

## Ownership rules

Before an accepted submission can be recorded, Desktop derives the public address from the same
seed that authenticates the journal and requires the transaction sender to match it. A valid
transaction from a different seed is rejected. Loading with a different seed or different wallet
identifier fails closed.

Tests prove rejection of:

- modified public transaction content;
- a different wallet seed;
- a different wallet identifier;
- changed sequence values;
- reordered authenticated events;
- unknown fields, truncation, and malformed tags;
- transactions whose sender is not owned by the authenticating seed.

The journal stores only public activity metadata and authentication tags. It stores no seed,
signature, signed transaction bytes, password, recovery credential, vault data, DPAPI blob,
session token, or activation proof.

## Deliberate limitations

Authentication proves that retained events were produced for this wallet and that the retained
chain has not been modified or reordered. It does not prove completeness. An attacker who saved an
older authentic complete prefix can replace the journal with that prefix because the latest head
tag is not yet anchored in a separately protected, transactionally updated store.

The journal therefore remains non-authoritative local display history. It never supplies balances,
nonces, signing decisions, retry decisions, receipt truth, or confidence by itself. Core remains
the source for current canonical observations.

Filesystem replacement races, cross-process ownership, interrupted append recovery, and atomic
publication are intentionally assigned to the next hardening tranche. This commit does not claim
to solve them. Version 1 journals fail closed; no migration is enabled while custody commands
remain unregistered.
