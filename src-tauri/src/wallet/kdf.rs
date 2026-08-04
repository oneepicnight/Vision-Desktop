use argon2::{Algorithm, Argon2, Block, Params, Version};
use zeroize::{Zeroize, Zeroizing};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletKdfError {
    InvalidParameters,
    AllocationUnavailable,
    DerivationFailed,
}

struct Argon2Workspace {
    blocks: Vec<Block>,
    #[cfg(test)]
    wipe_observer: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

impl Argon2Workspace {
    fn new(block_count: usize) -> Result<Self, WalletKdfError> {
        let mut blocks = Vec::new();
        blocks
            .try_reserve_exact(block_count)
            .map_err(|_| WalletKdfError::AllocationUnavailable)?;
        blocks.resize(block_count, Block::default());
        Ok(Self {
            blocks,
            #[cfg(test)]
            wipe_observer: None,
        })
    }

    #[cfg(test)]
    fn with_observer(
        block_count: usize,
        observer: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<Self, WalletKdfError> {
        let mut workspace = Self::new(block_count)?;
        workspace.wipe_observer = Some(observer);
        Ok(workspace)
    }
}

impl Drop for Argon2Workspace {
    fn drop(&mut self) {
        self.blocks.iter_mut().zeroize();
        #[cfg(test)]
        if let Some(observer) = &self.wipe_observer {
            use std::sync::atomic::Ordering;

            let cleared = self
                .blocks
                .iter()
                .all(|block| block.as_ref().iter().all(|word| *word == 0));
            observer.store(cleared, Ordering::Release);
        }
    }
}

pub(in crate::wallet) fn derive_argon2id_key(
    secret: &[u8],
    salt: &[u8],
    memory_kib: u32,
    iterations: u32,
    lanes: u32,
) -> Result<Zeroizing<[u8; 32]>, WalletKdfError> {
    let params = Params::new(memory_kib, iterations, lanes, Some(32))
        .map_err(|_| WalletKdfError::InvalidParameters)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params.clone());
    let mut workspace = Argon2Workspace::new(params.block_count())?;
    let mut output = Zeroizing::new([0_u8; 32]);
    argon2
        .hash_password_into_with_memory(secret, salt, output.as_mut(), &mut workspace.blocks)
        .map_err(|_| WalletKdfError::DerivationFailed)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        panic::{catch_unwind, AssertUnwindSafe},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
    };

    fn assert_zeroize<T: Zeroize>() {}

    #[test]
    fn argon2_block_zeroize_feature_is_enabled() {
        assert_zeroize::<Block>();
    }

    #[test]
    fn full_workspace_is_wiped_before_allocator_release() {
        let params = Params::new(65_536, 3, 1, Some(32)).unwrap();
        let observer = Arc::new(AtomicBool::new(false));
        {
            let mut workspace =
                Argon2Workspace::with_observer(params.block_count(), Arc::clone(&observer))
                    .unwrap();
            workspace.blocks[0].as_mut()[0] = u64::MAX;
            workspace.blocks.last_mut().unwrap().as_mut()[127] = u64::MAX;
        }
        assert!(observer.load(Ordering::Acquire));
    }

    #[test]
    fn workspace_is_wiped_after_derivation_error() {
        let observer = Arc::new(AtomicBool::new(false));
        {
            let params = Params::new(8, 1, 1, Some(32)).unwrap();
            let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params.clone());
            let mut workspace =
                Argon2Workspace::with_observer(params.block_count(), Arc::clone(&observer))
                    .unwrap();
            let mut invalid_output = [0_u8; 1];
            assert!(argon2
                .hash_password_into_with_memory(
                    b"test secret",
                    b"0123456789abcdef",
                    &mut invalid_output,
                    &mut workspace.blocks,
                )
                .is_err());
        }
        assert!(observer.load(Ordering::Acquire));
    }

    #[test]
    fn workspace_is_wiped_during_unwind() {
        let observer = Arc::new(AtomicBool::new(false));
        let result = catch_unwind(AssertUnwindSafe({
            let observer = Arc::clone(&observer);
            move || {
                let mut workspace = Argon2Workspace::with_observer(8, observer).unwrap();
                workspace.blocks[0].as_mut()[0] = u64::MAX;
                panic!("intentional workspace unwind test");
            }
        }));
        assert!(result.is_err());
        assert!(observer.load(Ordering::Acquire));
    }

    #[test]
    fn derivation_is_deterministic_with_caller_owned_memory() {
        let first = derive_argon2id_key(b"test secret", b"0123456789abcdef", 8, 1, 1).unwrap();
        let second = derive_argon2id_key(b"test secret", b"0123456789abcdef", 8, 1, 1).unwrap();
        assert_eq!(first.as_ref(), second.as_ref());
    }
}
