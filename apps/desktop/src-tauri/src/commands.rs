use cps::{domain::ProviderTransport, provider::ProviderRegistry};

use crate::dto::ProviderDto;

/// Returns the static, offline provider inventory from the CPS core registry.
#[tauri::command]
pub fn get_providers() -> Vec<ProviderDto> {
    ProviderRegistry::initial()
        .iter()
        .map(ProviderDto::from)
        .collect()
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
    use super::get_providers;

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
