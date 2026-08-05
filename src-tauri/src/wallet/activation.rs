use super::contract::{wallet_contract_gate, WalletContractRequirement};

const INDEPENDENT_LIFECYCLE_SECURITY_REVIEW_APPROVED: bool = false;
const INDEPENDENT_SIGNING_SECURITY_REVIEW_APPROVED: bool = false;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletActivationScope {
    Lifecycle,
    Signing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletActivationRequirement {
    CompatibilityApproval,
    Compatibility(WalletActivationScope, WalletContractRequirement),
    IndependentSecurityReview(WalletActivationScope),
}

pub(super) struct WalletActivationPolicy {
    lifecycle_unmet_requirements: Vec<WalletActivationRequirement>,
    signing_unmet_requirements: Vec<WalletActivationRequirement>,
}

impl WalletActivationPolicy {
    pub(super) fn production() -> Self {
        let gate = wallet_contract_gate();
        let mut lifecycle_unmet_requirements = gate
            .unmet_requirements
            .iter()
            .copied()
            .filter(is_lifecycle_contract_requirement)
            .map(|requirement| {
                WalletActivationRequirement::Compatibility(
                    WalletActivationScope::Lifecycle,
                    requirement,
                )
            })
            .collect::<Vec<_>>();
        if !INDEPENDENT_LIFECYCLE_SECURITY_REVIEW_APPROVED {
            lifecycle_unmet_requirements.push(
                WalletActivationRequirement::IndependentSecurityReview(
                    WalletActivationScope::Lifecycle,
                ),
            );
        }

        let mut signing_unmet_requirements = gate
            .unmet_requirements
            .into_iter()
            .map(|requirement| {
                WalletActivationRequirement::Compatibility(
                    WalletActivationScope::Signing,
                    requirement,
                )
            })
            .collect::<Vec<_>>();
        if !gate.signing_enabled {
            signing_unmet_requirements.push(WalletActivationRequirement::CompatibilityApproval);
        }
        if !INDEPENDENT_SIGNING_SECURITY_REVIEW_APPROVED {
            signing_unmet_requirements.push(
                WalletActivationRequirement::IndependentSecurityReview(
                    WalletActivationScope::Signing,
                ),
            );
        }

        Self {
            lifecycle_unmet_requirements,
            signing_unmet_requirements,
        }
    }

    pub(super) fn is_satisfied(&self, scope: WalletActivationScope) -> bool {
        self.lifecycle_unmet_requirements.is_empty()
            && (scope == WalletActivationScope::Lifecycle
                || self.signing_unmet_requirements.is_empty())
    }

    #[cfg(test)]
    pub(super) fn satisfied_for_test() -> Self {
        Self {
            lifecycle_unmet_requirements: Vec::new(),
            signing_unmet_requirements: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn missing_for_test(requirement: WalletActivationRequirement) -> Self {
        let mut policy = Self::satisfied_for_test();
        match requirement_scope(requirement) {
            WalletActivationScope::Lifecycle => {
                policy.lifecycle_unmet_requirements.push(requirement);
            }
            WalletActivationScope::Signing => {
                policy.signing_unmet_requirements.push(requirement);
            }
        }
        policy
    }
}

const fn is_lifecycle_contract_requirement(requirement: &WalletContractRequirement) -> bool {
    matches!(
        requirement,
        WalletContractRequirement::KeyDerivation | WalletContractRequirement::AddressEncoding
    )
}

#[cfg(test)]
const fn requirement_scope(requirement: WalletActivationRequirement) -> WalletActivationScope {
    match requirement {
        WalletActivationRequirement::CompatibilityApproval => WalletActivationScope::Signing,
        WalletActivationRequirement::Compatibility(scope, _)
        | WalletActivationRequirement::IndependentSecurityReview(scope) => scope,
    }
}

#[cfg(test)]
pub(in crate::wallet) fn lifecycle_activation_requirements_for_test(
) -> Vec<WalletActivationRequirement> {
    use WalletContractRequirement::{AddressEncoding, KeyDerivation};

    [KeyDerivation, AddressEncoding]
        .into_iter()
        .map(|requirement| {
            WalletActivationRequirement::Compatibility(
                WalletActivationScope::Lifecycle,
                requirement,
            )
        })
        .chain(std::iter::once(
            WalletActivationRequirement::IndependentSecurityReview(
                WalletActivationScope::Lifecycle,
            ),
        ))
        .collect()
}

#[cfg(test)]
pub(in crate::wallet) fn signing_activation_requirements_for_test(
) -> Vec<WalletActivationRequirement> {
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
            .map(|requirement| {
                WalletActivationRequirement::Compatibility(
                    WalletActivationScope::Signing,
                    requirement,
                )
            }),
        )
        .chain(std::iter::once(
            WalletActivationRequirement::IndependentSecurityReview(WalletActivationScope::Signing),
        ))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_policy_keeps_lifecycle_and_signing_independently_closed() {
        let policy = WalletActivationPolicy::production();

        assert!(!policy.is_satisfied(WalletActivationScope::Lifecycle));
        assert!(!policy.is_satisfied(WalletActivationScope::Signing));
        assert_eq!(
            policy.lifecycle_unmet_requirements,
            vec![WalletActivationRequirement::IndependentSecurityReview(
                WalletActivationScope::Lifecycle,
            )]
        );
        assert!(policy
            .signing_unmet_requirements
            .contains(&WalletActivationRequirement::CompatibilityApproval));
        assert!(policy.signing_unmet_requirements.contains(
            &WalletActivationRequirement::Compatibility(
                WalletActivationScope::Signing,
                WalletContractRequirement::PrivateLoopbackBinding,
            ),
        ));
        assert!(
            policy.signing_unmet_requirements.contains(
                &WalletActivationRequirement::IndependentSecurityReview(
                    WalletActivationScope::Signing,
                ),
            )
        );
    }

    #[test]
    fn signing_requirements_do_not_block_lifecycle_authority() {
        for requirement in signing_activation_requirements_for_test() {
            let policy = WalletActivationPolicy::missing_for_test(requirement);
            assert!(policy.is_satisfied(WalletActivationScope::Lifecycle));
            assert!(!policy.is_satisfied(WalletActivationScope::Signing));
        }
    }

    #[test]
    fn lifecycle_requirements_also_block_signing_authority() {
        for requirement in lifecycle_activation_requirements_for_test() {
            let policy = WalletActivationPolicy::missing_for_test(requirement);
            assert!(!policy.is_satisfied(WalletActivationScope::Lifecycle));
            assert!(!policy.is_satisfied(WalletActivationScope::Signing));
        }
    }

    #[test]
    fn satisfied_test_policy_allows_both_scopes() {
        let policy = WalletActivationPolicy::satisfied_for_test();
        assert!(policy.is_satisfied(WalletActivationScope::Lifecycle));
        assert!(policy.is_satisfied(WalletActivationScope::Signing));
    }
}
