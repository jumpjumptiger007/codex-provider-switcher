use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderTransport {
    Native,
    Responses,
    Bridge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityStatus {
    Verified,
    VerifiedBasic,
    Degraded,
    Unverified,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderModelTarget {
    pub provider: String,
    pub model: String,
}

impl FromStr for ProviderModelTarget {
    type Err = TargetParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((provider, model)) = value.split_once('/') else {
            return Err(TargetParseError::MissingSeparator);
        };

        if provider.is_empty() || model.is_empty() {
            return Err(TargetParseError::InvalidParts);
        }

        Ok(Self {
            provider: provider.to_owned(),
            model: model.to_owned(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetParseError {
    MissingSeparator,
    InvalidParts,
}

impl fmt::Display for TargetParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSeparator => formatter.write_str("target must use provider/model format"),
            Self::InvalidParts => {
                formatter.write_str("target must include one non-empty provider and model")
            }
        }
    }
}

impl std::error::Error for TargetParseError {}
