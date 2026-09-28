use std::{collections::HashMap, sync::Mutex};

use cps::{
    credential::{
        CredentialOperation, CredentialStore, CredentialStoreError, SecretValue, SecretValueError,
    },
    provider::{CredentialSlotId, ProviderRegistry},
};

#[derive(Default)]
struct FakeCredentialStore {
    entries: Mutex<HashMap<CredentialSlotId, String>>,
    failing_operation: Mutex<Option<CredentialOperation>>,
}

impl FakeCredentialStore {
    fn fail_during(&self, operation: CredentialOperation) {
        *self.failing_operation.lock().unwrap() = Some(operation);
    }

    fn check_failure(&self, operation: CredentialOperation) -> Result<(), CredentialStoreError> {
        if *self.failing_operation.lock().unwrap() == Some(operation) {
            Err(CredentialStoreError::Backend { operation })
        } else {
            Ok(())
        }
    }
}

impl CredentialStore for FakeCredentialStore {
    fn set(
        &self,
        slot: &CredentialSlotId,
        value: &SecretValue,
    ) -> Result<(), CredentialStoreError> {
        self.check_failure(CredentialOperation::Store)?;
        self.entries
            .lock()
            .unwrap()
            .insert(slot.clone(), value.expose_secret().to_owned());
        Ok(())
    }

    fn get(&self, slot: &CredentialSlotId) -> Result<Option<SecretValue>, CredentialStoreError> {
        self.check_failure(CredentialOperation::Retrieve)?;
        self.entries
            .lock()
            .unwrap()
            .get(slot)
            .cloned()
            .map(|value| {
                SecretValue::new(value).map_err(|_| CredentialStoreError::InvalidStoredValue)
            })
            .transpose()
    }

    fn delete(&self, slot: &CredentialSlotId) -> Result<(), CredentialStoreError> {
        self.check_failure(CredentialOperation::Delete)?;
        self.entries.lock().unwrap().remove(slot);
        Ok(())
    }
}

fn slot(value: &str) -> CredentialSlotId {
    CredentialSlotId::new(value).unwrap()
}

fn secret(value: &str) -> SecretValue {
    SecretValue::new(value).unwrap()
}

#[test]
fn secret_values_reject_empty_input_and_redact_debug_output() {
    assert_eq!(SecretValue::new(""), Err(SecretValueError::Empty));

    let value = secret("sk-secret-marker");
    let debug = format!("{value:?}");

    assert!(!debug.contains("sk-secret-marker"));
    assert!(debug.contains("REDACTED"));
}

#[test]
fn secret_values_preserve_whitespace_exactly() {
    let value = secret("  value with spaces \n");

    assert_eq!(value.expose_secret(), "  value with spaces \n");
}

#[test]
fn store_retrieves_and_updates_a_credential_by_typed_slot() {
    let store = FakeCredentialStore::default();
    let slot = slot("deepseek");

    store.set(&slot, &secret("first-value")).unwrap();
    assert_eq!(
        store.get(&slot).unwrap().unwrap().expose_secret(),
        "first-value"
    );

    store.set(&slot, &secret("replacement-value")).unwrap();
    assert_eq!(
        store.get(&slot).unwrap().unwrap().expose_secret(),
        "replacement-value"
    );
}

#[test]
fn slots_are_isolated_and_deleting_one_preserves_the_other() {
    let store = FakeCredentialStore::default();
    let deepseek = slot("deepseek");
    let xai = slot("xai");
    store.set(&deepseek, &secret("deepseek-value")).unwrap();
    store.set(&xai, &secret("xai-value")).unwrap();

    store.delete(&deepseek).unwrap();

    assert_eq!(store.get(&deepseek).unwrap(), None);
    assert_eq!(
        store.get(&xai).unwrap().unwrap().expose_secret(),
        "xai-value"
    );
}

#[test]
fn missing_credentials_are_distinct_from_backend_failures() {
    let store = FakeCredentialStore::default();
    let slot = slot("openrouter");

    assert_eq!(store.get(&slot).unwrap(), None);

    store.fail_during(CredentialOperation::Retrieve);
    assert_eq!(
        store.get(&slot),
        Err(CredentialStoreError::Backend {
            operation: CredentialOperation::Retrieve
        })
    );
}

#[test]
fn deleting_a_missing_credential_is_idempotent() {
    let store = FakeCredentialStore::default();

    assert_eq!(store.delete(&slot("deepseek")), Ok(()));
}

#[test]
fn backend_errors_do_not_expose_secret_values() {
    let store = FakeCredentialStore::default();
    let value = secret("sk-keep-this-private");
    store.fail_during(CredentialOperation::Store);

    let error = store.set(&slot("deepseek"), &value).unwrap_err();
    let diagnostic = format!("{error:?} {error}");

    assert!(!diagnostic.contains("sk-keep-this-private"));
}

#[test]
fn seeded_responses_providers_have_slots_and_native_providers_do_not() {
    let registry = ProviderRegistry::initial();

    for id in ["deepseek", "xai", "openrouter"] {
        let provider = registry.lookup(id).unwrap();
        let direct = provider.direct_responses.as_ref().unwrap();
        assert_eq!(direct.credential_slot.as_str(), id);
    }

    for id in ["openai", "ollama", "lmstudio"] {
        let provider = registry.lookup(id).unwrap();
        assert!(provider.direct_responses.is_none());
    }
}
