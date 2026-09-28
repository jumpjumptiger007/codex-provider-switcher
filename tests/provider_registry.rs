use std::collections::BTreeSet;

use cps::{
    domain::{CompatibilityStatus, ProviderModelTarget, ProviderTransport},
    provider::{
        CapabilitySupport, CredentialSlotId, DirectResponsesSpec, ModelDiscoverySchema,
        ModelDiscoveryStrategy, ProviderCapabilities, ProviderId, ProviderIdError,
        ProviderModelsEndpoint, ProviderRegistry, ProviderSpec, RegistryError, WireProtocol,
    },
};

fn custom_responses_provider(id: &str) -> ProviderSpec {
    let credential_slot = CredentialSlotId::new(id).unwrap();
    let responses = DirectResponsesSpec::new("https://example.test/v1", credential_slot).unwrap();
    ProviderSpec::new(
        id,
        "Example Provider",
        ProviderTransport::Responses,
        CompatibilityStatus::Unverified,
        Some(responses),
        ProviderCapabilities::UNKNOWN,
        ModelDiscoveryStrategy::ProviderModelsEndpoint(ProviderModelsEndpoint {
            path: "/models",
            schema: ModelDiscoverySchema::DeepSeek,
        }),
    )
    .unwrap()
}

#[test]
fn initial_registry_contains_expected_providers_and_unique_ids() {
    let registry = ProviderRegistry::initial();
    let ids = registry
        .iter()
        .map(|provider| provider.id.as_str())
        .collect::<Vec<_>>();
    let unique_ids = ids.iter().copied().collect::<BTreeSet<_>>();

    assert_eq!(ids.len(), 6);
    assert_eq!(
        unique_ids,
        BTreeSet::from([
            "deepseek",
            "lmstudio",
            "ollama",
            "openai",
            "openrouter",
            "xai"
        ])
    );
    assert!(
        registry
            .iter()
            .all(|provider| provider.transport != ProviderTransport::Bridge)
    );
}

#[test]
fn provider_lookup_is_deterministic() {
    let registry = ProviderRegistry::initial();
    let first = registry.lookup("deepseek").unwrap();
    let second = registry.lookup("deepseek").unwrap();

    assert!(std::ptr::eq(first, second));
    assert_eq!(first.display_name, "DeepSeek");
    assert_eq!(first.transport, ProviderTransport::Responses);
}

#[test]
fn unknown_provider_lookup_and_resolution_return_typed_errors() {
    let registry = ProviderRegistry::initial();

    assert!(matches!(
        registry.lookup("missing"),
        Err(RegistryError::UnknownProvider(id)) if id.as_str() == "missing"
    ));
    assert!(matches!(
        registry.resolve_str("missing/model"),
        Err(RegistryError::UnknownProvider(id)) if id.as_str() == "missing"
    ));
}

#[test]
fn duplicate_registration_is_rejected() {
    let mut registry = ProviderRegistry::initial();
    let duplicate = registry.lookup("xai").unwrap().clone();

    assert!(matches!(
        registry.register(duplicate),
        Err(RegistryError::DuplicateProviderId(id)) if id.as_str() == "xai"
    ));
}

#[test]
fn provider_ids_reject_empty_slash_whitespace_and_unusable_characters() {
    assert!(matches!(ProviderId::new(""), Err(ProviderIdError::Empty)));
    for invalid in [" ", "\t", "bad/id", "bad id", "Upper", "-leading"] {
        assert!(
            ProviderId::new(invalid).is_err(),
            "accepted invalid ID {invalid:?}"
        );
        assert!(matches!(
            ProviderRegistry::initial().lookup(invalid),
            Err(RegistryError::InvalidProviderId(_))
        ));
    }
}

#[test]
fn custom_responses_providers_cannot_claim_reserved_native_ids() {
    let mut registry = ProviderRegistry::initial();

    for id in ["openai", "ollama", "lmstudio"] {
        assert!(matches!(
            registry.register(custom_responses_provider(id)),
            Err(RegistryError::ReservedNativeProviderId(reserved)) if reserved.as_str() == id
        ));
    }
}

#[test]
fn reserved_responses_providers_cannot_project_without_registry_registration() {
    for id in ["openai", "ollama", "lmstudio"] {
        let provider = custom_responses_provider(id);

        assert!(matches!(
            provider.project_custom_codex_provider(),
            Err(RegistryError::ReservedNativeProviderId(reserved)) if reserved.as_str() == id
        ));
    }
}

#[test]
fn direct_responses_providers_have_expected_base_urls_and_credential_slots() {
    let registry = ProviderRegistry::initial();
    let providers = [
        ("deepseek", "https://api.deepseek.com"),
        ("xai", "https://api.x.ai/v1"),
        ("openrouter", "https://openrouter.ai/api/v1"),
    ];

    for (id, base_url) in providers {
        let provider = registry.lookup(id).unwrap();
        let responses = provider.direct_responses.as_ref().unwrap();
        assert_eq!(responses.base_url, base_url);
        assert_eq!(responses.credential_slot.as_str(), id);
        assert_eq!(responses.wire_protocol, WireProtocol::Responses);
    }
}

#[test]
fn direct_responses_providers_project_without_credentials() {
    let registry = ProviderRegistry::initial();

    for id in ["deepseek", "xai", "openrouter"] {
        let provider = registry.lookup(id).unwrap();
        let projected = provider.project_custom_codex_provider().unwrap().unwrap();
        let responses = provider.direct_responses.as_ref().unwrap();

        assert_eq!(projected.provider_id.as_str(), id);
        assert_eq!(projected.display_name, provider.display_name);
        assert_eq!(projected.base_url, responses.base_url);
        assert_eq!(projected.wire_api.as_config_value(), "responses");
    }
}

#[test]
fn native_providers_are_not_projected_as_custom_provider_tables() {
    let registry = ProviderRegistry::initial();

    for id in ["openai", "ollama", "lmstudio"] {
        let provider = registry.lookup(id).unwrap();
        assert_eq!(provider.transport, ProviderTransport::Native);
        assert!(provider.direct_responses.is_none());
        assert_eq!(provider.project_custom_codex_provider().unwrap(), None);
    }
}

#[test]
fn resolves_nested_model_ids_after_the_first_slash() {
    let registry = ProviderRegistry::initial();
    let target = "openrouter/openai/gpt-5";
    let resolved = registry.resolve_str(target).unwrap();
    let parsed = target.parse::<ProviderModelTarget>().unwrap();

    assert_eq!(resolved.provider.id.as_str(), "openrouter");
    assert_eq!(resolved.model, "openai/gpt-5");
    assert_eq!(parsed.provider, "openrouter");
    assert_eq!(parsed.model, "openai/gpt-5");
}

#[test]
fn seeded_compatibility_capabilities_and_discovery_metadata_are_conservative() {
    let registry = ProviderRegistry::initial();
    let deepseek = registry.lookup("deepseek").unwrap();

    assert_eq!(deepseek.compatibility, CompatibilityStatus::VerifiedBasic);
    assert_eq!(
        deepseek.capabilities.streaming,
        CapabilitySupport::Supported
    );
    assert_eq!(
        deepseek.capabilities.function_tools,
        CapabilitySupport::Supported
    );
    assert_eq!(
        deepseek.capabilities.custom_tools,
        CapabilitySupport::Supported
    );
    assert_eq!(
        deepseek.capabilities.stateful_responses,
        CapabilitySupport::Unsupported
    );
    assert_eq!(
        deepseek.capabilities.reasoning,
        CapabilitySupport::Supported
    );

    for (id, schema) in [
        ("xai", ModelDiscoverySchema::Xai),
        ("openrouter", ModelDiscoverySchema::OpenRouter),
    ] {
        let provider = registry.lookup(id).unwrap();
        assert_eq!(provider.compatibility, CompatibilityStatus::Unverified);
        assert_eq!(
            provider.capabilities.streaming,
            CapabilitySupport::ModelDependent
        );
        assert_eq!(
            provider.model_discovery,
            ModelDiscoveryStrategy::ProviderModelsEndpoint(ProviderModelsEndpoint {
                path: "/models",
                schema,
            })
        );
    }
}
