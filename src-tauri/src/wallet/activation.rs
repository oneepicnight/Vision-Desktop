use super::contract::{wallet_contract_gate, WalletContractRequirement};

const INDEPENDENT_SECURITY_REVIEW_APPROVED: bool = false;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletActivationRequirement {
    CompatibilityApproval,
    Compatibility(WalletContractRequirement),
    IndependentSecurityReview,
}

pub(super) struct WalletActivationPolicy {
    unmet_requirements: Vec<WalletActivationRequirement>,
}

impl WalletActivationPolicy {
    pub(super) fn production() -> Self {
        let gate = wallet_contract_gate();
        let mut unmet_requirements = gate
            .unmet_requirements
            .into_iter()
            .map(WalletActivationRequirement::Compatibility)
            .collect::<Vec<_>>();
        if !gate.signing_enabled {
            unmet_requirements.push(WalletActivationRequirement::CompatibilityApproval);
        }
        if !INDEPENDENT_SECURITY_REVIEW_APPROVED {
            unmet_requirements.push(WalletActivationRequirement::IndependentSecurityReview);
        }
        Self { unmet_requirements }
    }

    pub(super) fn is_satisfied(&self) -> bool {
        self.unmet_requirements.is_empty()
    }

    #[cfg(test)]
    pub(super) fn satisfied_for_test() -> Self {
        Self {
            unmet_requirements: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn missing_for_test(requirement: WalletActivationRequirement) -> Self {
        Self {
            unmet_requirements: vec![requirement],
        }
    }
}

#[cfg(test)]
pub(in crate::wallet) fn all_activation_requirements_for_test() -> Vec<WalletActivationRequirement>
{
    use WalletContractRequirement::{
        AddressEncoding, AmountDenomination, FeeAndNonceRules, KeyDerivation,
        PrivateLoopbackBinding, ReceiptAndHistory, SignatureVector, SubmissionResponse,
        TransactionSerialization,
    };

    std::iter::once(WalletActivationRequirement::CompatibilityApproval)
        .chain(
            [
                KeyDerivation,
                AddressEncoding,
                AmountDenomination,
                TransactionSerialization,
                SignatureVector,
                FeeAndNonceRules,
                SubmissionResponse,
                ReceiptAndHistory,
                PrivateLoopbackBinding,
            ]
            .into_iter()
            .map(WalletActivationRequirement::Compatibility),
        )
        .chain(std::iter::once(
            WalletActivationRequirement::IndependentSecurityReview,
        ))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_policy_is_blocked_by_runtime_and_review_requirements() {
        let policy = WalletActivationPolicy::production();

        assert!(!policy.is_satisfied());
        assert!(policy
            .unmet_requirements
            .contains(&WalletActivationRequirement::CompatibilityApproval));
        assert!(policy
            .unmet_requirements
            .contains(&WalletActivationRequirement::Compatibility(
                WalletContractRequirement::PrivateLoopbackBinding,
            ),));
        assert!(policy
            .unmet_requirements
            .contains(&WalletActivationRequirement::IndependentSecurityReview));
    }

    #[test]
    fn test_policy_can_isolate_every_individual_requirement() {
        for requirement in all_activation_requirements_for_test() {
            assert!(!WalletActivationPolicy::missing_for_test(requirement).is_satisfied());
        }
        assert!(WalletActivationPolicy::satisfied_for_test().is_satisfied());
    }
}
