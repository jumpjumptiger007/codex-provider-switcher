pub(crate) mod safety;

use std::path::Path;

use cps::{
    application::{self, ApplicationError},
    config::ConfigError,
    credential::CredentialStore,
    domain::{ProviderModelTarget, ProviderTransport},
    model_discovery::DiscoveredModel,
    provider::{ProviderRegistry, RegistryError},
};

use crate::dto::{CredentialStatusDto, DesktopError, ModelDto, ProviderDto, StatusDto};

/// Returns the static, offline provider inventory from the CPS core registry.
#[tauri::command]
pub fn get_providers() -> Vec<ProviderDto> {
    ProviderRegistry::initial()
        .iter()
        .map(ProviderDto::from)
        .collect()
}

/// Returns the active Codex provider and model without changing local state.
#[tauri::command]
pub fn get_status() -> Result<StatusDto, DesktopError> {
    let config_path =
        application::resolve_user_config_path().map_err(|_| status_error("status_unavailable"))?;
    get_status_at(&config_path)
}

fn get_status_at(config_path: &Path) -> Result<StatusDto, DesktopError> {
    let registry = ProviderRegistry::initial();
    let status = application::status(&registry, config_path).map_err(map_status_error)?;
    Ok(status_dto(status))
}

fn status_dto(status: application::Status) -> StatusDto {
    let provider = status.selection.model_provider;
    let model = status.selection.model;
    let known_provider = status.known_provider;

    StatusDto {
        target: format!("{provider}/{model}"),
        provider,
        model,
        known_provider: known_provider.is_some(),
        transport: known_provider.map(|transport| {
            match transport {
                ProviderTransport::Native => "native",
                ProviderTransport::Responses => "responses",
                ProviderTransport::Bridge => "bridge",
            }
            .to_owned()
        }),
    }
}

/// Inspect only whether a CPS-owned credential is present for the selected provider.
#[tauri::command]
pub async fn get_credential_status(
    provider_id: String,
) -> Result<CredentialStatusDto, DesktopError> {
    tauri::async_runtime::spawn_blocking(move || {
        with_platform_store(|store| {
            credential_status_with_store(&ProviderRegistry::initial(), store, &provider_id)
        })
    })
    .await
    .map_err(|_| internal_error())?
}

/// Store a credential using the CPS application API. The value is never returned or logged.
#[tauri::command]
pub async fn save_credential(
    provider_id: String,
    credential: String,
) -> Result<CredentialStatusDto, DesktopError> {
    tauri::async_runtime::spawn_blocking(move || {
        with_platform_store(|store| {
            save_credential_with_store(
                &ProviderRegistry::initial(),
                store,
                &provider_id,
                &credential,
            )
        })
    })
    .await
    .map_err(|_| internal_error())?
}

/// Discover models for one selected direct API provider through the CPS core.
#[tauri::command]
pub async fn get_models(provider_id: String) -> Result<Vec<ModelDto>, DesktopError> {
    tauri::async_runtime::spawn_blocking(move || {
        with_platform_store(|store| {
            get_models_with_store(&ProviderRegistry::initial(), store, &provider_id)
        })
    })
    .await
    .map_err(|_| internal_error())?
}

/// Switch the active Codex model through the CPS config transaction and return fresh status.
#[tauri::command]
pub async fn switch_model(
    provider_id: String,
    model_id: String,
) -> Result<StatusDto, DesktopError> {
    tauri::async_runtime::spawn_blocking(move || {
        with_platform_store(|store| {
            let config_path =
                application::resolve_user_config_path().map_err(|_| error("config_missing"))?;
            switch_model_at(
                &ProviderRegistry::initial(),
                store,
                &config_path,
                &provider_id,
                &model_id,
            )
        })
    })
    .await
    .map_err(|_| internal_error())?
}

fn with_platform_store<T>(operation: impl FnOnce(Option<&dyn CredentialStore>) -> T) -> T {
    #[cfg(target_os = "macos")]
    {
        let store = cps::credential::macos::MacOsKeychainStore::new();
        operation(Some(&store))
    }
    #[cfg(not(target_os = "macos"))]
    {
        operation(None)
    }
}

fn credential_status_with_store(
    registry: &ProviderRegistry,
    store: Option<&dyn CredentialStore>,
    provider_id: &str,
) -> Result<CredentialStatusDto, DesktopError> {
    let provider = registry.lookup(provider_id).map_err(map_registry_error)?;
    if provider.transport == ProviderTransport::Native {
        return Ok(CredentialStatusDto {
            provider_id: provider.id.as_str().to_owned(),
            status: "not_applicable".to_owned(),
        });
    }
    let direct = provider
        .direct_responses
        .as_ref()
        .ok_or_else(internal_error)?;
    let store = store.ok_or_else(|| error("unsupported_platform"))?;
    let status = match store.get(&direct.credential_slot) {
        Ok(Some(_)) => "present",
        Ok(None) => "missing",
        Err(_) => return Err(error("credential_backend_error")),
    };
    Ok(CredentialStatusDto {
        provider_id: provider.id.as_str().to_owned(),
        status: status.to_owned(),
    })
}

fn save_credential_with_store(
    registry: &ProviderRegistry,
    store: Option<&dyn CredentialStore>,
    provider_id: &str,
    credential: &str,
) -> Result<CredentialStatusDto, DesktopError> {
    let provider = registry.lookup(provider_id).map_err(map_registry_error)?;
    if provider.transport == ProviderTransport::Native {
        return Err(error("credential_not_applicable"));
    }
    if credential.is_empty() {
        return Err(error("credential_empty"));
    }
    let store = store.ok_or_else(|| error("unsupported_platform"))?;
    application::store_provider_credential(registry, store, provider_id, credential)
        .map_err(map_credential_operation_error)?;
    Ok(CredentialStatusDto {
        provider_id: provider.id.as_str().to_owned(),
        status: "present".to_owned(),
    })
}

fn get_models_with_store(
    registry: &ProviderRegistry,
    store: Option<&dyn CredentialStore>,
    provider_id: &str,
) -> Result<Vec<ModelDto>, DesktopError> {
    let provider = registry.lookup(provider_id).map_err(map_registry_error)?;
    if provider.transport == ProviderTransport::Native {
        return Err(error("discovery_not_supported"));
    }
    let models = application::discover_provider_models(registry, store, provider_id)
        .map_err(map_model_discovery_error)?;
    Ok(models
        .into_iter()
        .map(|model| model_dto(provider.id.as_str(), model))
        .collect())
}

fn model_dto(provider_id: &str, model: DiscoveredModel) -> ModelDto {
    ModelDto {
        target: format!("{provider_id}/{}", model.id),
        id: model.id,
    }
}

fn switch_model_at(
    registry: &ProviderRegistry,
    store: Option<&dyn CredentialStore>,
    config_path: &Path,
    provider_id: &str,
    model_id: &str,
) -> Result<StatusDto, DesktopError> {
    let provider = registry.lookup(provider_id).map_err(map_registry_error)?;
    if model_id.trim().is_empty() {
        return Err(error("invalid_model"));
    }
    let target = ProviderModelTarget {
        provider: provider.id.as_str().to_owned(),
        model: model_id.to_owned(),
    };
    let store = if provider.transport == ProviderTransport::Native {
        None
    } else {
        store
    };
    application::use_provider_model(registry, store, config_path, &target)
        .map_err(map_switch_error)?;
    let status = application::status(registry, config_path).map_err(map_status_error)?;
    Ok(status_dto(status))
}

fn map_registry_error(registry_error: RegistryError) -> DesktopError {
    match registry_error {
        RegistryError::UnknownProvider(_) | RegistryError::InvalidProviderId(_) => {
            error("provider_unknown")
        }
        _ => internal_error(),
    }
}

fn map_credential_operation_error(application_error: ApplicationError) -> DesktopError {
    match application_error {
        ApplicationError::Registry(error) => map_registry_error(error),
        ApplicationError::EmptyCredential(_) => error("credential_empty"),
        ApplicationError::CredentialNotApplicable(_) => error("credential_not_applicable"),
        ApplicationError::UnsupportedPlatform => error("unsupported_platform"),
        ApplicationError::Credential(_) => error("credential_backend_error"),
        _ => internal_error(),
    }
}

fn map_model_discovery_error(application_error: ApplicationError) -> DesktopError {
    match application_error {
        ApplicationError::Registry(error) => map_registry_error(error),
        ApplicationError::MissingProviderCredential(_) => error("credential_missing"),
        ApplicationError::UnsupportedPlatform => error("unsupported_platform"),
        ApplicationError::Credential(_) => error("credential_backend_error"),
        ApplicationError::CodexManagedModelDiscovery(_) => error("discovery_not_supported"),
        ApplicationError::ModelDiscovery(_) => error("model_discovery_failed"),
        _ => internal_error(),
    }
}

fn map_switch_error(application_error: ApplicationError) -> DesktopError {
    match application_error {
        ApplicationError::Registry(RegistryError::UnknownProvider(_))
        | ApplicationError::Registry(RegistryError::InvalidProviderId(_)) => {
            error("provider_unknown")
        }
        ApplicationError::Config(config_error) => map_config_switch_error(config_error),
        ApplicationError::MissingProviderCredential(_) => error("credential_missing"),
        ApplicationError::Credential(_) => error("credential_backend_error"),
        ApplicationError::UnsupportedPlatform => error("unsupported_platform"),
        ApplicationError::CredentialNotApplicable(_) => error("credential_not_applicable"),
        _ => error("switch_failed"),
    }
}

fn map_config_switch_error(config_error: ConfigError) -> DesktopError {
    match config_error {
        ConfigError::Missing { .. } => error("config_missing"),
        ConfigError::Conflict { .. } => error("config_conflict"),
        ConfigError::ProviderDefinitionConflict { .. } => error("switch_failed"),
        ConfigError::InvalidConfigUtf8 { .. }
        | ConfigError::InvalidConfigToml { .. }
        | ConfigError::MissingActiveModel
        | ConfigError::InvalidActiveModel
        | ConfigError::EmptyActiveModel
        | ConfigError::MissingActiveProvider
        | ConfigError::InvalidActiveProvider
        | ConfigError::EmptyActiveProvider => error("config_invalid"),
        _ => error("switch_failed"),
    }
}

fn map_status_error(error: ApplicationError) -> DesktopError {
    match error {
        ApplicationError::Config(ConfigError::Missing { .. }) => status_error("config_missing"),
        ApplicationError::Config(
            ConfigError::MissingActiveModel
            | ConfigError::InvalidActiveModel
            | ConfigError::EmptyActiveModel
            | ConfigError::InvalidActiveProvider
            | ConfigError::EmptyActiveProvider
            | ConfigError::InvalidConfigUtf8 { .. }
            | ConfigError::InvalidConfigToml { .. },
        ) => status_error("config_invalid"),
        ApplicationError::Config(ConfigError::Read { .. }) | ApplicationError::ConfigPath(_) => {
            status_error("status_unavailable")
        }
        _ => status_error("internal_error"),
    }
}

fn status_error(code: &str) -> DesktopError {
    let message = match code {
        "config_missing" => {
            "Codex configuration was not found. The provider list is still available."
        }
        "config_invalid" => "Codex configuration is missing a valid active provider or model.",
        "status_unavailable" => {
            "Current setup could not be read. Check access to your Codex configuration."
        }
        _ => "Current setup is temporarily unavailable.",
    };
    DesktopError {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

fn error(code: &str) -> DesktopError {
    let message = match code {
        "provider_unknown" => "This provider is not supported by CPS.",
        "unsupported_platform" => "Credential storage is available only on macOS.",
        "credential_missing" => "Save an API key before loading models or switching.",
        "credential_not_applicable" => "This provider does not use a CPS API key.",
        "credential_backend_error" => {
            "Stored credentials could not be accessed. Check Keychain access and try again."
        }
        "credential_empty" => "Enter an API key before saving it.",
        "discovery_not_supported" => "Models are managed by Codex for this provider.",
        "model_discovery_failed" => "Models could not be loaded. Check the provider connection and try again.",
        "invalid_model" => "Enter a non-empty model ID.",
        "config_missing" => "Codex configuration was not found.",
        "config_invalid" => "Codex configuration is missing a valid active provider or model.",
        "config_conflict" => "Codex configuration changed outside CPS. CPS did not overwrite it. Refresh the current setup before trying again.",
        "doctor_unavailable" => "Local diagnostics could not access the Codex configuration location.",
        "no_recovery_backup" => "No CPS recovery backup was found.",
        "ambiguous_recovery_backup" => "The newest recovery backup is ambiguous. No configuration was restored.",
        "recovery_invalid" => "The newest recovery backup is not valid UTF-8 or TOML. No configuration was restored.",
        "restore_unavailable" => "The configuration or recovery backups could not be accessed. Check local file access and try again.",
        "restore_failed" => "Configuration could not be restored. Try again.",
        "switch_failed" => "The model switch failed. Current setup was not confirmed; refresh before retrying.",
        _ => "This operation could not be completed. Try again.",
    };
    DesktopError {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

fn internal_error() -> DesktopError {
    error("internal_error")
}

impl From<&cps::provider::ProviderSpec> for ProviderDto {
    fn from(provider: &cps::provider::ProviderSpec) -> Self {
        let transport = match provider.transport {
            ProviderTransport::Native => "native",
            ProviderTransport::Responses => "responses",
            ProviderTransport::Bridge => "bridge",
        };
        let compatibility = match provider.compatibility {
            cps::domain::CompatibilityStatus::Verified => "verified",
            cps::domain::CompatibilityStatus::VerifiedBasic => "verified-basic",
            cps::domain::CompatibilityStatus::Degraded => "degraded",
            cps::domain::CompatibilityStatus::Unverified => "unverified",
            cps::domain::CompatibilityStatus::Unsupported => "unsupported",
        };
        let discovery = match provider.model_discovery {
            cps::provider::ModelDiscoveryStrategy::CodexManaged => "codex-managed",
            cps::provider::ModelDiscoveryStrategy::ProviderModelsEndpoint(_) => "provider-models",
        };

        Self {
            id: provider.id.as_str().to_owned(),
            name: provider.display_name.clone(),
            transport: transport.to_owned(),
            compatibility: compatibility.to_owned(),
            discovery: discovery.to_owned(),
            requires_credential: provider.direct_responses.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        collections::HashMap,
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use cps::{
        application::ApplicationError,
        config::ConfigError,
        credential::{CredentialOperation, CredentialStore, CredentialStoreError, SecretValue},
        provider::CredentialSlotId,
    };
    use serde_json::json;

    use super::{
        credential_status_with_store, get_models_with_store, get_providers, get_status_at,
        map_model_discovery_error, map_switch_error, model_dto, save_credential_with_store,
        switch_model_at,
    };

    struct ConfigFixture {
        directory: PathBuf,
        config_path: PathBuf,
    }

    impl ConfigFixture {
        fn new(contents: &str) -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let directory = std::env::temp_dir().join(format!(
                "cps-desktop-status-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).expect("create isolated config directory");
            let config_path = directory.join("config.toml");
            fs::write(&config_path, contents).expect("write isolated config");
            Self {
                directory,
                config_path,
            }
        }
    }

    impl Drop for ConfigFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    fn config(provider: &str, model: &str) -> String {
        format!("model_provider = {provider:?}\nmodel = {model:?}\n")
    }

    #[derive(Default)]
    struct FakeCredentialStore {
        values: RefCell<HashMap<String, String>>,
        get_failure: bool,
        set_failure: bool,
    }

    impl FakeCredentialStore {
        fn with_credential(provider_id: &str, secret: &str) -> Self {
            let store = Self::default();
            store
                .values
                .borrow_mut()
                .insert(provider_id.to_owned(), secret.to_owned());
            store
        }

        fn contains(&self, provider_id: &str) -> bool {
            self.values.borrow().contains_key(provider_id)
        }
    }

    impl CredentialStore for FakeCredentialStore {
        fn set(
            &self,
            slot: &CredentialSlotId,
            value: &SecretValue,
        ) -> Result<(), CredentialStoreError> {
            if self.set_failure {
                return Err(CredentialStoreError::Backend {
                    operation: CredentialOperation::Store,
                });
            }
            self.values
                .borrow_mut()
                .insert(slot.as_str().to_owned(), value.expose_secret().to_owned());
            Ok(())
        }

        fn get(
            &self,
            slot: &CredentialSlotId,
        ) -> Result<Option<SecretValue>, CredentialStoreError> {
            if self.get_failure {
                return Err(CredentialStoreError::Backend {
                    operation: CredentialOperation::Retrieve,
                });
            }
            self.values
                .borrow()
                .get(slot.as_str())
                .map(|value| {
                    SecretValue::new(value.clone())
                        .map_err(|_| CredentialStoreError::InvalidStoredValue)
                })
                .transpose()
        }

        fn delete(&self, slot: &CredentialSlotId) -> Result<(), CredentialStoreError> {
            self.values.borrow_mut().remove(slot.as_str());
            Ok(())
        }
    }

    #[test]
    fn maps_known_provider_and_transport() {
        let fixture = ConfigFixture::new(&config("openrouter", "anthropic/claude-sonnet-4"));

        let status = get_status_at(&fixture.config_path).unwrap();

        assert_eq!(status.provider, "openrouter");
        assert_eq!(status.transport.as_deref(), Some("responses"));
        assert!(status.known_provider);
    }

    #[test]
    fn status_defaults_an_omitted_provider_to_openai() {
        let fixture = ConfigFixture::new("model = \"gpt-6-luna\"\n");

        let status = get_status_at(&fixture.config_path).unwrap();

        assert_eq!(status.provider, "openai");
        assert_eq!(status.model, "gpt-6-luna");
        assert_eq!(status.target, "openai/gpt-6-luna");
        assert!(status.known_provider);
    }

    #[test]
    fn preserves_provider_model_and_target_exactly() {
        let model = "vendor/codex-plus/2026-09";
        let fixture = ConfigFixture::new(&config("openai", model));

        let status = get_status_at(&fixture.config_path).unwrap();

        assert_eq!(status.provider, "openai");
        assert_eq!(status.model, model);
        assert_eq!(status.target, format!("openai/{model}"));
    }

    #[test]
    fn preserves_nested_model_ids_with_slashes() {
        let fixture = ConfigFixture::new(&config("deepseek", "deepseek/deepseek-v3.2/chat"));

        let status = get_status_at(&fixture.config_path).unwrap();

        assert_eq!(status.model, "deepseek/deepseek-v3.2/chat");
        assert_eq!(status.target, "deepseek/deepseek/deepseek-v3.2/chat");
    }

    #[test]
    fn unknown_configured_provider_is_valid_and_unmanaged() {
        let fixture = ConfigFixture::new(&config("custom-codex", "team/model-v2"));

        let status = get_status_at(&fixture.config_path).unwrap();

        assert_eq!(status.provider, "custom-codex");
        assert_eq!(status.model, "team/model-v2");
        assert_eq!(status.target, "custom-codex/team/model-v2");
        assert!(!status.known_provider);
        assert_eq!(status.transport, None);
    }

    #[test]
    fn status_errors_are_safe_and_do_not_expose_config_secrets_or_paths() {
        let fixture = ConfigFixture::new("api_key = \"super-secret-value\"\n");

        let error = get_status_at(&fixture.config_path).unwrap_err();
        let serialized = serde_json::to_value(error).unwrap();

        assert_eq!(
            serialized,
            json!({
                "code": "config_invalid",
                "message": "Codex configuration is missing a valid active provider or model."
            })
        );
        assert!(!serialized.to_string().contains("super-secret-value"));
        assert!(!serialized
            .to_string()
            .contains(fixture.directory.to_str().unwrap()));
        assert!(!serialized.to_string().contains("ConfigError"));
    }

    #[test]
    fn missing_config_has_a_stable_recoverable_error_code() {
        let fixture = ConfigFixture::new(&config("openai", "gpt-5"));
        fs::remove_file(&fixture.config_path).expect("remove isolated config");

        let error = get_status_at(&fixture.config_path).unwrap_err();

        assert_eq!(error.code, "config_missing");
        assert_eq!(
            error.message,
            "Codex configuration was not found. The provider list is still available."
        );
    }

    #[test]
    fn status_read_only_path_has_no_credential_or_network_dependencies() {
        let contents = config("openrouter", "openai/gpt-4.1");
        let fixture = ConfigFixture::new(&contents);

        let status = get_status_at(&fixture.config_path).unwrap();
        let after = fs::read_to_string(&fixture.config_path).unwrap();
        let serialized = serde_json::to_value(status).unwrap();

        assert_eq!(after, contents);
        assert_eq!(
            serialized,
            json!({
                "provider": "openrouter",
                "model": "openai/gpt-4.1",
                "target": "openrouter/openai/gpt-4.1",
                "knownProvider": true,
                "transport": "responses"
            })
        );
        assert!(!serialized.to_string().contains("credential"));
        assert!(!serialized.to_string().contains("https://"));
    }

    #[test]
    fn returns_the_six_registry_providers_in_stable_order() {
        let providers = get_providers();

        assert_eq!(providers.len(), 6);
        assert_eq!(
            providers
                .iter()
                .map(|provider| provider.id.as_str())
                .collect::<Vec<_>>(),
            [
                "deepseek",
                "lmstudio",
                "ollama",
                "openai",
                "openrouter",
                "xai"
            ]
        );
    }

    #[test]
    fn preserves_native_and_direct_provider_classifications() {
        let providers = get_providers();
        let find = |id| providers.iter().find(|provider| provider.id == id).unwrap();

        for id in ["lmstudio", "ollama", "openai"] {
            let provider = find(id);
            assert_eq!(provider.transport, "native");
            assert_eq!(provider.compatibility, "verified");
            assert_eq!(provider.discovery, "codex-managed");
            assert!(!provider.requires_credential);
        }

        let deepseek = find("deepseek");
        assert_eq!(deepseek.transport, "responses");
        assert_eq!(deepseek.compatibility, "verified-basic");
        assert_eq!(deepseek.discovery, "provider-models");
        assert!(deepseek.requires_credential);
    }

    #[test]
    fn dto_contains_no_credential_values_or_internal_connection_details() {
        let json = serde_json::to_value(get_providers()).unwrap();
        let providers = json.as_array().unwrap();

        for provider in providers {
            let object = provider.as_object().unwrap();
            assert_eq!(object.len(), 6);
            assert!(object.contains_key("id"));
            assert!(object.contains_key("name"));
            assert!(object.contains_key("transport"));
            assert!(object.contains_key("compatibility"));
            assert!(object.contains_key("discovery"));
            assert!(object.contains_key("requiresCredential"));
            assert!(!object.contains_key("credential"));
            assert!(!object.contains_key("baseUrl"));
        }
        assert!(!json.to_string().contains("https://"));
    }

    #[test]
    fn credential_status_distinguishes_present_missing_and_native() {
        let registry = cps::provider::ProviderRegistry::initial();
        let stored = FakeCredentialStore::with_credential("deepseek", "test-secret-value");
        let missing = FakeCredentialStore::default();

        assert_eq!(
            credential_status_with_store(&registry, Some(&stored), "deepseek")
                .unwrap()
                .status,
            "present"
        );
        assert_eq!(
            credential_status_with_store(&registry, Some(&missing), "xai")
                .unwrap()
                .status,
            "missing"
        );
        assert_eq!(
            credential_status_with_store(&registry, None, "openai")
                .unwrap()
                .status,
            "not_applicable"
        );
    }

    #[test]
    fn credential_status_returns_safe_unknown_and_backend_errors() {
        let registry = cps::provider::ProviderRegistry::initial();
        let failed_store = FakeCredentialStore {
            get_failure: true,
            ..FakeCredentialStore::default()
        };

        let unknown = credential_status_with_store(&registry, None, "unknown").unwrap_err();
        let backend =
            credential_status_with_store(&registry, Some(&failed_store), "deepseek").unwrap_err();
        assert_eq!(unknown.code, "provider_unknown");
        assert_eq!(backend.code, "credential_backend_error");
        assert!(!backend.message.contains("test-secret-value"));
    }

    #[test]
    fn credential_storage_uses_the_store_and_serializes_no_secret() {
        let registry = cps::provider::ProviderRegistry::initial();
        let store = FakeCredentialStore::default();

        let result =
            save_credential_with_store(&registry, Some(&store), "openrouter", "test-secret-value")
                .unwrap();
        let serialized = serde_json::to_value(result).unwrap();

        assert!(store.contains("openrouter"));
        assert_eq!(serialized["status"], "present");
        assert_eq!(serialized["providerId"], "openrouter");
        assert!(!serialized.to_string().contains("test-secret-value"));
    }

    #[test]
    fn empty_and_native_credentials_are_rejected_safely() {
        let registry = cps::provider::ProviderRegistry::initial();
        let store = FakeCredentialStore::default();

        let empty = save_credential_with_store(&registry, Some(&store), "xai", "").unwrap_err();
        let native =
            save_credential_with_store(&registry, Some(&store), "ollama", "test-secret-value")
                .unwrap_err();
        assert_eq!(empty.code, "credential_empty");
        assert_eq!(native.code, "credential_not_applicable");
        assert!(!native.message.contains("test-secret-value"));
    }

    #[test]
    fn model_discovery_rejects_native_and_missing_credentials_without_network() {
        let registry = cps::provider::ProviderRegistry::initial();
        let store = FakeCredentialStore::default();

        let native = get_models_with_store(&registry, None, "openai").unwrap_err();
        let missing = get_models_with_store(&registry, Some(&store), "deepseek").unwrap_err();
        assert_eq!(native.code, "discovery_not_supported");
        assert_eq!(missing.code, "credential_missing");
    }

    #[test]
    fn model_dto_preserves_nested_ids_and_exact_target_qualification() {
        let model = model_dto(
            "openrouter",
            cps::model_discovery::DiscoveredModel::new("anthropic/claude-sonnet-4"),
        );
        assert_eq!(model.id, "anthropic/claude-sonnet-4");
        assert_eq!(model.target, "openrouter/anthropic/claude-sonnet-4");
    }

    #[test]
    fn native_switch_updates_an_isolated_config_and_returns_refreshed_status() {
        let fixture = ConfigFixture::new(&config("openai", "gpt-4.1"));
        let registry = cps::provider::ProviderRegistry::initial();

        let status = switch_model_at(
            &registry,
            None,
            &fixture.config_path,
            "ollama",
            "llama3.2:latest",
        )
        .unwrap();

        assert_eq!(status.provider, "ollama");
        assert_eq!(status.model, "llama3.2:latest");
        assert_eq!(status.target, "ollama/llama3.2:latest");
        assert_eq!(status.transport.as_deref(), Some("native"));
    }

    #[test]
    fn direct_switch_keeps_nested_model_id_exact_with_fake_credentials() {
        let fixture = ConfigFixture::new(&config("openai", "gpt-4.1"));
        let registry = cps::provider::ProviderRegistry::initial();
        let store = FakeCredentialStore::with_credential("deepseek", "test-secret-value");

        let status = switch_model_at(
            &registry,
            Some(&store),
            &fixture.config_path,
            "deepseek",
            "deepseek/deepseek-v3.2/chat",
        )
        .unwrap();

        assert_eq!(status.provider, "deepseek");
        assert_eq!(status.model, "deepseek/deepseek-v3.2/chat");
        assert_eq!(status.target, "deepseek/deepseek/deepseek-v3.2/chat");
        assert_eq!(status.transport.as_deref(), Some("responses"));
    }

    #[test]
    fn failed_switch_does_not_report_success_and_conflicts_are_safe() {
        let fixture = ConfigFixture::new(&config("openai", "gpt-4.1"));
        let original = fs::read_to_string(&fixture.config_path).unwrap();
        let registry = cps::provider::ProviderRegistry::initial();
        let store = FakeCredentialStore::default();

        let failure = switch_model_at(
            &registry,
            Some(&store),
            &fixture.config_path,
            "deepseek",
            "deepseek-v3.2",
        )
        .unwrap_err();
        let conflict = map_switch_error(ApplicationError::Config(ConfigError::Conflict {
            path: fixture.directory.join("private-config-path"),
        }));

        assert_eq!(failure.code, "credential_missing");
        assert_eq!(fs::read_to_string(&fixture.config_path).unwrap(), original);
        assert_eq!(conflict.code, "config_conflict");
        assert!(conflict.message.contains("changed outside CPS"));
        assert!(!conflict
            .message
            .contains(fixture.directory.to_str().unwrap()));
    }

    #[test]
    fn model_discovery_errors_hide_provider_details_and_credentials() {
        let error = map_model_discovery_error(ApplicationError::ModelDiscovery(
            cps::model_discovery::ModelDiscoveryError::HttpStatus {
                provider_id: "deepseek".to_owned(),
                status: 401,
            },
        ));
        let serialized = serde_json::to_string(&error).unwrap();
        assert_eq!(error.code, "model_discovery_failed");
        assert!(!serialized.contains("test-secret-value"));
        assert!(!serialized.contains("Authorization"));
        assert!(!serialized.contains("https://"));
    }
}
