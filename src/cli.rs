use std::fmt;

use clap::{Parser, Subcommand};

use crate::{
    application::{self, ApplicationError},
    config,
    domain::{CompatibilityStatus, ProviderModelTarget, ProviderTransport},
    provider::{ModelDiscoveryStrategy, ProviderRegistry, ProviderSpec},
};

#[derive(Debug, Parser)]
#[command(name = "cps", version, about = "Codex Provider Switcher")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    Use { target: ProviderModelTarget },
    Add { provider: String },
    Disable { provider: String },
    List,
    Models { provider: Option<String> },
    Status,
    Doctor,
    Auth { provider: String },
    Bridge,
    Restore,
}

pub fn execute(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Use { target } => use_target(target),
        Command::Auth { provider } => auth_provider(&provider),
        Command::List => list_providers(),
        Command::Models { provider } => list_models(provider.as_deref()),
        Command::Status => show_status(),
        Command::Restore => restore_config(),
        command => Err(CliError::NotImplemented(command)),
    }
}

fn show_status() -> Result<(), CliError> {
    let registry = ProviderRegistry::initial();
    let config_path = application::resolve_user_config_path().map_err(CliError::Application)?;
    let status = application::status(&registry, config_path).map_err(CliError::Application)?;
    println!("{}", format_status(&status));
    Ok(())
}

fn restore_config() -> Result<(), CliError> {
    let config_path = application::resolve_user_config_path().map_err(CliError::Application)?;
    let result = application::restore(config_path).map_err(CliError::Application)?;
    println!("{}", format_restore_success(&result));
    Ok(())
}

/// Render stable local status without exposing config paths or diagnostic internals.
pub fn format_status(status: &application::Status) -> String {
    let selection = &status.selection;
    let mut output = format!(
        "provider: {}\nmodel: {}\ntarget: {}/{}",
        selection.model_provider, selection.model, selection.model_provider, selection.model
    );
    match status.known_provider {
        Some(ProviderTransport::Native) => {
            output.push_str("\nknown_provider: yes\ntransport: native");
        }
        Some(ProviderTransport::Responses) => {
            output.push_str("\nknown_provider: yes\ntransport: responses");
        }
        Some(ProviderTransport::Bridge) => {
            output.push_str("\nknown_provider: yes\ntransport: bridge");
        }
        None => output.push_str("\nknown_provider: no"),
    }
    output
}

/// Render a compact restore confirmation using only filenames.
pub fn format_restore_success(result: &config::RestoreResult) -> String {
    format!(
        "Restored config from {}.\nRecovery backup: {}",
        display_basename(&result.restored_from),
        display_basename(&result.backup_path)
    )
}

fn display_basename(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn list_providers() -> Result<(), CliError> {
    for row in format_provider_inventory(&ProviderRegistry::initial()) {
        println!("{row}");
    }
    Ok(())
}

fn list_models(explicit_provider: Option<&str>) -> Result<(), CliError> {
    let registry = ProviderRegistry::initial();
    let provider_id = resolve_models_provider(explicit_provider, || {
        let config_path = application::resolve_user_config_path()?;
        config::read_active_provider(config_path).map_err(ApplicationError::Config)
    })
    .map_err(CliError::Application)?;

    #[cfg(target_os = "macos")]
    let models = {
        let store = crate::credential::macos::MacOsKeychainStore::new();
        application::discover_provider_models(&registry, Some(&store), &provider_id)
            .map_err(CliError::Application)?
    };
    #[cfg(not(target_os = "macos"))]
    let models = application::discover_provider_models(&registry, None, &provider_id)
        .map_err(CliError::Application)?;

    for model in models {
        println!("{}", format_qualified_model_target(&provider_id, &model));
    }
    Ok(())
}

/// Resolve the explicit CLI selection or lazily read the explicit active provider.
/// The config path is not accessed when a provider argument was supplied.
pub fn resolve_models_provider(
    explicit_provider: Option<&str>,
    read_active_provider: impl FnOnce() -> Result<String, ApplicationError>,
) -> Result<String, ApplicationError> {
    match explicit_provider {
        Some(provider) => Ok(provider.to_owned()),
        None => read_active_provider(),
    }
}

/// Rows for the offline registry inventory, in the registry's stable order.
pub fn format_provider_inventory(registry: &ProviderRegistry) -> Vec<String> {
    let mut rows = vec!["ID\tNAME\tTRANSPORT\tCOMPATIBILITY\tDISCOVERY".to_owned()];
    rows.extend(registry.iter().map(format_provider_row));
    rows
}

pub fn format_qualified_model_target(
    provider_id: &str,
    model: &crate::model_discovery::DiscoveredModel,
) -> String {
    format!("{provider_id}/{}", model.id)
}

fn format_provider_row(provider: &ProviderSpec) -> String {
    let transport = match provider.transport {
        ProviderTransport::Native => "native",
        ProviderTransport::Responses => "responses",
        ProviderTransport::Bridge => "bridge",
    };
    let compatibility = match provider.compatibility {
        CompatibilityStatus::Verified => "verified",
        CompatibilityStatus::VerifiedBasic => "verified-basic",
        CompatibilityStatus::Degraded => "degraded",
        CompatibilityStatus::Unverified => "unverified",
        CompatibilityStatus::Unsupported => "unsupported",
    };
    let discovery = match provider.model_discovery {
        ModelDiscoveryStrategy::CodexManaged => "codex-managed",
        ModelDiscoveryStrategy::ProviderModelsEndpoint(_) => "provider-models",
    };
    format!(
        "{}\t{}\t{}\t{}\t{}",
        provider.id, provider.display_name, transport, compatibility, discovery
    )
}

fn use_target(target: ProviderModelTarget) -> Result<(), CliError> {
    let registry = ProviderRegistry::initial();
    let resolved = registry
        .resolve(&target)
        .map_err(ApplicationError::Registry)
        .map_err(CliError::Application)?;
    let config_path = application::resolve_user_config_path().map_err(CliError::Application)?;
    if resolved.provider.transport == ProviderTransport::Native {
        application::use_provider_model(&registry, None, config_path, &target)
            .map_err(CliError::Application)?;
    } else {
        #[cfg(target_os = "macos")]
        {
            let store = crate::credential::macos::MacOsKeychainStore::new();
            application::use_provider_model(&registry, Some(&store), config_path, &target)
                .map_err(CliError::Application)?;
        }
        #[cfg(not(target_os = "macos"))]
        {
            return Err(CliError::Application(ApplicationError::UnsupportedPlatform));
        }
    }
    println!("Switched to {}/{}.", target.provider, target.model);
    Ok(())
}

fn auth_provider(provider_id: &str) -> Result<(), CliError> {
    let registry = ProviderRegistry::initial();
    let provider = registry
        .lookup(provider_id)
        .map_err(ApplicationError::Registry)
        .map_err(CliError::Application)?;
    if provider.direct_responses.is_none() {
        return Err(CliError::Application(
            ApplicationError::CredentialNotApplicable(provider.id.to_string()),
        ));
    }

    #[cfg(target_os = "macos")]
    {
        let secret = rpassword::prompt_password("API key: ").map_err(CliError::Prompt)?;
        let store = crate::credential::macos::MacOsKeychainStore::new();
        application::store_provider_credential(&registry, &store, provider_id, &secret)
            .map_err(CliError::Application)?;
        println!("Stored credential for {provider_id}.");
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(CliError::Application(ApplicationError::UnsupportedPlatform))
    }
}

#[derive(Debug)]
pub enum CliError {
    NotImplemented(Command),
    Application(ApplicationError),
    Prompt(std::io::Error),
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        2
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotImplemented(command) => {
                write!(formatter, "{:?} is not implemented", command)
            }
            Self::Application(error) => error.fmt(formatter),
            Self::Prompt(error) => write!(formatter, "could not read credential securely: {error}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<ApplicationError> for CliError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error)
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Command};

    #[test]
    fn parses_use_target() {
        let cli = Cli::try_parse_from(["cps", "use", "openai/gpt-5"]).unwrap();

        assert_eq!(
            cli.command,
            Command::Use {
                target: "openai/gpt-5".parse().unwrap(),
            }
        );
    }

    #[test]
    fn parses_use_target_with_nested_model_id() {
        let cli = Cli::try_parse_from(["cps", "use", "openrouter/openai/gpt-5"]).unwrap();

        assert_eq!(
            cli.command,
            Command::Use {
                target: "openrouter/openai/gpt-5".parse().unwrap(),
            }
        );
    }

    #[test]
    fn parses_representative_commands() {
        for command in [
            ["cps", "add", "openai"].as_slice(),
            ["cps", "disable", "openai"].as_slice(),
            ["cps", "list"].as_slice(),
            ["cps", "models", "openai"].as_slice(),
            ["cps", "models", "openrouter"].as_slice(),
            ["cps", "models"].as_slice(),
            ["cps", "status"].as_slice(),
            ["cps", "doctor"].as_slice(),
            ["cps", "auth", "openai"].as_slice(),
            ["cps", "bridge"].as_slice(),
            ["cps", "restore"].as_slice(),
        ] {
            assert!(
                Cli::try_parse_from(command).is_ok(),
                "failed to parse {command:?}"
            );
        }
    }

    #[test]
    fn rejects_malformed_target() {
        assert!(Cli::try_parse_from(["cps", "use", "openai"]).is_err());
        assert!(Cli::try_parse_from(["cps", "use", "openai/"]).is_err());
    }
}
