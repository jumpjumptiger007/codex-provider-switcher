use std::{collections::BTreeMap, fmt};

use crate::domain::{
    CompatibilityStatus, ProviderModelTarget, ProviderTransport, TargetParseError,
};

const RESERVED_NATIVE_PROVIDER_IDS: &[&str] = &["openai", "ollama", "lmstudio"];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(value: impl Into<String>) -> Result<Self, ProviderIdError> {
        let value = value.into();
        if value.is_empty() {
            return Err(ProviderIdError::Empty);
        }
        if value.contains('/') {
            return Err(ProviderIdError::ContainsSlash);
        }
        if !value.as_bytes()[0].is_ascii_lowercase() && !value.as_bytes()[0].is_ascii_digit() {
            return Err(ProviderIdError::InvalidCharacters);
        }
        if !value.bytes().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, b'-' | b'_')
        }) {
            return Err(ProviderIdError::InvalidCharacters);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderIdError {
    Empty,
    ContainsSlash,
    InvalidCharacters,
}

impl fmt::Display for ProviderIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("provider ID must not be empty"),
            Self::ContainsSlash => formatter.write_str("provider ID must not contain '/'"),
            Self::InvalidCharacters => formatter.write_str(
                "provider ID must be a lowercase ASCII slug using letters, digits, '-' or '_'",
            ),
        }
    }
}

impl std::error::Error for ProviderIdError {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CredentialSlotId(String);

impl CredentialSlotId {
    pub fn new(value: impl Into<String>) -> Result<Self, ProviderIdError> {
        ProviderId::new(value).map(|id| Self(id.0))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilitySupport {
    Supported,
    Unsupported,
    ModelDependent,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub streaming: CapabilitySupport,
    pub function_tools: CapabilitySupport,
    pub custom_tools: CapabilitySupport,
    pub stateful_responses: CapabilitySupport,
    pub reasoning: CapabilitySupport,
}

impl ProviderCapabilities {
    pub const UNKNOWN: Self = Self {
        streaming: CapabilitySupport::Unknown,
        function_tools: CapabilitySupport::Unknown,
        custom_tools: CapabilitySupport::Unknown,
        stateful_responses: CapabilitySupport::Unknown,
        reasoning: CapabilitySupport::Unknown,
    };

    pub const fn model_dependent() -> Self {
        Self {
            streaming: CapabilitySupport::ModelDependent,
            function_tools: CapabilitySupport::ModelDependent,
            custom_tools: CapabilitySupport::ModelDependent,
            stateful_responses: CapabilitySupport::ModelDependent,
            reasoning: CapabilitySupport::ModelDependent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireProtocol {
    Responses,
}

impl WireProtocol {
    pub const fn as_config_value(self) -> &'static str {
        match self {
            Self::Responses => "responses",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectResponsesSpec {
    pub base_url: String,
    pub wire_protocol: WireProtocol,
    pub credential_slot: CredentialSlotId,
}

impl DirectResponsesSpec {
    pub fn new(
        base_url: impl Into<String>,
        credential_slot: CredentialSlotId,
    ) -> Result<Self, RegistryError> {
        let base_url = base_url.into();
        if base_url.trim().is_empty() {
            return Err(RegistryError::EmptyBaseUrl);
        }
        Ok(Self {
            base_url,
            wire_protocol: WireProtocol::Responses,
            credential_slot,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelDiscoveryStrategy {
    CodexManaged,
    ProviderModelsEndpoint { path: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSpec {
    pub id: ProviderId,
    pub display_name: String,
    pub transport: ProviderTransport,
    pub compatibility: CompatibilityStatus,
    pub direct_responses: Option<DirectResponsesSpec>,
    pub capabilities: ProviderCapabilities,
    pub model_discovery: ModelDiscoveryStrategy,
}

impl ProviderSpec {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        display_name: impl Into<String>,
        transport: ProviderTransport,
        compatibility: CompatibilityStatus,
        direct_responses: Option<DirectResponsesSpec>,
        capabilities: ProviderCapabilities,
        model_discovery: ModelDiscoveryStrategy,
    ) -> Result<Self, RegistryError> {
        let id = ProviderId::new(id).map_err(RegistryError::InvalidProviderId)?;
        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(RegistryError::EmptyDisplayName);
        }
        match (transport, direct_responses.is_some()) {
            (ProviderTransport::Responses, false) => {
                return Err(RegistryError::MissingDirectResponsesSpec(id));
            }
            (ProviderTransport::Responses, true) | (_, false) => {}
            (_, true) => {
                return Err(RegistryError::UnexpectedDirectResponsesSpec(id));
            }
        }

        Ok(Self {
            id,
            display_name,
            transport,
            compatibility,
            direct_responses,
            capabilities,
            model_discovery,
        })
    }

    pub fn project_custom_codex_provider(
        &self,
    ) -> Result<Option<CodexCustomProviderDefinition>, RegistryError> {
        let Some(response) = self.direct_responses.as_ref() else {
            return Ok(None);
        };
        if self.transport != ProviderTransport::Responses {
            return Ok(None);
        }
        if RESERVED_NATIVE_PROVIDER_IDS.contains(&self.id.as_str()) {
            return Err(RegistryError::ReservedNativeProviderId(self.id.clone()));
        }
        Ok(Some(CodexCustomProviderDefinition {
            provider_id: self.id.clone(),
            display_name: self.display_name.clone(),
            base_url: response.base_url.clone(),
            wire_api: response.wire_protocol,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexCustomProviderDefinition {
    pub provider_id: ProviderId,
    pub display_name: String,
    pub base_url: String,
    pub wire_api: WireProtocol,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProviderModel<'a> {
    pub provider: &'a ProviderSpec,
    pub model: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderRegistry {
    providers: BTreeMap<ProviderId, ProviderSpec>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn initial() -> Self {
        let mut registry = Self::new();
        for provider in initial_providers() {
            registry
                .register(provider)
                .expect("built-in provider definitions are valid and unique");
        }
        registry
    }

    pub fn register(&mut self, provider: ProviderSpec) -> Result<(), RegistryError> {
        if provider.transport == ProviderTransport::Responses
            && RESERVED_NATIVE_PROVIDER_IDS.contains(&provider.id.as_str())
        {
            return Err(RegistryError::ReservedNativeProviderId(provider.id));
        }
        if self.providers.contains_key(&provider.id) {
            return Err(RegistryError::DuplicateProviderId(provider.id));
        }
        self.providers.insert(provider.id.clone(), provider);
        Ok(())
    }

    pub fn lookup(&self, id: &str) -> Result<&ProviderSpec, RegistryError> {
        let id = ProviderId::new(id).map_err(RegistryError::InvalidProviderId)?;
        self.providers
            .get(&id)
            .ok_or(RegistryError::UnknownProvider(id))
    }

    pub fn resolve(
        &self,
        target: &ProviderModelTarget,
    ) -> Result<ResolvedProviderModel<'_>, RegistryError> {
        let provider = self.lookup(&target.provider)?;
        Ok(ResolvedProviderModel {
            provider,
            model: target.model.clone(),
        })
    }

    pub fn resolve_str(&self, target: &str) -> Result<ResolvedProviderModel<'_>, RegistryError> {
        let target = target
            .parse::<ProviderModelTarget>()
            .map_err(RegistryError::InvalidTarget)?;
        self.resolve(&target)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &ProviderSpec> {
        self.providers.values()
    }
}

fn initial_providers() -> [ProviderSpec; 6] {
    [
        native_provider("openai", "OpenAI"),
        native_provider("ollama", "Ollama"),
        native_provider("lmstudio", "LM Studio"),
        direct_responses_provider(
            "deepseek",
            "DeepSeek",
            "https://api.deepseek.com",
            CompatibilityStatus::VerifiedBasic,
            ProviderCapabilities {
                streaming: CapabilitySupport::Supported,
                function_tools: CapabilitySupport::Supported,
                custom_tools: CapabilitySupport::Supported,
                stateful_responses: CapabilitySupport::Unsupported,
                reasoning: CapabilitySupport::Supported,
            },
        ),
        direct_responses_provider(
            "xai",
            "xAI",
            "https://api.x.ai/v1",
            CompatibilityStatus::Unverified,
            ProviderCapabilities::model_dependent(),
        ),
        direct_responses_provider(
            "openrouter",
            "OpenRouter",
            "https://openrouter.ai/api/v1",
            CompatibilityStatus::Unverified,
            ProviderCapabilities::model_dependent(),
        ),
    ]
}

fn native_provider(id: &str, display_name: &str) -> ProviderSpec {
    ProviderSpec::new(
        id,
        display_name,
        ProviderTransport::Native,
        CompatibilityStatus::Verified,
        None,
        ProviderCapabilities::UNKNOWN,
        ModelDiscoveryStrategy::CodexManaged,
    )
    .expect("built-in native provider definition is valid")
}

fn direct_responses_provider(
    id: &str,
    display_name: &str,
    base_url: &str,
    compatibility: CompatibilityStatus,
    capabilities: ProviderCapabilities,
) -> ProviderSpec {
    let credential_slot =
        CredentialSlotId::new(id).expect("built-in credential slot identifier is valid");
    let responses = DirectResponsesSpec::new(base_url, credential_slot)
        .expect("built-in Responses base URL is present");
    ProviderSpec::new(
        id,
        display_name,
        ProviderTransport::Responses,
        compatibility,
        Some(responses),
        capabilities,
        ModelDiscoveryStrategy::ProviderModelsEndpoint { path: "/models" },
    )
    .expect("built-in direct Responses provider definition is valid")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    InvalidProviderId(ProviderIdError),
    UnknownProvider(ProviderId),
    DuplicateProviderId(ProviderId),
    ReservedNativeProviderId(ProviderId),
    MissingDirectResponsesSpec(ProviderId),
    UnexpectedDirectResponsesSpec(ProviderId),
    EmptyDisplayName,
    EmptyBaseUrl,
    InvalidTarget(TargetParseError),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProviderId(error) => write!(formatter, "invalid provider ID: {error}"),
            Self::UnknownProvider(id) => write!(formatter, "unknown provider: {id}"),
            Self::DuplicateProviderId(id) => write!(formatter, "duplicate provider ID: {id}"),
            Self::ReservedNativeProviderId(id) => {
                write!(
                    formatter,
                    "provider ID is reserved for native Codex use: {id}"
                )
            }
            Self::MissingDirectResponsesSpec(id) => {
                write!(
                    formatter,
                    "Responses provider {id} is missing its direct specification"
                )
            }
            Self::UnexpectedDirectResponsesSpec(id) => {
                write!(
                    formatter,
                    "provider {id} cannot have a direct Responses specification for its transport"
                )
            }
            Self::EmptyDisplayName => {
                formatter.write_str("provider display name must not be empty")
            }
            Self::EmptyBaseUrl => formatter.write_str("Responses base URL must not be empty"),
            Self::InvalidTarget(error) => {
                write!(formatter, "invalid provider/model target: {error}")
            }
        }
    }
}

impl std::error::Error for RegistryError {}
