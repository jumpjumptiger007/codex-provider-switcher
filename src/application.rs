use std::{error::Error, ffi::OsStr, fmt, path::PathBuf};

use crate::{
    auth_command::project_keychain_auth_command,
    config::{
        self, ActiveConfigSelection, ActiveSelection, ConfigError, ConfigPathError,
        ProviderDefinitionInspection, RecoveryInspection, RestoreResult,
    },
    credential::{CredentialStore, CredentialStoreError, SecretValue, SecretValueError},
    domain::{ProviderModelTarget, ProviderTransport},
    model_discovery::{DiscoveredModel, ModelDiscoveryError},
    provider::{ModelDiscoveryStrategy, ProviderRegistry, RegistryError},
};

pub fn store_provider_credential(
    registry: &ProviderRegistry,
    store: &dyn CredentialStore,
    provider_id: &str,
    secret: &str,
) -> Result<(), ApplicationError> {
    let provider = registry.lookup(provider_id)?;
    let Some(direct) = provider.direct_responses.as_ref() else {
        return Err(ApplicationError::CredentialNotApplicable(
            provider.id.to_string(),
        ));
    };
    let secret = SecretValue::new(secret).map_err(ApplicationError::EmptyCredential)?;
    store.set(&direct.credential_slot, &secret)?;
    Ok(())
}

pub fn use_provider_model(
    registry: &ProviderRegistry,
    store: Option<&dyn CredentialStore>,
    config_path: impl AsRef<std::path::Path>,
    target: &ProviderModelTarget,
) -> Result<config::TransactionResult, ApplicationError> {
    let resolved = registry.resolve(target)?;
    let selection = ActiveSelection::new(resolved.model, resolved.provider.id.as_str())?;

    if resolved.provider.transport == ProviderTransport::Native {
        return Ok(config::update_active_selection(config_path, &selection)?);
    }

    let Some(direct) = resolved.provider.direct_responses.as_ref() else {
        return Err(ApplicationError::ProviderHasNoCredentialSlot(
            resolved.provider.id.to_string(),
        ));
    };
    let store = store.ok_or(ApplicationError::UnsupportedPlatform)?;
    if store.get(&direct.credential_slot)?.is_none() {
        return Err(ApplicationError::MissingProviderCredential(
            resolved.provider.id.to_string(),
        ));
    }

    let definition = resolved
        .provider
        .project_custom_codex_provider()?
        .expect("direct Responses providers project to a custom Codex provider");
    let auth_command = project_keychain_auth_command(&direct.credential_slot);
    Ok(config::update_provider_and_selection(
        config_path,
        &selection,
        &definition,
        &auth_command,
    )?)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub selection: ActiveConfigSelection,
    pub known_provider: Option<ProviderTransport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Ok,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticFinding {
    pub check: &'static str,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DoctorReport {
    pub findings: Vec<DiagnosticFinding>,
}

impl DoctorReport {
    pub fn has_errors(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity == DiagnosticSeverity::Error)
    }

    fn push(
        &mut self,
        check: &'static str,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
    ) {
        self.findings.push(DiagnosticFinding {
            check,
            severity,
            message: message.into(),
        });
    }
}

/// Diagnose local CPS readiness without changing config, backups, or credentials.
pub fn doctor(
    registry: &ProviderRegistry,
    store: Option<&dyn CredentialStore>,
    config_path: impl AsRef<std::path::Path>,
) -> DoctorReport {
    let config_path = config_path.as_ref();
    let mut report = DoctorReport::default();
    if let Err(error) = config::validate_config(config_path) {
        report.push(
            "config",
            DiagnosticSeverity::Error,
            config_error_summary(&error),
        );
        add_recovery_finding(&mut report, config_path);
        return report;
    }
    report.push("config", DiagnosticSeverity::Ok, "valid");

    let selection = match config::read_active_selection(config_path) {
        Ok(selection) => selection,
        Err(error) => {
            report.push(
                "selection",
                DiagnosticSeverity::Error,
                selection_error_summary(&error),
            );
            add_recovery_finding(&mut report, config_path);
            return report;
        }
    };
    report.push(
        "selection",
        DiagnosticSeverity::Ok,
        format!("{}/{}", selection.model_provider, selection.model),
    );

    let Some(provider) = registry.get(&selection.model_provider) else {
        report.push(
            "provider",
            DiagnosticSeverity::Warning,
            "not managed by CPS",
        );
        add_recovery_finding(&mut report, config_path);
        return report;
    };
    report.push(
        "provider",
        DiagnosticSeverity::Ok,
        format!("{} ({})", provider.id, transport_name(provider.transport)),
    );

    if provider.transport == ProviderTransport::Responses {
        let definition = match provider.project_custom_codex_provider() {
            Ok(Some(definition)) => definition,
            _ => {
                report.push(
                    "provider_definition",
                    DiagnosticSeverity::Error,
                    "could not project CPS definition",
                );
                add_recovery_finding(&mut report, config_path);
                return report;
            }
        };
        let auth_command = project_keychain_auth_command(
            &provider
                .direct_responses
                .as_ref()
                .expect("Responses providers have direct settings")
                .credential_slot,
        );
        match config::inspect_provider_definition(config_path, &definition, &auth_command) {
            Ok(ProviderDefinitionInspection::Matches) => {
                report.push("provider_definition", DiagnosticSeverity::Ok, "matches CPS")
            }
            Ok(ProviderDefinitionInspection::Missing) => {
                report.push("provider_definition", DiagnosticSeverity::Error, "missing")
            }
            Ok(ProviderDefinitionInspection::Conflicts) => report.push(
                "provider_definition",
                DiagnosticSeverity::Error,
                "conflicts with CPS",
            ),
            Ok(ProviderDefinitionInspection::Malformed) => report.push(
                "provider_definition",
                DiagnosticSeverity::Error,
                "model_providers is malformed",
            ),
            Err(_) => report.push(
                "provider_definition",
                DiagnosticSeverity::Error,
                "could not inspect",
            ),
        }

        match store {
            Some(store) => match store.get(
                &provider
                    .direct_responses
                    .as_ref()
                    .expect("Responses providers have direct settings")
                    .credential_slot,
            ) {
                Ok(Some(_)) => report.push("credential", DiagnosticSeverity::Ok, "present"),
                Ok(None) => report.push("credential", DiagnosticSeverity::Error, "missing"),
                Err(_) => report.push(
                    "credential",
                    DiagnosticSeverity::Error,
                    "could not retrieve stored credential",
                ),
            },
            None => report.push(
                "credential",
                DiagnosticSeverity::Error,
                "credential store unavailable",
            ),
        }
    }
    add_recovery_finding(&mut report, config_path);
    report
}

fn transport_name(transport: ProviderTransport) -> &'static str {
    match transport {
        ProviderTransport::Native => "native",
        ProviderTransport::Responses => "responses",
        ProviderTransport::Bridge => "bridge",
    }
}

fn config_error_summary(error: &ConfigError) -> &'static str {
    match error {
        ConfigError::Missing { .. } => "missing",
        ConfigError::InvalidConfigUtf8 { .. } => "invalid UTF-8",
        ConfigError::InvalidConfigToml { .. } => "invalid TOML",
        _ => "unreadable",
    }
}

fn selection_error_summary(error: &ConfigError) -> &'static str {
    match error {
        ConfigError::MissingActiveModel => "model is missing",
        ConfigError::InvalidActiveModel => "model must be a string",
        ConfigError::EmptyActiveModel => "model is blank",
        ConfigError::MissingActiveProvider => "model_provider is missing",
        ConfigError::InvalidActiveProvider => "model_provider must be a string",
        ConfigError::EmptyActiveProvider => "model_provider is blank",
        _ => "could not inspect active selection",
    }
}

fn add_recovery_finding(report: &mut DoctorReport, config_path: &std::path::Path) {
    match config::inspect_recovery(config_path) {
        RecoveryInspection::None => report.push(
            "recovery",
            DiagnosticSeverity::Info,
            "no CPS recovery backup",
        ),
        RecoveryInspection::Ready { backup_path } => report.push(
            "recovery",
            DiagnosticSeverity::Ok,
            format!(
                "latest backup is valid ({})",
                backup_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ),
        ),
        RecoveryInspection::Ambiguous => report.push(
            "recovery",
            DiagnosticSeverity::Warning,
            "newest backup is ambiguous",
        ),
        RecoveryInspection::InvalidUtf8 => report.push(
            "recovery",
            DiagnosticSeverity::Warning,
            "latest backup is invalid UTF-8",
        ),
        RecoveryInspection::InvalidToml => report.push(
            "recovery",
            DiagnosticSeverity::Warning,
            "latest backup is invalid TOML",
        ),
        RecoveryInspection::Unavailable => report.push(
            "recovery",
            DiagnosticSeverity::Warning,
            "could not inspect CPS recovery backups",
        ),
    }
}

/// Inspect explicit active config values and classify a provider only when it is in the registry.
pub fn status(
    registry: &ProviderRegistry,
    config_path: impl AsRef<std::path::Path>,
) -> Result<Status, ApplicationError> {
    let selection = config::read_active_selection(config_path)?;
    let known_provider = registry
        .get(&selection.model_provider)
        .map(|provider| provider.transport);
    Ok(Status {
        selection,
        known_provider,
    })
}

/// Restore the newest validated CPS recovery backup for the active Codex config file.
pub fn restore(
    config_path: impl AsRef<std::path::Path>,
) -> Result<RestoreResult, ApplicationError> {
    Ok(config::restore_config(config_path)?)
}

/// Discover advertised model IDs for one explicitly selected provider.
pub fn discover_provider_models(
    registry: &ProviderRegistry,
    store: Option<&dyn CredentialStore>,
    provider_id: &str,
) -> Result<Vec<DiscoveredModel>, ApplicationError> {
    let provider = registry.lookup(provider_id)?;
    let endpoint = match provider.model_discovery {
        ModelDiscoveryStrategy::CodexManaged => {
            return Err(ApplicationError::CodexManagedModelDiscovery(
                provider.id.to_string(),
            ));
        }
        ModelDiscoveryStrategy::ProviderModelsEndpoint(endpoint) => endpoint,
    };
    let Some(direct) = provider.direct_responses.as_ref() else {
        return Err(ApplicationError::InvalidDiscoveryProvider(
            provider.id.to_string(),
        ));
    };
    let store = store.ok_or(ApplicationError::UnsupportedPlatform)?;
    let credential = store
        .get(&direct.credential_slot)?
        .ok_or_else(|| ApplicationError::MissingProviderCredential(provider.id.to_string()))?;
    Ok(crate::model_discovery::discover_models(
        provider.id.as_str(),
        &direct.base_url,
        endpoint,
        &credential,
    )?)
}

pub fn resolve_user_config_path() -> Result<PathBuf, ApplicationError> {
    resolve_user_config_path_from(
        std::env::var_os("CODEX_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
    .map_err(ApplicationError::ConfigPath)
}

pub fn resolve_user_config_path_from(
    codex_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Result<PathBuf, ConfigPathError> {
    if let Some(codex_home) = codex_home {
        if codex_home.is_empty() {
            return Err(ConfigPathError::EmptyCodexHome);
        }
        return Ok(PathBuf::from(codex_home).join("config.toml"));
    }
    let home = home.ok_or(ConfigPathError::HomeUnavailable)?;
    if home.is_empty() {
        return Err(ConfigPathError::HomeUnavailable);
    }
    Ok(PathBuf::from(home).join(".codex").join("config.toml"))
}

#[derive(Debug)]
pub enum ApplicationError {
    Registry(RegistryError),
    Credential(CredentialStoreError),
    Config(ConfigError),
    ConfigPath(ConfigPathError),
    ModelDiscovery(ModelDiscoveryError),
    EmptyCredential(SecretValueError),
    CredentialNotApplicable(String),
    ProviderHasNoCredentialSlot(String),
    MissingProviderCredential(String),
    UnsupportedPlatform,
    CodexManagedModelDiscovery(String),
    InvalidDiscoveryProvider(String),
}

impl From<RegistryError> for ApplicationError {
    fn from(error: RegistryError) -> Self {
        Self::Registry(error)
    }
}

impl From<CredentialStoreError> for ApplicationError {
    fn from(error: CredentialStoreError) -> Self {
        Self::Credential(error)
    }
}

impl From<ConfigError> for ApplicationError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<ModelDiscoveryError> for ApplicationError {
    fn from(error: ModelDiscoveryError) -> Self {
        Self::ModelDiscovery(error)
    }
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(error) => error.fmt(formatter),
            Self::Credential(error) => error.fmt(formatter),
            Self::Config(error) => error.fmt(formatter),
            Self::ConfigPath(error) => error.fmt(formatter),
            Self::ModelDiscovery(error) => error.fmt(formatter),
            Self::EmptyCredential(_) => formatter.write_str("credential must not be empty"),
            Self::CredentialNotApplicable(provider) => write!(
                formatter,
                "CPS credential storage does not apply to native provider {provider}"
            ),
            Self::ProviderHasNoCredentialSlot(provider) => {
                write!(formatter, "provider {provider} has no CPS credential slot")
            }
            Self::MissingProviderCredential(provider) => {
                write!(
                    formatter,
                    "no CPS credential is stored for provider {provider}"
                )
            }
            Self::UnsupportedPlatform => {
                formatter.write_str("CPS Keychain authentication is supported only on macOS")
            }
            Self::CodexManagedModelDiscovery(provider) => write!(
                formatter,
                "CPS does not enumerate models for Codex-managed provider {provider} in Gate 5"
            ),
            Self::InvalidDiscoveryProvider(provider) => write!(
                formatter,
                "provider {provider} has a model discovery endpoint without a direct provider configuration"
            ),
        }
    }
}

impl Error for ApplicationError {}
