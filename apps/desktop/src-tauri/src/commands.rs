use std::path::Path;

use cps::{
    application::{self, ApplicationError},
    config::ConfigError,
    domain::ProviderTransport,
    provider::ProviderRegistry,
};

use crate::dto::{DesktopError, ProviderDto, StatusDto};

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
    let provider = status.selection.model_provider;
    let model = status.selection.model;
    let known_provider = status.known_provider;

    Ok(StatusDto {
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
    })
}

fn map_status_error(error: ApplicationError) -> DesktopError {
    match error {
        ApplicationError::Config(ConfigError::Missing { .. }) => status_error("config_missing"),
        ApplicationError::Config(
            ConfigError::MissingActiveModel
            | ConfigError::InvalidActiveModel
            | ConfigError::EmptyActiveModel
            | ConfigError::MissingActiveProvider
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
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use serde_json::json;

    use super::{get_providers, get_status_at};

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

    #[test]
    fn maps_known_provider_and_transport() {
        let fixture = ConfigFixture::new(&config("openrouter", "anthropic/claude-sonnet-4"));

        let status = get_status_at(&fixture.config_path).unwrap();

        assert_eq!(status.provider, "openrouter");
        assert_eq!(status.transport.as_deref(), Some("responses"));
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
}
