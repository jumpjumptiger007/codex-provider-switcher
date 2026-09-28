use std::{error::Error, fmt};

use crate::provider::CredentialSlotId;

pub const CPS_KEYCHAIN_SERVICE: &str = "codex-provider-switcher";

/// A secret held only in process memory and redacted from diagnostic output.
#[derive(PartialEq, Eq)]
pub struct SecretValue(String);

impl SecretValue {
    pub fn new(value: impl Into<String>) -> Result<Self, SecretValueError> {
        let value = value.into();
        if value.is_empty() {
            return Err(SecretValueError::Empty);
        }
        Ok(Self(value))
    }

    /// Explicitly access the secret for a credential backend or future API use.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretValueError {
    Empty,
}

impl fmt::Display for SecretValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("secret value must not be empty")
    }
}

impl Error for SecretValueError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialOperation {
    Store,
    Retrieve,
    Delete,
}

impl fmt::Display for CredentialOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store => formatter.write_str("store"),
            Self::Retrieve => formatter.write_str("retrieve"),
            Self::Delete => formatter.write_str("delete"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialStoreError {
    Backend { operation: CredentialOperation },
    InvalidStoredValue,
}

impl fmt::Display for CredentialStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend { operation } => {
                write!(formatter, "credential backend failed during {operation}")
            }
            Self::InvalidStoredValue => {
                formatter.write_str("stored credential is not a valid non-empty UTF-8 value")
            }
        }
    }
}

impl Error for CredentialStoreError {}

/// Backend-neutral operations keyed by a provider credential slot.
pub trait CredentialStore {
    fn set(&self, slot: &CredentialSlotId, value: &SecretValue)
    -> Result<(), CredentialStoreError>;

    fn get(&self, slot: &CredentialSlotId) -> Result<Option<SecretValue>, CredentialStoreError>;

    /// Delete is idempotent: deleting a missing slot succeeds.
    fn delete(&self, slot: &CredentialSlotId) -> Result<(), CredentialStoreError>;
}

#[cfg(target_os = "macos")]
pub mod macos;
