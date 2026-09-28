use security_framework::{
    base::Error as SecurityFrameworkError,
    passwords::{PasswordOptions, delete_generic_password, generic_password, set_generic_password},
};

use crate::{credential::CPS_KEYCHAIN_SERVICE, provider::CredentialSlotId};

use super::{CredentialOperation, CredentialStore, CredentialStoreError, SecretValue};

// OSStatus for Apple's errSecItemNotFound.
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25_300;

#[derive(Debug, Default, Clone, Copy)]
pub struct MacOsKeychainStore;

impl MacOsKeychainStore {
    pub const fn new() -> Self {
        Self
    }
}

impl CredentialStore for MacOsKeychainStore {
    fn set(
        &self,
        slot: &CredentialSlotId,
        value: &SecretValue,
    ) -> Result<(), CredentialStoreError> {
        let (service, account) = keychain_identity(slot);
        set_generic_password(service, account, value.expose_secret().as_bytes()).map_err(|_| {
            CredentialStoreError::Backend {
                operation: CredentialOperation::Store,
            }
        })
    }

    fn get(&self, slot: &CredentialSlotId) -> Result<Option<SecretValue>, CredentialStoreError> {
        let (service, account) = keychain_identity(slot);
        match generic_password(PasswordOptions::new_generic_password(service, account)) {
            Ok(bytes) => {
                let value = String::from_utf8(bytes)
                    .map_err(|_| CredentialStoreError::InvalidStoredValue)?;
                let secret = SecretValue::new(value)
                    .map_err(|_| CredentialStoreError::InvalidStoredValue)?;
                Ok(Some(secret))
            }
            Err(error) if is_item_not_found(&error) => Ok(None),
            Err(_) => Err(CredentialStoreError::Backend {
                operation: CredentialOperation::Retrieve,
            }),
        }
    }

    fn delete(&self, slot: &CredentialSlotId) -> Result<(), CredentialStoreError> {
        let (service, account) = keychain_identity(slot);
        match delete_generic_password(service, account) {
            Ok(()) => Ok(()),
            Err(error) if is_item_not_found(&error) => Ok(()),
            Err(_) => Err(CredentialStoreError::Backend {
                operation: CredentialOperation::Delete,
            }),
        }
    }
}

fn keychain_identity(slot: &CredentialSlotId) -> (&'static str, &str) {
    (CPS_KEYCHAIN_SERVICE, slot.as_str())
}

fn is_item_not_found(error: &SecurityFrameworkError) -> bool {
    error.code() == ERR_SEC_ITEM_NOT_FOUND
}

#[cfg(test)]
mod tests {
    use super::{
        ERR_SEC_ITEM_NOT_FOUND, SecurityFrameworkError, is_item_not_found, keychain_identity,
    };
    use crate::credential::CPS_KEYCHAIN_SERVICE;
    use crate::provider::CredentialSlotId;

    #[test]
    fn slot_mapping_uses_only_the_cps_service_and_requested_account() {
        let slot = CredentialSlotId::new("deepseek").unwrap();

        assert_eq!(keychain_identity(&slot), (CPS_KEYCHAIN_SERVICE, "deepseek"));
    }

    #[test]
    fn only_item_not_found_is_classified_as_missing() {
        assert!(is_item_not_found(&SecurityFrameworkError::from_code(
            ERR_SEC_ITEM_NOT_FOUND
        )));
        assert!(!is_item_not_found(&SecurityFrameworkError::from_code(
            -25_299
        )));
    }
}
