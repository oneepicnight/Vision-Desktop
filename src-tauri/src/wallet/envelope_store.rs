use super::{
    lifecycle::WalletCustodyPathAuthority,
    reconciliation::{ReconciliationEnvelopeBinding, ReconciliationReservation},
    secrets::WalletSeed,
    secure_filesystem::{
        create_new_publishable_file, open_existing_file, publish_open_file, replace_with_open_file,
        DirectoryChainGuard,
    },
    storage_security,
    transaction::{canonical_transaction_id, verify_signed_transaction, VisionTransaction},
};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    Key, XChaCha20Poly1305, XNonce,
};
use once_cell::sync::Lazy;
use secrecy::{ExposeSecret, SecretBox};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};
use zeroize::{Zeroize, Zeroizing};

const STORE_SCHEMA: &str = "vision-desktop-wallet-signed-envelopes";
const STORE_VERSION: u32 = 1;
const WRAPPER_SCHEMA: &str = "vision-desktop-wallet-signed-envelopes-ciphertext";
const WRAPPER_VERSION: u32 = 1;
const HEAD_SCHEMA: &str = "vision-desktop-wallet-signed-envelopes-head";
const HEAD_VERSION: u32 = 1;
const STORE_FILE: &str = "wallet.signed-envelopes.v1.enc";
const HEAD_FILE: &str = "wallet.signed-envelopes.v1.head.json";
const STAGING_PREFIX: &str = ".wallet-signed-envelopes-stage-";
const MAX_BODY_BYTES: usize = 65_536;
const MAX_ENTRIES: usize = 10_000;
const MAX_CONTAINER_BYTES: usize = 64 * 1024 * 1024;
const MAX_HEAD_BYTES: usize = 8 * 1024;
const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 24;
const ENCRYPTION_KEY_CONTEXT: &str = "com.vision.desktop.wallet-signed-envelope-encryption-key.v1";
const HEAD_KEY_CONTEXT: &str = "com.vision.desktop.wallet-signed-envelope-head-key.v1";
const HEAD_DOMAIN: &[u8] = b"vision-desktop.wallet-signed-envelope-head.v1";
const BODY_DIGEST_CONTEXT: &str = "com.vision.desktop.wallet-signed-envelope-digest.v1";
const COMMITMENT_CONTEXT: &str = "com.vision.desktop.wallet-signed-envelope-commitment.v1";

static STORE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub(super) struct EnvelopeStore {
    container_path: PathBuf,
    head_path: PathBuf,
}

pub(super) struct EnvelopeStoreAuthenticator {
    wallet_id: String,
    sender_address: String,
    encryption_key: SecretBox<[u8; KEY_BYTES]>,
    head_key: SecretBox<[u8; KEY_BYTES]>,
}

pub(super) struct PreparedEnvelopeAuthority {
    attempt_id: String,
    transaction_id: String,
    commitment_hex: String,
    reconciliation_parent_generation: u64,
    reconciliation_parent_tag_hex: String,
    reserved_prepared_generation: u64,
}

pub(super) struct AmbiguousEnvelopeAuthority {
    attempt_id: String,
    transaction_id: String,
    commitment_hex: String,
}

pub(super) struct AcceptedEnvelopeAuthority {
    attempt_id: String,
    transaction_id: String,
    commitment_hex: String,
}

/// Proof that one exact Prepared envelope occupies an otherwise empty authenticated
/// reconciliation reservation. It grants cleanup only; it carries no Core or signing authority.
pub(super) struct PreparedOrphanAuthority {
    attempt_id: String,
    transaction_id: String,
    commitment_hex: String,
}

impl PreparedOrphanAuthority {
    pub(super) fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    pub(super) fn commitment_hex(&self) -> &str {
        &self.commitment_hex
    }
}

pub(super) struct EnvelopeEntryInput<'a> {
    pub wallet_id: &'a str,
    pub attempt_id: &'a str,
    pub transaction_id: &'a str,
    pub transaction: &'a VisionTransaction,
    pub exact_body: &'a [u8],
    pub signed_body_digest_hex: &'a str,
    pub compatibility_contract_digest_hex: &'a str,
    pub created_at_unix_ms: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RetentionState {
    Prepared,
    Ambiguous,
    Accepted,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvelopeEntry {
    attempt_id: String,
    transaction_id: String,
    transaction: VisionTransaction,
    exact_body_hex: String,
    signed_body_digest_hex: String,
    compatibility_contract_digest_hex: String,
    reconciliation_parent_generation: u64,
    reconciliation_parent_tag_hex: String,
    reserved_prepared_generation: u64,
    envelope_commitment_hex: String,
    created_at_unix_ms: u64,
    retention_state: RetentionState,
}

impl Drop for EnvelopeEntry {
    fn drop(&mut self) {
        self.attempt_id.zeroize();
        self.transaction_id.zeroize();
        self.transaction.sender_pubkey.zeroize();
        self.transaction.module.zeroize();
        self.transaction.method.zeroize();
        self.transaction.args.zeroize();
        self.transaction.sig.zeroize();
        self.exact_body_hex.zeroize();
        self.signed_body_digest_hex.zeroize();
        self.compatibility_contract_digest_hex.zeroize();
        self.reconciliation_parent_tag_hex.zeroize();
        self.envelope_commitment_hex.zeroize();
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaintextContainer {
    schema: String,
    version: u32,
    wallet_id: String,
    generation: u64,
    previous_head_tag_hex: String,
    entries: Vec<EnvelopeEntry>,
}

impl Drop for PlaintextContainer {
    fn drop(&mut self) {
        self.wallet_id.zeroize();
        self.previous_head_tag_hex.zeroize();
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CiphertextWrapper {
    schema: String,
    version: u32,
    wallet_id: String,
    generation: u64,
    previous_head_tag_hex: String,
    nonce_hex: String,
    ciphertext_length: u64,
    entry_count: u32,
    ciphertext_hex: String,
}

#[derive(Serialize)]
struct CiphertextAad<'a> {
    schema: &'a str,
    version: u32,
    wallet_id: &'a str,
    generation: u64,
    previous_head_tag_hex: &'a str,
    ciphertext_length: u64,
    entry_count: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvelopeHead {
    schema: String,
    version: u32,
    wallet_id: String,
    generation: u64,
    previous_head_tag_hex: String,
    state: EnvelopeHeadState,
    authentication_tag_hex: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum EnvelopeHeadState {
    Committed {
        position: EnvelopePosition,
    },
    Transition {
        previous_generation: u64,
        previous_previous_head_tag_hex: String,
        previous: EnvelopePosition,
        next: EnvelopePosition,
    },
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvelopePosition {
    container_generation: u64,
    ciphertext_digest_hex: String,
    ciphertext_length: u64,
    nonce_hex: String,
    entry_count: u32,
}

struct LoadedStore {
    container: Option<PlaintextContainer>,
    head: EnvelopeHead,
}

#[cfg_attr(test, derive(Debug))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EnvelopeStoreError {
    InvalidRequest,
    StorageUnavailable,
    AuthenticationFailed,
    Collision,
    StoreFull,
    InvalidTransition,
}

impl EnvelopeStore {
    pub(super) fn for_custody(
        custody: &WalletCustodyPathAuthority,
    ) -> Result<Self, EnvelopeStoreError> {
        let directory = custody
            .vault_path()
            .parent()
            .ok_or(EnvelopeStoreError::InvalidRequest)?;
        Ok(Self {
            container_path: directory.join(STORE_FILE),
            head_path: directory.join(HEAD_FILE),
        })
    }

    pub(super) fn publish_prepared(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
        input: EnvelopeEntryInput<'_>,
        reservation: &ReconciliationReservation,
    ) -> Result<PreparedEnvelopeAuthority, EnvelopeStoreError> {
        let _lock = lock_store()?;
        let mut loaded = self.load_or_create_genesis(authenticator)?;
        let entry = build_entry(authenticator, input, reservation)?;
        validate_collision(loaded.container.as_ref(), &entry)?;
        let authority = PreparedEnvelopeAuthority {
            attempt_id: entry.attempt_id.clone(),
            transaction_id: entry.transaction_id.clone(),
            commitment_hex: entry.envelope_commitment_hex.clone(),
            reconciliation_parent_generation: entry.reconciliation_parent_generation,
            reconciliation_parent_tag_hex: entry.reconciliation_parent_tag_hex.clone(),
            reserved_prepared_generation: entry.reserved_prepared_generation,
        };
        let mut entries = loaded
            .container
            .as_mut()
            .map(|container| std::mem::take(&mut container.entries))
            .unwrap_or_default();
        if entries.len() >= MAX_ENTRIES {
            return Err(EnvelopeStoreError::StoreFull);
        }
        entries.push(entry);
        entries.sort_by(|left, right| left.attempt_id.cmp(&right.attempt_id));
        validate_entry_set(
            &authenticator.wallet_id,
            &authenticator.sender_address,
            &entries,
        )?;
        self.publish_container(authenticator, &loaded.head, entries)?;
        let verified = self.load_authenticated_unlocked(authenticator)?;
        require_entry(
            verified.container.as_ref(),
            &authority.attempt_id,
            &authority.transaction_id,
            &authority.commitment_hex,
            RetentionState::Prepared,
        )?;
        Ok(authority)
    }

    pub(super) fn contains_transaction_id(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
        transaction_id: &str,
    ) -> Result<bool, EnvelopeStoreError> {
        if !is_lower_hex(transaction_id, 32) {
            return Err(EnvelopeStoreError::InvalidRequest);
        }
        let _lock = lock_store()?;
        if !self.container_path.try_exists().unwrap_or(true)
            && !self.head_path.try_exists().unwrap_or(true)
        {
            return Ok(false);
        }
        let loaded = self.load_authenticated_unlocked(authenticator)?;
        Ok(loaded.container.is_some_and(|container| {
            container
                .entries
                .iter()
                .any(|entry| entry.transaction_id == transaction_id)
        }))
    }

    pub(super) fn ensure_ambiguous_binding(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
        binding: &ReconciliationEnvelopeBinding,
    ) -> Result<(), EnvelopeStoreError> {
        self.ensure_state_binding(authenticator, binding, RetentionState::Ambiguous)
    }

    pub(super) fn ensure_accepted_binding(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
        binding: &ReconciliationEnvelopeBinding,
    ) -> Result<(), EnvelopeStoreError> {
        self.ensure_state_binding(authenticator, binding, RetentionState::Accepted)
    }

    pub(super) fn cleanup_terminal_binding(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
        binding: &ReconciliationEnvelopeBinding,
    ) -> Result<(), EnvelopeStoreError> {
        let _lock = lock_store()?;
        if !self.container_path.try_exists().unwrap_or(true)
            && !self.head_path.try_exists().unwrap_or(true)
        {
            return Err(EnvelopeStoreError::AuthenticationFailed);
        }
        let mut loaded = self.load_authenticated_unlocked(authenticator)?;
        let container = loaded
            .container
            .as_mut()
            .ok_or(EnvelopeStoreError::AuthenticationFailed)?;
        let index = container.entries.iter().position(|entry| {
            entry.attempt_id == binding.attempt_id()
                && entry.transaction_id == binding.transaction_id()
                && entry.envelope_commitment_hex == binding.commitment_hex()
                && entry.reconciliation_parent_generation == binding.parent_generation()
                && entry.reconciliation_parent_tag_hex == binding.parent_tag_hex()
                && entry.reserved_prepared_generation == binding.reserved_prepared_generation()
                && entry.retention_state != RetentionState::Accepted
        });
        let Some(index) = index else {
            return if container
                .entries
                .iter()
                .all(|entry| entry.retention_state == RetentionState::Accepted)
            {
                Ok(())
            } else {
                Err(EnvelopeStoreError::InvalidTransition)
            };
        };
        container.entries.remove(index);
        let entries = std::mem::take(&mut container.entries);
        self.publish_container(authenticator, &loaded.head, entries)
    }

    pub(super) fn identify_prepared_orphan(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
        reservation: &ReconciliationReservation,
    ) -> Result<Option<PreparedOrphanAuthority>, EnvelopeStoreError> {
        let _lock = lock_store()?;
        if !self.container_path.try_exists().unwrap_or(true)
            && !self.head_path.try_exists().unwrap_or(true)
        {
            return Ok(None);
        }
        let loaded = self.load_authenticated_unlocked(authenticator)?;
        let Some(container) = loaded.container.as_ref() else {
            return Ok(None);
        };
        let mut candidate = None;
        for entry in &container.entries {
            if entry.retention_state == RetentionState::Accepted {
                continue;
            }
            if entry.retention_state != RetentionState::Prepared
                || entry.reconciliation_parent_generation != reservation.parent_generation()
                || entry.reconciliation_parent_tag_hex
                    != reservation.parent_authentication_tag_hex()
                || entry.reserved_prepared_generation != reservation.prepared_generation()
                || candidate.is_some()
            {
                return Err(EnvelopeStoreError::AuthenticationFailed);
            }
            candidate = Some(PreparedOrphanAuthority {
                attempt_id: entry.attempt_id.clone(),
                transaction_id: entry.transaction_id.clone(),
                commitment_hex: entry.envelope_commitment_hex.clone(),
            });
        }
        Ok(candidate)
    }

    pub(super) fn cleanup_prepared_orphan(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
        authority: PreparedOrphanAuthority,
    ) -> Result<(), EnvelopeStoreError> {
        let _lock = lock_store()?;
        let mut loaded = self.load_authenticated_unlocked(authenticator)?;
        let container = loaded
            .container
            .as_mut()
            .ok_or(EnvelopeStoreError::AuthenticationFailed)?;
        let index = container
            .entries
            .iter()
            .position(|entry| {
                entry.attempt_id == authority.attempt_id
                    && entry.transaction_id == authority.transaction_id
                    && entry.envelope_commitment_hex == authority.commitment_hex
                    && entry.retention_state == RetentionState::Prepared
            })
            .ok_or(EnvelopeStoreError::InvalidTransition)?;
        container.entries.remove(index);
        let entries = std::mem::take(&mut container.entries);
        self.publish_container(authenticator, &loaded.head, entries)
    }

    pub(super) fn mark_ambiguous(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
        authority: PreparedEnvelopeAuthority,
    ) -> Result<AmbiguousEnvelopeAuthority, EnvelopeStoreError> {
        self.transition_entry(
            authenticator,
            &authority.attempt_id,
            &authority.transaction_id,
            &authority.commitment_hex,
            RetentionState::Prepared,
            RetentionState::Ambiguous,
        )?;
        Ok(AmbiguousEnvelopeAuthority {
            attempt_id: authority.attempt_id,
            transaction_id: authority.transaction_id,
            commitment_hex: authority.commitment_hex,
        })
    }

    pub(super) fn mark_accepted(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
        authority: AmbiguousEnvelopeAuthority,
    ) -> Result<AcceptedEnvelopeAuthority, EnvelopeStoreError> {
        self.transition_entry(
            authenticator,
            &authority.attempt_id,
            &authority.transaction_id,
            &authority.commitment_hex,
            RetentionState::Ambiguous,
            RetentionState::Accepted,
        )?;
        Ok(AcceptedEnvelopeAuthority {
            attempt_id: authority.attempt_id,
            transaction_id: authority.transaction_id,
            commitment_hex: authority.commitment_hex,
        })
    }

    pub(super) fn remove_prewrite(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
        authority: PreparedEnvelopeAuthority,
    ) -> Result<(), EnvelopeStoreError> {
        let _lock = lock_store()?;
        let mut loaded = self.load_authenticated_unlocked(authenticator)?;
        let container = loaded
            .container
            .as_mut()
            .ok_or(EnvelopeStoreError::InvalidTransition)?;
        let index = container
            .entries
            .iter()
            .position(|entry| {
                entry.attempt_id == authority.attempt_id
                    && entry.transaction_id == authority.transaction_id
                    && entry.envelope_commitment_hex == authority.commitment_hex
                    && entry.retention_state == RetentionState::Prepared
            })
            .ok_or(EnvelopeStoreError::InvalidTransition)?;
        container.entries.remove(index);
        let entries = std::mem::take(&mut container.entries);
        self.publish_container(authenticator, &loaded.head, entries)?;
        Ok(())
    }

    pub(super) fn verify_accepted(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
        authority: &AcceptedEnvelopeAuthority,
    ) -> Result<(), EnvelopeStoreError> {
        let _lock = lock_store()?;
        let loaded = self.load_authenticated_unlocked(authenticator)?;
        require_entry(
            loaded.container.as_ref(),
            &authority.attempt_id,
            &authority.transaction_id,
            &authority.commitment_hex,
            RetentionState::Accepted,
        )
    }

    pub(super) fn verify_prepared(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
        authority: &PreparedEnvelopeAuthority,
    ) -> Result<(), EnvelopeStoreError> {
        let _lock = lock_store()?;
        let loaded = self.load_authenticated_unlocked(authenticator)?;
        require_entry(
            loaded.container.as_ref(),
            &authority.attempt_id,
            &authority.transaction_id,
            &authority.commitment_hex,
            RetentionState::Prepared,
        )
    }

    fn transition_entry(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
        attempt_id: &str,
        transaction_id: &str,
        commitment_hex: &str,
        expected: RetentionState,
        next: RetentionState,
    ) -> Result<(), EnvelopeStoreError> {
        let _lock = lock_store()?;
        let mut loaded = self.load_authenticated_unlocked(authenticator)?;
        let container = loaded
            .container
            .as_mut()
            .ok_or(EnvelopeStoreError::InvalidTransition)?;
        let entry = container
            .entries
            .iter_mut()
            .find(|entry| {
                entry.attempt_id == attempt_id
                    && entry.transaction_id == transaction_id
                    && entry.envelope_commitment_hex == commitment_hex
            })
            .ok_or(EnvelopeStoreError::InvalidTransition)?;
        if entry.retention_state != expected {
            return Err(EnvelopeStoreError::InvalidTransition);
        }
        entry.retention_state = next;
        let entries = std::mem::take(&mut container.entries);
        self.publish_container(authenticator, &loaded.head, entries)?;
        Ok(())
    }

    fn ensure_state_binding(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
        binding: &ReconciliationEnvelopeBinding,
        required: RetentionState,
    ) -> Result<(), EnvelopeStoreError> {
        let _lock = lock_store()?;
        let mut loaded = self.load_authenticated_unlocked(authenticator)?;
        let container = loaded
            .container
            .as_mut()
            .ok_or(EnvelopeStoreError::AuthenticationFailed)?;
        let entry = container
            .entries
            .iter_mut()
            .find(|entry| {
                entry.attempt_id == binding.attempt_id()
                    && entry.transaction_id == binding.transaction_id()
                    && entry.envelope_commitment_hex == binding.commitment_hex()
                    && entry.reconciliation_parent_generation == binding.parent_generation()
                    && entry.reconciliation_parent_tag_hex == binding.parent_tag_hex()
                    && entry.reserved_prepared_generation == binding.reserved_prepared_generation()
            })
            .ok_or(EnvelopeStoreError::AuthenticationFailed)?;
        if entry.retention_state == required {
            return Ok(());
        }
        let permitted_repair = matches!(
            (entry.retention_state, required),
            (RetentionState::Prepared, RetentionState::Ambiguous)
                | (RetentionState::Prepared, RetentionState::Accepted)
                | (RetentionState::Ambiguous, RetentionState::Accepted)
        );
        if !permitted_repair {
            return Err(EnvelopeStoreError::InvalidTransition);
        }
        entry.retention_state = required;
        let entries = std::mem::take(&mut container.entries);
        self.publish_container(authenticator, &loaded.head, entries)?;
        Ok(())
    }

    fn load_or_create_genesis(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
    ) -> Result<LoadedStore, EnvelopeStoreError> {
        match self.load_authenticated_unlocked(authenticator) {
            Ok(loaded) => Ok(loaded),
            Err(EnvelopeStoreError::StorageUnavailable)
                if !self.container_path.try_exists().unwrap_or(true)
                    && !self.head_path.try_exists().unwrap_or(true) =>
            {
                let mut head = EnvelopeHead {
                    schema: HEAD_SCHEMA.to_string(),
                    version: HEAD_VERSION,
                    wallet_id: authenticator.wallet_id.clone(),
                    generation: 0,
                    previous_head_tag_hex: zero_tag(),
                    state: EnvelopeHeadState::Committed {
                        position: empty_position(),
                    },
                    authentication_tag_hex: String::new(),
                };
                authenticate_head(authenticator, &mut head)?;
                persist_json(&self.head_path, &head, true, MAX_HEAD_BYTES)?;
                Ok(LoadedStore {
                    container: None,
                    head,
                })
            }
            Err(error) => Err(error),
        }
    }

    fn load_authenticated_unlocked(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
    ) -> Result<LoadedStore, EnvelopeStoreError> {
        let wrapper_bytes = read_protected(&self.container_path, MAX_CONTAINER_BYTES)?;
        let head_bytes = read_protected(&self.head_path, MAX_HEAD_BYTES)?
            .ok_or(EnvelopeStoreError::StorageUnavailable)?;
        let mut head: EnvelopeHead = serde_json::from_slice(&head_bytes)
            .map_err(|_| EnvelopeStoreError::AuthenticationFailed)?;
        verify_head(authenticator, &head)?;
        let wrapper = wrapper_bytes.as_deref().map(decode_wrapper).transpose()?;
        if let Some(wrapper) = wrapper.as_ref() {
            validate_wrapper(authenticator, wrapper)?;
        }
        let actual = wrapper
            .as_ref()
            .map(position_for_wrapper)
            .transpose()?
            .unwrap_or_else(empty_position);
        let recovery = match &head.state {
            EnvelopeHeadState::Committed { position } if *position == actual => None,
            EnvelopeHeadState::Transition {
                previous_generation,
                previous_previous_head_tag_hex,
                previous,
                ..
            } if *previous == actual => Some((
                *previous_generation,
                previous_previous_head_tag_hex.clone(),
                previous.clone(),
            )),
            EnvelopeHeadState::Transition { next, .. } if *next == actual => Some((
                head.generation,
                head.previous_head_tag_hex.clone(),
                next.clone(),
            )),
            _ => return Err(EnvelopeStoreError::AuthenticationFailed),
        };
        if let Some((generation, previous_head_tag_hex, committed_position)) = recovery {
            head.generation = generation;
            head.previous_head_tag_hex = previous_head_tag_hex;
            head.state = EnvelopeHeadState::Committed {
                position: committed_position,
            };
            head.authentication_tag_hex.clear();
            authenticate_head(authenticator, &mut head)?;
            persist_json(&self.head_path, &head, false, MAX_HEAD_BYTES)?;
        }
        let container = wrapper
            .as_ref()
            .map(|wrapper| decrypt_container(authenticator, wrapper))
            .transpose()?;
        if let Some(container) = container.as_ref() {
            validate_container(authenticator, container, &head)?;
        } else if head.generation != 0 {
            return Err(EnvelopeStoreError::AuthenticationFailed);
        }
        Ok(LoadedStore { container, head })
    }

    fn publish_container(
        &self,
        authenticator: &EnvelopeStoreAuthenticator,
        current_head: &EnvelopeHead,
        entries: Vec<EnvelopeEntry>,
    ) -> Result<(), EnvelopeStoreError> {
        let generation = current_head
            .generation
            .checked_add(1)
            .ok_or(EnvelopeStoreError::InvalidTransition)?;
        let previous_head_tag_hex = current_head.authentication_tag_hex.clone();
        let container = PlaintextContainer {
            schema: STORE_SCHEMA.to_string(),
            version: STORE_VERSION,
            wallet_id: authenticator.wallet_id.clone(),
            generation,
            previous_head_tag_hex: previous_head_tag_hex.clone(),
            entries,
        };
        validate_entry_set(
            &authenticator.wallet_id,
            &authenticator.sender_address,
            &container.entries,
        )?;
        let wrapper = encrypt_container(authenticator, &container)?;
        let encoded_wrapper =
            serde_json::to_vec(&wrapper).map_err(|_| EnvelopeStoreError::InvalidRequest)?;
        if encoded_wrapper.is_empty() || encoded_wrapper.len() > MAX_CONTAINER_BYTES {
            return Err(EnvelopeStoreError::StoreFull);
        }
        let next_position = position_for_wrapper(&wrapper)?;
        let previous_position = match &current_head.state {
            EnvelopeHeadState::Committed { position } => position.clone(),
            EnvelopeHeadState::Transition { .. } => {
                return Err(EnvelopeStoreError::InvalidTransition)
            }
        };
        let mut transition = EnvelopeHead {
            schema: HEAD_SCHEMA.to_string(),
            version: HEAD_VERSION,
            wallet_id: authenticator.wallet_id.clone(),
            generation,
            previous_head_tag_hex: previous_head_tag_hex.clone(),
            state: EnvelopeHeadState::Transition {
                previous_generation: current_head.generation,
                previous_previous_head_tag_hex: current_head.previous_head_tag_hex.clone(),
                previous: previous_position,
                next: next_position.clone(),
            },
            authentication_tag_hex: String::new(),
        };
        authenticate_head(authenticator, &mut transition)?;
        persist_json(&self.head_path, &transition, false, MAX_HEAD_BYTES)?;
        persist_bytes(
            &self.container_path,
            &encoded_wrapper,
            current_head.generation == 0,
            MAX_CONTAINER_BYTES,
        )?;
        let mut committed = EnvelopeHead {
            schema: HEAD_SCHEMA.to_string(),
            version: HEAD_VERSION,
            wallet_id: authenticator.wallet_id.clone(),
            generation,
            previous_head_tag_hex,
            state: EnvelopeHeadState::Committed {
                position: next_position,
            },
            authentication_tag_hex: String::new(),
        };
        authenticate_head(authenticator, &mut committed)?;
        persist_json(&self.head_path, &committed, false, MAX_HEAD_BYTES)?;
        Ok(())
    }
}

impl EnvelopeStoreAuthenticator {
    pub(super) fn new(wallet_id: &str, seed: &WalletSeed) -> Result<Self, EnvelopeStoreError> {
        validate_wallet_id(wallet_id)?;
        Ok(Self {
            wallet_id: wallet_id.to_string(),
            sender_address: super::account::derive_account_identity(seed).address,
            encryption_key: derive_key(seed, ENCRYPTION_KEY_CONTEXT),
            head_key: derive_key(seed, HEAD_KEY_CONTEXT),
        })
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::wallet::{
        lifecycle::WalletCustodyPathAuthority,
        reconciliation::{ReconciliationAuthenticator, ReconciliationStore},
        transaction::{canonical_transaction_id, sign_cash_transfer_for_test, CashTransferDraft},
    };
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    fn fixture() -> (
        PathBuf,
        WalletCustodyPathAuthority,
        WalletSeed,
        VisionTransaction,
    ) {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let directory = std::env::temp_dir().join(format!(
            "vision-envelope-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        storage_security::protect_directory(&directory).unwrap();
        let vault_path = directory.join("wallet.vault.json");
        let custody = WalletCustodyPathAuthority::issue_for_test(&vault_path);
        let seed = WalletSeed::for_test(7);
        let transaction = sign_cash_transfer_for_test(
            &seed,
            &CashTransferDraft {
                nonce: 1,
                recipient: "22".repeat(32),
                amount_raw_units: 42,
                tip_raw_units: 0,
                fee_limit_raw_units: 201,
            },
        )
        .unwrap();
        (directory, custody, seed, transaction)
    }

    fn publish_fixture(
        custody: &WalletCustodyPathAuthority,
        seed: &WalletSeed,
        transaction: &VisionTransaction,
    ) -> (
        EnvelopeStore,
        EnvelopeStoreAuthenticator,
        PreparedEnvelopeAuthority,
    ) {
        let store = EnvelopeStore::for_custody(custody).unwrap();
        let authenticator = EnvelopeStoreAuthenticator::new("primary", seed).unwrap();
        let reconciliation_store = ReconciliationStore::for_custody(custody).unwrap();
        let reconciliation_authenticator =
            ReconciliationAuthenticator::new("primary", seed).unwrap();
        let reservation = reconciliation_store
            .reserve_prepared(&reconciliation_authenticator)
            .unwrap();
        let body = Zeroizing::new(serde_json::to_vec(transaction).unwrap());
        let transaction_id = canonical_transaction_id(transaction).unwrap();
        let body_digest = digest_hex(BODY_DIGEST_CONTEXT, body.as_slice());
        let authority = store
            .publish_prepared(
                &authenticator,
                EnvelopeEntryInput {
                    wallet_id: "primary",
                    attempt_id: &"11".repeat(32),
                    transaction_id: &transaction_id,
                    transaction,
                    exact_body: body.as_slice(),
                    signed_body_digest_hex: &body_digest,
                    compatibility_contract_digest_hex: &"33".repeat(32),
                    created_at_unix_ms: 1,
                },
                &reservation,
            )
            .unwrap();
        (store, authenticator, authority)
    }

    #[test]
    fn first_publication_is_encrypted_authenticated_and_restart_readable() {
        let (directory, custody, seed, transaction) = fixture();
        let (store, authenticator, authority) = publish_fixture(&custody, &seed, &transaction);
        store.verify_prepared(&authenticator, &authority).unwrap();
        let wire = fs::read(&store.container_path).unwrap();
        let text = String::from_utf8(wire).unwrap();
        assert!(!text.contains(&transaction.sig));
        assert!(!text.contains(&hex::encode(serde_json::to_vec(&transaction).unwrap())));
        let reopened = EnvelopeStore::for_custody(&custody).unwrap();
        reopened
            .verify_prepared(&authenticator, &authority)
            .unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn wrong_wallet_key_and_ciphertext_mutation_fail_closed() {
        let (directory, custody, seed, transaction) = fixture();
        let (store, authenticator, authority) = publish_fixture(&custody, &seed, &transaction);
        let wrong = EnvelopeStoreAuthenticator::new("primary", &WalletSeed::for_test(8)).unwrap();
        assert_eq!(
            store.verify_prepared(&wrong, &authority).err(),
            Some(EnvelopeStoreError::AuthenticationFailed)
        );
        let mut wire: serde_json::Value =
            serde_json::from_slice(&fs::read(&store.container_path).unwrap()).unwrap();
        wire["ciphertext_hex"] = serde_json::Value::String("00".repeat(32));
        fs::write(&store.container_path, serde_json::to_vec(&wire).unwrap()).unwrap();
        storage_security::protect_file(&store.container_path).unwrap();
        assert!(store.verify_prepared(&authenticator, &authority).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn retention_is_linear_and_accepted_entries_are_not_prewrite_removable() {
        let (directory, custody, seed, transaction) = fixture();
        let (store, authenticator, prepared) = publish_fixture(&custody, &seed, &transaction);
        let ambiguous = store.mark_ambiguous(&authenticator, prepared).unwrap();
        let accepted = store.mark_accepted(&authenticator, ambiguous).unwrap();
        store.verify_accepted(&authenticator, &accepted).unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn prepared_orphan_cleanup_requires_the_exact_empty_reconciliation_reservation() {
        let (directory, custody, seed, transaction) = fixture();
        let (store, authenticator, prepared) = publish_fixture(&custody, &seed, &transaction);
        let reconciliation_store = ReconciliationStore::for_custody(&custody).unwrap();
        let reconciliation_authenticator =
            ReconciliationAuthenticator::new("primary", &seed).unwrap();
        let reservation = reconciliation_store
            .empty_head_reservation(&reconciliation_authenticator)
            .unwrap()
            .unwrap();
        let orphan = store
            .identify_prepared_orphan(&authenticator, &reservation)
            .unwrap()
            .unwrap();
        assert_eq!(orphan.transaction_id(), prepared.transaction_id);
        assert_eq!(orphan.commitment_hex(), prepared.commitment_hex);
        store
            .cleanup_prepared_orphan(&authenticator, orphan)
            .unwrap();
        assert!(!store
            .contains_transaction_id(&authenticator, &prepared.transaction_id)
            .unwrap());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn orphan_cleanup_refuses_ambiguous_or_accepted_entries() {
        for accept in [false, true] {
            let (directory, custody, seed, transaction) = fixture();
            let (store, authenticator, prepared) = publish_fixture(&custody, &seed, &transaction);
            let reconciliation_store = ReconciliationStore::for_custody(&custody).unwrap();
            let reconciliation_authenticator =
                ReconciliationAuthenticator::new("primary", &seed).unwrap();
            let reservation = reconciliation_store
                .empty_head_reservation(&reconciliation_authenticator)
                .unwrap()
                .unwrap();
            let ambiguous = store.mark_ambiguous(&authenticator, prepared).unwrap();
            if accept {
                store.mark_accepted(&authenticator, ambiguous).unwrap();
                assert!(store
                    .identify_prepared_orphan(&authenticator, &reservation)
                    .unwrap()
                    .is_none());
            } else {
                assert_eq!(
                    store
                        .identify_prepared_orphan(&authenticator, &reservation)
                        .err(),
                    Some(EnvelopeStoreError::AuthenticationFailed)
                );
            }
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn orphan_staging_file_blocks_authenticated_reads() {
        let (directory, custody, seed, transaction) = fixture();
        let (store, authenticator, authority) = publish_fixture(&custody, &seed, &transaction);
        let staging = directory.join(format!("{STAGING_PREFIX}interrupted.tmp"));
        fs::write(&staging, b"interrupted").unwrap();
        storage_security::protect_file(&staging).unwrap();
        assert_eq!(
            store.verify_prepared(&authenticator, &authority).err(),
            Some(EnvelopeStoreError::StorageUnavailable)
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn authenticated_head_transition_recovers_the_exact_old_or_new_container() {
        for recover_new in [false, true] {
            let (directory, custody, seed, transaction) = fixture();
            let (store, authenticator, prepared) = publish_fixture(&custody, &seed, &transaction);
            let old_wrapper = fs::read(&store.container_path).unwrap();
            let old_head_bytes = fs::read(&store.head_path).unwrap();
            let old_head: EnvelopeHead = serde_json::from_slice(&old_head_bytes).unwrap();
            let prepared_identity = (
                prepared.attempt_id.clone(),
                prepared.transaction_id.clone(),
                prepared.commitment_hex.clone(),
                prepared.reconciliation_parent_generation,
                prepared.reconciliation_parent_tag_hex.clone(),
                prepared.reserved_prepared_generation,
            );
            let ambiguous = store.mark_ambiguous(&authenticator, prepared).unwrap();
            let new_wrapper = fs::read(&store.container_path).unwrap();
            let new_head: EnvelopeHead =
                serde_json::from_slice(&fs::read(&store.head_path).unwrap()).unwrap();
            let previous = match &old_head.state {
                EnvelopeHeadState::Committed { position } => position.clone(),
                EnvelopeHeadState::Transition { .. } => unreachable!(),
            };
            let next = match &new_head.state {
                EnvelopeHeadState::Committed { position } => position.clone(),
                EnvelopeHeadState::Transition { .. } => unreachable!(),
            };
            let mut transition = EnvelopeHead {
                schema: HEAD_SCHEMA.to_string(),
                version: HEAD_VERSION,
                wallet_id: "primary".to_string(),
                generation: new_head.generation,
                previous_head_tag_hex: old_head.authentication_tag_hex.clone(),
                state: EnvelopeHeadState::Transition {
                    previous_generation: old_head.generation,
                    previous_previous_head_tag_hex: old_head.previous_head_tag_hex.clone(),
                    previous,
                    next,
                },
                authentication_tag_hex: String::new(),
            };
            authenticate_head(&authenticator, &mut transition).unwrap();
            fs::write(&store.head_path, serde_json::to_vec(&transition).unwrap()).unwrap();
            storage_security::protect_file(&store.head_path).unwrap();
            fs::write(
                &store.container_path,
                if recover_new {
                    &new_wrapper
                } else {
                    &old_wrapper
                },
            )
            .unwrap();
            storage_security::protect_file(&store.container_path).unwrap();
            if recover_new {
                let recovered = store.load_authenticated_unlocked(&authenticator).unwrap();
                require_entry(
                    recovered.container.as_ref(),
                    &ambiguous.attempt_id,
                    &ambiguous.transaction_id,
                    &ambiguous.commitment_hex,
                    RetentionState::Ambiguous,
                )
                .unwrap();
            } else {
                store
                    .verify_prepared(
                        &authenticator,
                        &PreparedEnvelopeAuthority {
                            attempt_id: prepared_identity.0,
                            transaction_id: prepared_identity.1,
                            commitment_hex: prepared_identity.2,
                            reconciliation_parent_generation: prepared_identity.3,
                            reconciliation_parent_tag_hex: prepared_identity.4,
                            reserved_prepared_generation: prepared_identity.5,
                        },
                    )
                    .unwrap();
                assert_eq!(fs::read(&store.head_path).unwrap(), old_head_bytes);
            }
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn transaction_identifier_collision_and_lock_contention_fail_before_publication() {
        let (directory, custody, seed, transaction) = fixture();
        let (store, authenticator, prepared) = publish_fixture(&custody, &seed, &transaction);
        let ambiguous = store.mark_ambiguous(&authenticator, prepared).unwrap();
        let accepted = store.mark_accepted(&authenticator, ambiguous).unwrap();
        let reconciliation_store = ReconciliationStore::for_custody(&custody).unwrap();
        let reconciliation_authenticator =
            ReconciliationAuthenticator::new("primary", &seed).unwrap();
        let reservation = reconciliation_store
            .reserve_prepared(&reconciliation_authenticator)
            .unwrap();
        let body = Zeroizing::new(serde_json::to_vec(&transaction).unwrap());
        let transaction_id = canonical_transaction_id(&transaction).unwrap();
        let body_digest = digest_hex(BODY_DIGEST_CONTEXT, body.as_slice());
        let input = EnvelopeEntryInput {
            wallet_id: "primary",
            attempt_id: &"44".repeat(32),
            transaction_id: &transaction_id,
            transaction: &transaction,
            exact_body: body.as_slice(),
            signed_body_digest_hex: &body_digest,
            compatibility_contract_digest_hex: &"33".repeat(32),
            created_at_unix_ms: 2,
        };
        assert_eq!(
            store
                .publish_prepared(&authenticator, input, &reservation)
                .err(),
            Some(EnvelopeStoreError::Collision)
        );
        store.verify_accepted(&authenticator, &accepted).unwrap();
        let held = STORE_LOCK.lock().unwrap();
        assert_eq!(
            store.verify_accepted(&authenticator, &accepted).err(),
            Some(EnvelopeStoreError::StorageUnavailable)
        );
        drop(held);
        fs::remove_dir_all(directory).unwrap();
    }
}

impl PreparedEnvelopeAuthority {
    pub(super) fn attempt_id(&self) -> &str {
        &self.attempt_id
    }

    pub(super) fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    pub(super) fn commitment_hex(&self) -> &str {
        &self.commitment_hex
    }

    pub(super) const fn reconciliation_parent_generation(&self) -> u64 {
        self.reconciliation_parent_generation
    }

    pub(super) fn reconciliation_parent_tag_hex(&self) -> &str {
        &self.reconciliation_parent_tag_hex
    }

    pub(super) const fn reserved_prepared_generation(&self) -> u64 {
        self.reserved_prepared_generation
    }
}

impl AcceptedEnvelopeAuthority {
    pub(super) fn transaction_id(&self) -> &str {
        &self.transaction_id
    }
    pub(super) fn commitment_hex(&self) -> &str {
        &self.commitment_hex
    }
}

fn build_entry(
    authenticator: &EnvelopeStoreAuthenticator,
    input: EnvelopeEntryInput<'_>,
    reservation: &ReconciliationReservation,
) -> Result<EnvelopeEntry, EnvelopeStoreError> {
    if input.wallet_id != authenticator.wallet_id
        || input.transaction.sender_pubkey != authenticator.sender_address
        || input.exact_body.is_empty()
        || input.exact_body.len() > MAX_BODY_BYTES
        || serde_json::to_vec(input.transaction).map_err(|_| EnvelopeStoreError::InvalidRequest)?
            != input.exact_body
        || canonical_transaction_id(input.transaction)
            .map_err(|_| EnvelopeStoreError::InvalidRequest)?
            != input.transaction_id
        || verify_signed_transaction(input.transaction).is_err()
        || digest_hex(BODY_DIGEST_CONTEXT, input.exact_body) != input.signed_body_digest_hex
        || !is_lower_hex(input.attempt_id, 32)
        || !is_lower_hex(input.transaction_id, 32)
        || !is_lower_hex(input.signed_body_digest_hex, 32)
        || !is_lower_hex(input.compatibility_contract_digest_hex, 32)
    {
        return Err(EnvelopeStoreError::InvalidRequest);
    }
    let commitment_hex = envelope_commitment(&input, reservation)?;
    Ok(EnvelopeEntry {
        attempt_id: input.attempt_id.to_string(),
        transaction_id: input.transaction_id.to_string(),
        transaction: input.transaction.clone(),
        exact_body_hex: hex::encode(input.exact_body),
        signed_body_digest_hex: input.signed_body_digest_hex.to_string(),
        compatibility_contract_digest_hex: input.compatibility_contract_digest_hex.to_string(),
        reconciliation_parent_generation: reservation.parent_generation(),
        reconciliation_parent_tag_hex: reservation.parent_authentication_tag_hex(),
        reserved_prepared_generation: reservation.prepared_generation(),
        envelope_commitment_hex: commitment_hex,
        created_at_unix_ms: input.created_at_unix_ms,
        retention_state: RetentionState::Prepared,
    })
}

fn envelope_commitment(
    input: &EnvelopeEntryInput<'_>,
    reservation: &ReconciliationReservation,
) -> Result<String, EnvelopeStoreError> {
    validate_wallet_id(input.wallet_id)?;
    let wallet_len =
        u16::try_from(input.wallet_id.len()).map_err(|_| EnvelopeStoreError::InvalidRequest)?;
    let body_len =
        u32::try_from(input.exact_body.len()).map_err(|_| EnvelopeStoreError::InvalidRequest)?;
    let attempt = decode_fixed::<32>(input.attempt_id)?;
    let transaction_id = decode_fixed::<32>(input.transaction_id)?;
    let body_digest = decode_fixed::<32>(input.signed_body_digest_hex)?;
    let compatibility_digest = decode_fixed::<32>(input.compatibility_contract_digest_hex)?;
    let parent_tag = decode_fixed::<32>(&reservation.parent_authentication_tag_hex())?;
    let mut hasher = blake3::Hasher::new_derive_key(COMMITMENT_CONTEXT);
    hasher.update(&1_u32.to_be_bytes());
    hasher.update(&wallet_len.to_be_bytes());
    hasher.update(input.wallet_id.as_bytes());
    hasher.update(&attempt);
    hasher.update(&transaction_id);
    hasher.update(&body_len.to_be_bytes());
    hasher.update(input.exact_body);
    hasher.update(&body_digest);
    hasher.update(&compatibility_digest);
    hasher.update(&reservation.parent_generation().to_be_bytes());
    hasher.update(&parent_tag);
    hasher.update(&reservation.prepared_generation().to_be_bytes());
    Ok(hasher.finalize().to_hex().to_string())
}

fn validate_container(
    authenticator: &EnvelopeStoreAuthenticator,
    container: &PlaintextContainer,
    head: &EnvelopeHead,
) -> Result<(), EnvelopeStoreError> {
    if container.schema != STORE_SCHEMA
        || container.version != STORE_VERSION
        || container.wallet_id != authenticator.wallet_id
        || container.generation != head.generation
        || container.previous_head_tag_hex != head.previous_head_tag_hex
    {
        return Err(EnvelopeStoreError::AuthenticationFailed);
    }
    validate_entry_set(
        &container.wallet_id,
        &authenticator.sender_address,
        &container.entries,
    )
}

fn validate_entry_set(
    wallet_id: &str,
    sender_address: &str,
    entries: &[EnvelopeEntry],
) -> Result<(), EnvelopeStoreError> {
    if entries.len() > MAX_ENTRIES {
        return Err(EnvelopeStoreError::StoreFull);
    }
    let mut nonterminal = 0_usize;
    for (index, entry) in entries.iter().enumerate() {
        validate_entry(wallet_id, sender_address, entry)?;
        if entry.retention_state != RetentionState::Accepted {
            nonterminal += 1;
        }
        if let Some(previous) = index.checked_sub(1).and_then(|i| entries.get(i)) {
            if previous.attempt_id >= entry.attempt_id
                || previous.transaction_id == entry.transaction_id
                || previous.envelope_commitment_hex == entry.envelope_commitment_hex
            {
                return Err(EnvelopeStoreError::Collision);
            }
        }
        if entries[..index].iter().any(|prior| {
            prior.transaction_id == entry.transaction_id
                || prior.envelope_commitment_hex == entry.envelope_commitment_hex
        }) {
            return Err(EnvelopeStoreError::Collision);
        }
    }
    if nonterminal > 1 {
        return Err(EnvelopeStoreError::InvalidTransition);
    }
    Ok(())
}

fn validate_entry(
    wallet_id: &str,
    sender_address: &str,
    entry: &EnvelopeEntry,
) -> Result<(), EnvelopeStoreError> {
    if entry.exact_body_hex.is_empty()
        || entry.exact_body_hex.len() > MAX_BODY_BYTES * 2
        || !entry.exact_body_hex.len().is_multiple_of(2)
        || !entry.exact_body_hex.bytes().all(is_lower_hex_byte)
    {
        return Err(EnvelopeStoreError::AuthenticationFailed);
    }
    let body = Zeroizing::new(
        hex::decode(&entry.exact_body_hex).map_err(|_| EnvelopeStoreError::AuthenticationFailed)?,
    );
    if body.is_empty()
        || body.len() > MAX_BODY_BYTES
        || !is_lower_hex(&entry.attempt_id, 32)
        || !is_lower_hex(&entry.transaction_id, 32)
        || !is_lower_hex(&entry.signed_body_digest_hex, 32)
        || !is_lower_hex(&entry.compatibility_contract_digest_hex, 32)
        || !is_lower_hex(&entry.reconciliation_parent_tag_hex, 32)
        || !is_lower_hex(&entry.envelope_commitment_hex, 32)
        || entry.transaction.sender_pubkey != sender_address
        || entry.reserved_prepared_generation
            != entry
                .reconciliation_parent_generation
                .checked_add(1)
                .unwrap_or(0)
        || serde_json::to_vec(&entry.transaction)
            .map_err(|_| EnvelopeStoreError::AuthenticationFailed)?
            != body.as_slice()
        || canonical_transaction_id(&entry.transaction)
            .map_err(|_| EnvelopeStoreError::AuthenticationFailed)?
            != entry.transaction_id
        || verify_signed_transaction(&entry.transaction).is_err()
        || digest_hex(BODY_DIGEST_CONTEXT, body.as_slice()) != entry.signed_body_digest_hex
    {
        return Err(EnvelopeStoreError::AuthenticationFailed);
    }
    let reservation = ReservationView {
        parent_generation: entry.reconciliation_parent_generation,
        parent_tag_hex: &entry.reconciliation_parent_tag_hex,
        prepared_generation: entry.reserved_prepared_generation,
    };
    let expected = commitment_from_entry(wallet_id, entry, body.as_slice(), reservation)?;
    if expected != entry.envelope_commitment_hex {
        return Err(EnvelopeStoreError::AuthenticationFailed);
    }
    Ok(())
}

struct ReservationView<'a> {
    parent_generation: u64,
    parent_tag_hex: &'a str,
    prepared_generation: u64,
}

fn commitment_from_entry(
    wallet_id: &str,
    entry: &EnvelopeEntry,
    body: &[u8],
    reservation: ReservationView<'_>,
) -> Result<String, EnvelopeStoreError> {
    validate_wallet_id(wallet_id)?;
    let wallet_len =
        u16::try_from(wallet_id.len()).map_err(|_| EnvelopeStoreError::InvalidRequest)?;
    let body_len = u32::try_from(body.len()).map_err(|_| EnvelopeStoreError::InvalidRequest)?;
    let mut hasher = blake3::Hasher::new_derive_key(COMMITMENT_CONTEXT);
    hasher.update(&1_u32.to_be_bytes());
    hasher.update(&wallet_len.to_be_bytes());
    hasher.update(wallet_id.as_bytes());
    hasher.update(&decode_fixed::<32>(&entry.attempt_id)?);
    hasher.update(&decode_fixed::<32>(&entry.transaction_id)?);
    hasher.update(&body_len.to_be_bytes());
    hasher.update(body);
    hasher.update(&decode_fixed::<32>(&entry.signed_body_digest_hex)?);
    hasher.update(&decode_fixed::<32>(
        &entry.compatibility_contract_digest_hex,
    )?);
    hasher.update(&reservation.parent_generation.to_be_bytes());
    hasher.update(&decode_fixed::<32>(reservation.parent_tag_hex)?);
    hasher.update(&reservation.prepared_generation.to_be_bytes());
    Ok(hasher.finalize().to_hex().to_string())
}

fn validate_collision(
    container: Option<&PlaintextContainer>,
    candidate: &EnvelopeEntry,
) -> Result<(), EnvelopeStoreError> {
    if container.is_some_and(|container| {
        container.entries.iter().any(|entry| {
            entry.attempt_id == candidate.attempt_id
                || entry.transaction_id == candidate.transaction_id
                || entry.envelope_commitment_hex == candidate.envelope_commitment_hex
        })
    }) {
        Err(EnvelopeStoreError::Collision)
    } else {
        Ok(())
    }
}

fn require_entry(
    container: Option<&PlaintextContainer>,
    attempt_id: &str,
    transaction_id: &str,
    commitment_hex: &str,
    state: RetentionState,
) -> Result<(), EnvelopeStoreError> {
    let matches = container
        .into_iter()
        .flat_map(|container| container.entries.iter())
        .filter(|entry| {
            entry.attempt_id == attempt_id
                && entry.transaction_id == transaction_id
                && entry.envelope_commitment_hex == commitment_hex
                && entry.retention_state == state
        })
        .count();
    if matches == 1 {
        Ok(())
    } else {
        Err(EnvelopeStoreError::AuthenticationFailed)
    }
}

fn encrypt_container(
    authenticator: &EnvelopeStoreAuthenticator,
    container: &PlaintextContainer,
) -> Result<CiphertextWrapper, EnvelopeStoreError> {
    let plaintext = Zeroizing::new(
        serde_json::to_vec(container).map_err(|_| EnvelopeStoreError::InvalidRequest)?,
    );
    if plaintext.is_empty() || plaintext.len() > MAX_CONTAINER_BYTES {
        return Err(EnvelopeStoreError::StoreFull);
    }
    let mut nonce = [0_u8; NONCE_BYTES];
    getrandom::fill(&mut nonce).map_err(|_| EnvelopeStoreError::StorageUnavailable)?;
    let ciphertext_length =
        u64::try_from(plaintext.len() + 16).map_err(|_| EnvelopeStoreError::StoreFull)?;
    let aad = CiphertextAad {
        schema: WRAPPER_SCHEMA,
        version: WRAPPER_VERSION,
        wallet_id: &authenticator.wallet_id,
        generation: container.generation,
        previous_head_tag_hex: &container.previous_head_tag_hex,
        ciphertext_length,
        entry_count: u32::try_from(container.entries.len())
            .map_err(|_| EnvelopeStoreError::StoreFull)?,
    };
    let aad = serde_json::to_vec(&aad).map_err(|_| EnvelopeStoreError::InvalidRequest)?;
    let key = <&Key>::try_from(authenticator.encryption_key.expose_secret().as_slice())
        .map_err(|_| EnvelopeStoreError::AuthenticationFailed)?;
    let nonce_ref =
        XNonce::try_from(nonce.as_slice()).map_err(|_| EnvelopeStoreError::AuthenticationFailed)?;
    let ciphertext = XChaCha20Poly1305::new(key)
        .encrypt(
            &nonce_ref,
            Payload {
                msg: plaintext.as_slice(),
                aad: &aad,
            },
        )
        .map_err(|_| EnvelopeStoreError::AuthenticationFailed)?;
    Ok(CiphertextWrapper {
        schema: WRAPPER_SCHEMA.to_string(),
        version: WRAPPER_VERSION,
        wallet_id: authenticator.wallet_id.clone(),
        generation: container.generation,
        previous_head_tag_hex: container.previous_head_tag_hex.clone(),
        nonce_hex: hex::encode(nonce),
        ciphertext_length,
        entry_count: u32::try_from(container.entries.len())
            .map_err(|_| EnvelopeStoreError::StoreFull)?,
        ciphertext_hex: hex::encode(ciphertext),
    })
}

fn decrypt_container(
    authenticator: &EnvelopeStoreAuthenticator,
    wrapper: &CiphertextWrapper,
) -> Result<PlaintextContainer, EnvelopeStoreError> {
    validate_wrapper(authenticator, wrapper)?;
    let nonce = decode_fixed::<NONCE_BYTES>(&wrapper.nonce_hex)?;
    let ciphertext = Zeroizing::new(
        hex::decode(&wrapper.ciphertext_hex)
            .map_err(|_| EnvelopeStoreError::AuthenticationFailed)?,
    );
    if u64::try_from(ciphertext.len()).ok() != Some(wrapper.ciphertext_length) {
        return Err(EnvelopeStoreError::AuthenticationFailed);
    }
    let aad = CiphertextAad {
        schema: WRAPPER_SCHEMA,
        version: WRAPPER_VERSION,
        wallet_id: &wrapper.wallet_id,
        generation: wrapper.generation,
        previous_head_tag_hex: &wrapper.previous_head_tag_hex,
        ciphertext_length: wrapper.ciphertext_length,
        entry_count: wrapper.entry_count,
    };
    let aad = serde_json::to_vec(&aad).map_err(|_| EnvelopeStoreError::AuthenticationFailed)?;
    let key = <&Key>::try_from(authenticator.encryption_key.expose_secret().as_slice())
        .map_err(|_| EnvelopeStoreError::AuthenticationFailed)?;
    let nonce_ref =
        XNonce::try_from(nonce.as_slice()).map_err(|_| EnvelopeStoreError::AuthenticationFailed)?;
    let plaintext = Zeroizing::new(
        XChaCha20Poly1305::new(key)
            .decrypt(
                &nonce_ref,
                Payload {
                    msg: ciphertext.as_slice(),
                    aad: &aad,
                },
            )
            .map_err(|_| EnvelopeStoreError::AuthenticationFailed)?,
    );
    let container: PlaintextContainer = serde_json::from_slice(plaintext.as_slice())
        .map_err(|_| EnvelopeStoreError::AuthenticationFailed)?;
    if container.entries.len() != wrapper.entry_count as usize {
        return Err(EnvelopeStoreError::AuthenticationFailed);
    }
    Ok(container)
}

fn validate_wrapper(
    authenticator: &EnvelopeStoreAuthenticator,
    wrapper: &CiphertextWrapper,
) -> Result<(), EnvelopeStoreError> {
    if wrapper.schema != WRAPPER_SCHEMA
        || wrapper.version != WRAPPER_VERSION
        || wrapper.wallet_id != authenticator.wallet_id
        || wrapper.generation == 0
        || !is_lower_hex(&wrapper.previous_head_tag_hex, 32)
        || !is_lower_hex(&wrapper.nonce_hex, NONCE_BYTES)
        || wrapper.ciphertext_length < 16
        || wrapper.ciphertext_length > MAX_CONTAINER_BYTES as u64
        || wrapper.entry_count as usize > MAX_ENTRIES
        || wrapper.ciphertext_hex.len() != wrapper.ciphertext_length as usize * 2
        || !wrapper.ciphertext_hex.bytes().all(is_lower_hex_byte)
    {
        return Err(EnvelopeStoreError::AuthenticationFailed);
    }
    Ok(())
}

fn decode_wrapper(bytes: &[u8]) -> Result<CiphertextWrapper, EnvelopeStoreError> {
    serde_json::from_slice(bytes).map_err(|_| EnvelopeStoreError::AuthenticationFailed)
}

fn position_for_wrapper(
    wrapper: &CiphertextWrapper,
) -> Result<EnvelopePosition, EnvelopeStoreError> {
    let ciphertext = hex::decode(&wrapper.ciphertext_hex)
        .map_err(|_| EnvelopeStoreError::AuthenticationFailed)?;
    Ok(EnvelopePosition {
        container_generation: wrapper.generation,
        ciphertext_digest_hex: digest_hex(
            "com.vision.desktop.wallet-envelope-ciphertext.v1",
            &ciphertext,
        ),
        ciphertext_length: wrapper.ciphertext_length,
        nonce_hex: wrapper.nonce_hex.clone(),
        entry_count: wrapper.entry_count,
    })
}

fn empty_position() -> EnvelopePosition {
    EnvelopePosition {
        container_generation: 0,
        ciphertext_digest_hex: zero_tag(),
        ciphertext_length: 0,
        nonce_hex: "00".repeat(NONCE_BYTES),
        entry_count: 0,
    }
}

fn authenticate_head(
    authenticator: &EnvelopeStoreAuthenticator,
    head: &mut EnvelopeHead,
) -> Result<(), EnvelopeStoreError> {
    head.authentication_tag_hex.clear();
    let payload = serde_json::to_vec(head).map_err(|_| EnvelopeStoreError::InvalidRequest)?;
    let mut hasher = blake3::Hasher::new_keyed(authenticator.head_key.expose_secret());
    hasher.update(HEAD_DOMAIN);
    hasher.update(&[0]);
    hasher.update(&payload);
    head.authentication_tag_hex = hasher.finalize().to_hex().to_string();
    Ok(())
}

fn verify_head(
    authenticator: &EnvelopeStoreAuthenticator,
    head: &EnvelopeHead,
) -> Result<(), EnvelopeStoreError> {
    if head.schema != HEAD_SCHEMA
        || head.version != HEAD_VERSION
        || head.wallet_id != authenticator.wallet_id
        || !is_lower_hex(&head.previous_head_tag_hex, 32)
        || !is_lower_hex(&head.authentication_tag_hex, 32)
    {
        return Err(EnvelopeStoreError::AuthenticationFailed);
    }
    let supplied = decode_fixed::<32>(&head.authentication_tag_hex)?;
    let bytes = serde_json::to_vec(head).map_err(|_| EnvelopeStoreError::AuthenticationFailed)?;
    let mut unsigned: EnvelopeHead =
        serde_json::from_slice(&bytes).map_err(|_| EnvelopeStoreError::AuthenticationFailed)?;
    unsigned.authentication_tag_hex.clear();
    authenticate_head(authenticator, &mut unsigned)?;
    let expected = decode_fixed::<32>(&unsigned.authentication_tag_hex)?;
    if !constant_time_equal(&supplied, &expected)
        || (head.generation == 0
            && (!matches!(&head.state, EnvelopeHeadState::Committed { position } if *position == empty_position())
                || head.previous_head_tag_hex != zero_tag()))
    {
        return Err(EnvelopeStoreError::AuthenticationFailed);
    }
    if let EnvelopeHeadState::Transition {
        previous_generation,
        previous_previous_head_tag_hex,
        ..
    } = &head.state
    {
        if previous_generation.checked_add(1) != Some(head.generation)
            || !is_lower_hex(previous_previous_head_tag_hex, 32)
        {
            return Err(EnvelopeStoreError::AuthenticationFailed);
        }
    }
    Ok(())
}

fn derive_key(seed: &WalletSeed, context: &'static str) -> SecretBox<[u8; KEY_BYTES]> {
    SecretBox::<[u8; KEY_BYTES]>::init_with_mut(|output| {
        let mut hasher = blake3::Hasher::new_derive_key(context);
        seed.with_exposed(|bytes| hasher.update(bytes));
        hasher.finalize_xof().fill(output);
        hasher.reset();
    })
}

fn digest_hex(context: &'static str, bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new_derive_key(context);
    hasher.update(bytes);
    hasher.finalize().to_hex().to_string()
}

fn validate_wallet_id(wallet_id: &str) -> Result<(), EnvelopeStoreError> {
    if wallet_id.is_empty()
        || wallet_id.len() > 64
        || !wallet_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Err(EnvelopeStoreError::InvalidRequest)
    } else {
        Ok(())
    }
}

fn decode_fixed<const N: usize>(value: &str) -> Result<[u8; N], EnvelopeStoreError> {
    if !is_lower_hex(value, N) {
        return Err(EnvelopeStoreError::AuthenticationFailed);
    }
    let bytes = hex::decode(value).map_err(|_| EnvelopeStoreError::AuthenticationFailed)?;
    let mut result = [0_u8; N];
    result.copy_from_slice(&bytes);
    Ok(result)
}

fn is_lower_hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes * 2 && value.bytes().all(is_lower_hex_byte)
}

fn is_lower_hex_byte(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

fn zero_tag() -> String {
    "00".repeat(32)
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

fn lock_store() -> Result<MutexGuard<'static, ()>, EnvelopeStoreError> {
    STORE_LOCK
        .try_lock()
        .map_err(|_| EnvelopeStoreError::StorageUnavailable)
}

fn persist_json<T: Serialize>(
    path: &Path,
    value: &T,
    create_new: bool,
    maximum: usize,
) -> Result<(), EnvelopeStoreError> {
    let bytes = serde_json::to_vec(value).map_err(|_| EnvelopeStoreError::InvalidRequest)?;
    persist_bytes(path, &bytes, create_new, maximum)
}

fn persist_bytes(
    path: &Path,
    bytes: &[u8],
    create_new: bool,
    maximum: usize,
) -> Result<(), EnvelopeStoreError> {
    if bytes.is_empty() || bytes.len() > maximum {
        return Err(EnvelopeStoreError::StoreFull);
    }
    let parent = path
        .parent()
        .ok_or(EnvelopeStoreError::StorageUnavailable)?;
    let _directories =
        DirectoryChainGuard::ensure(parent).map_err(|_| EnvelopeStoreError::StorageUnavailable)?;
    storage_security::protect_directory(parent)
        .map_err(|_| EnvelopeStoreError::StorageUnavailable)?;
    let mut suffix = [0_u8; 16];
    getrandom::fill(&mut suffix).map_err(|_| EnvelopeStoreError::StorageUnavailable)?;
    let staging_path = parent.join(format!("{STAGING_PREFIX}{}.tmp", hex::encode(suffix)));
    let mut staging = create_new_publishable_file(&staging_path)
        .map_err(|_| EnvelopeStoreError::StorageUnavailable)?;
    storage_security::protect_open_file(&staging)
        .map_err(|_| EnvelopeStoreError::StorageUnavailable)?;
    staging
        .write_all(bytes)
        .and_then(|_| staging.sync_all())
        .map_err(|_| EnvelopeStoreError::StorageUnavailable)?;
    storage_security::verify_open_file(&staging)
        .map_err(|_| EnvelopeStoreError::StorageUnavailable)?;
    if create_new {
        publish_open_file(&staging, path).map_err(|_| EnvelopeStoreError::StorageUnavailable)
    } else {
        let existing =
            open_existing_file(path).map_err(|_| EnvelopeStoreError::StorageUnavailable)?;
        storage_security::verify_open_file(&existing)
            .map_err(|_| EnvelopeStoreError::StorageUnavailable)?;
        drop(existing);
        replace_with_open_file(&staging, path).map_err(|_| EnvelopeStoreError::StorageUnavailable)
    }
}

fn read_protected(path: &Path, maximum: usize) -> Result<Option<Vec<u8>>, EnvelopeStoreError> {
    let parent = path
        .parent()
        .ok_or(EnvelopeStoreError::StorageUnavailable)?;
    let _directories = match DirectoryChainGuard::open_existing(parent) {
        Ok(guard) => guard,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(EnvelopeStoreError::StorageUnavailable),
    };
    storage_security::verify_directory(parent)
        .map_err(|_| EnvelopeStoreError::StorageUnavailable)?;
    if fs::read_dir(parent)
        .map_err(|_| EnvelopeStoreError::StorageUnavailable)?
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(STAGING_PREFIX)
        })
    {
        return Err(EnvelopeStoreError::StorageUnavailable);
    }
    let file = match open_existing_file(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(EnvelopeStoreError::StorageUnavailable),
    };
    storage_security::verify_open_file(&file)
        .map_err(|_| EnvelopeStoreError::StorageUnavailable)?;
    let size = usize::try_from(
        file.metadata()
            .map_err(|_| EnvelopeStoreError::StorageUnavailable)?
            .len(),
    )
    .map_err(|_| EnvelopeStoreError::StoreFull)?;
    if size == 0 || size > maximum {
        return Err(EnvelopeStoreError::StoreFull);
    }
    let mut bytes = Vec::with_capacity(size);
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| EnvelopeStoreError::StorageUnavailable)?;
    if bytes.len() > maximum {
        return Err(EnvelopeStoreError::StoreFull);
    }
    Ok(Some(bytes))
}
