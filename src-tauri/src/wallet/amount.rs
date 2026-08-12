use std::fmt;

pub(in crate::wallet) const VISION_TOKEN_DECIMALS: u32 = 9;
pub(in crate::wallet) const RAW_UNITS_PER_VISION: u128 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::wallet) enum WalletAmountError {
    InvalidFormat,
    TooManyDecimalPlaces,
    Overflow,
}

impl fmt::Display for WalletAmountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidFormat => "amount must be a plain non-negative decimal number",
            Self::TooManyDecimalPlaces => "amount has more than 9 decimal places",
            Self::Overflow => "amount exceeds the supported range",
        })
    }
}

impl std::error::Error for WalletAmountError {}

/// Converts a user-facing Vision amount to exact raw units without floats.
pub(in crate::wallet) fn parse_vision_amount(value: &str) -> Result<u128, WalletAmountError> {
    if value.is_empty() || value.trim() != value {
        return Err(WalletAmountError::InvalidFormat);
    }

    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fractional = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(WalletAmountError::InvalidFormat);
    }

    let fractional = fractional.unwrap_or("");
    if value.contains('.') && fractional.is_empty() {
        return Err(WalletAmountError::InvalidFormat);
    }
    if !fractional.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(WalletAmountError::InvalidFormat);
    }
    if fractional.len() > VISION_TOKEN_DECIMALS as usize {
        return Err(WalletAmountError::TooManyDecimalPlaces);
    }

    let whole = whole
        .parse::<u128>()
        .map_err(|_| WalletAmountError::Overflow)?;
    let fractional = if fractional.is_empty() {
        0
    } else {
        let parsed = fractional
            .parse::<u128>()
            .map_err(|_| WalletAmountError::Overflow)?;
        parsed
            .checked_mul(10_u128.pow(VISION_TOKEN_DECIMALS - fractional.len() as u32))
            .ok_or(WalletAmountError::Overflow)?
    };

    whole
        .checked_mul(RAW_UNITS_PER_VISION)
        .and_then(|raw| raw.checked_add(fractional))
        .ok_or(WalletAmountError::Overflow)
}

/// Formats exact raw units using the RC2 9-decimal denomination.
pub(in crate::wallet) fn format_vision_amount(raw_units: u128) -> String {
    let whole = raw_units / RAW_UNITS_PER_VISION;
    let remainder = raw_units % RAW_UNITS_PER_VISION;
    if remainder == 0 {
        return whole.to_string();
    }

    let fractional = format!("{remainder:09}");
    format!("{whole}.{}", fractional.trim_end_matches('0'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_amount_vectors_match_nine_decimal_raw_units() {
        let vectors = [
            ("0", 0),
            ("0.000000001", 1),
            ("1", 1_000_000_000),
            ("1.23", 1_230_000_000),
            ("510", 510_000_000_000),
            ("42.000000042", 42_000_000_042),
        ];

        for (display, raw) in vectors {
            assert_eq!(parse_vision_amount(display), Ok(raw));
            assert_eq!(parse_vision_amount(&format_vision_amount(raw)), Ok(raw));
        }
    }

    #[test]
    fn formatting_never_uses_floating_point_or_scientific_notation() {
        assert_eq!(format_vision_amount(1), "0.000000001");
        assert_eq!(format_vision_amount(1_230_000_000), "1.23");
        assert_eq!(
            format_vision_amount(u128::MAX),
            "340282366920938463463374607431.768211455"
        );
    }

    #[test]
    fn parser_rejects_ambiguous_or_unsupported_amounts() {
        for invalid in [
            "", " 1", "1 ", ".1", "1.", "1.2.3", "-1", "+1", "1e9", "1,000", "one",
        ] {
            assert_eq!(
                parse_vision_amount(invalid),
                Err(WalletAmountError::InvalidFormat)
            );
        }
        assert_eq!(
            parse_vision_amount("0.0000000001"),
            Err(WalletAmountError::TooManyDecimalPlaces)
        );
        assert_eq!(
            parse_vision_amount("340282366920938463463374607432"),
            Err(WalletAmountError::Overflow)
        );
    }
}
