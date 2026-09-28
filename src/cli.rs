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
    List,
    Models { provider: Option<String> },
    Status,
    Doctor,
    Auth { provider: String },
    Restore,
}

pub fn execute(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Use { target } => use_target(target),
        Command::Auth { provider } => auth_provider(&provider),
        Command::List => list_providers(),
        Command::Models { provider } => list_models(provider.as_deref()),
        Command::Status => show_status(),
        Command::Doctor => run_doctor(),
        Command::Restore => restore_config(),
    }
}

fn run_doctor() -> Result<(), CliError> {
    let registry = ProviderRegistry::initial();
    let config_path = application::resolve_user_config_path().map_err(CliError::Application)?;
    #[cfg(target_os = "macos")]
    let report = {
        let store = crate::credential::macos::MacOsKeychainStore::new();
        application::doctor(&registry, Some(&store), config_path)
    };
    #[cfg(not(target_os = "macos"))]
    let report = application::doctor(&registry, None, config_path);
    println!("{}", format_doctor_report(&report));
    if report.has_errors() {
        Err(CliError::DoctorFoundErrors)
    } else {
        Ok(())
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

/// Render an ordered, stable, secret-free local diagnostics report.
pub fn format_doctor_report(report: &application::DoctorReport) -> String {
    let mut lines = report
        .findings
        .iter()
        .map(|finding| {
            format!(
                "[{}] {}: {}",
                severity_label(finding.severity),
                finding.check,
                finding.message
            )
        })
        .collect::<Vec<_>>();
    lines.push(format!(
        "result: {}",
        if report.has_errors() {
            "errors"
        } else if report.findings.iter().any(|finding| matches!(
            finding.severity,
            application::DiagnosticSeverity::Warning | application::DiagnosticSeverity::Info
        )) {
            "warnings"
        } else {
            "healthy"
        }
    ));
    lines.join("\n")
}

fn severity_label(severity: application::DiagnosticSeverity) -> &'static str {
    match severity {
        application::DiagnosticSeverity::Ok => "ok",
        application::DiagnosticSeverity::Info => "info",
        application::DiagnosticSeverity::Warning => "warning",
        application::DiagnosticSeverity::Error => "error",
    }
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
    Application(ApplicationError),
    Prompt(std::io::Error),
    DoctorFoundErrors,
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        2
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Application(error) => error.fmt(formatter),
            Self::Prompt(error) => write!(formatter, "could not read credential securely: {error}"),
            Self::DoctorFoundErrors => {
                formatter.write_str("doctor found local configuration errors")
            }
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
    use clap::{CommandFactory, Parser, error::ErrorKind};

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
    fn parses_all_supported_commands() {
        for command in [
            ["cps", "list"].as_slice(),
            ["cps", "auth", "deepseek"].as_slice(),
            ["cps", "models"].as_slice(),
            ["cps", "models", "openrouter"].as_slice(),
            ["cps", "use", "openrouter/openai/example-model"].as_slice(),
            ["cps", "status"].as_slice(),
            ["cps", "doctor"].as_slice(),
            ["cps", "restore"].as_slice(),
        ] {
            assert!(
                Cli::try_parse_from(command).is_ok(),
                "failed to parse {command:?}"
            );
        }
    }

    #[test]
    fn rejects_deferred_commands_as_unknown_subcommands() {
        for command in ["add", "disable", "bridge"] {
            let error = Cli::try_parse_from(["cps", command]).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::InvalidSubcommand, "{command}");
            assert!(!error.to_string().contains("not implemented"));
        }
    }

    #[test]
    fn help_lists_only_supported_mvp_commands() {
        let help = Cli::command().render_help().to_string();
        for command in [
            "use", "auth", "list", "models", "status", "doctor", "restore",
        ] {
            assert!(
                help.lines()
                    .any(|line| line.trim_start().starts_with(&format!("{command} "))
                        || line.trim() == command),
                "missing {command} in help:\n{help}"
            );
        }
        for command in ["add", "disable", "bridge"] {
            assert!(
                !help
                    .lines()
                    .any(|line| line.trim_start().starts_with(&format!("{command} "))
                        || line.trim() == command),
                "unexpected {command} in help:\n{help}"
            );
        }
    }

    #[test]
    fn help_and_package_version_are_available() {
        let help_error = Cli::try_parse_from(["cps", "--help"]).unwrap_err();
        assert_eq!(help_error.kind(), ErrorKind::DisplayHelp);
        let version_error = Cli::try_parse_from(["cps", "--version"]).unwrap_err();
        assert_eq!(version_error.kind(), ErrorKind::DisplayVersion);
        assert_eq!(version_error.to_string(), "cps 0.1.0\n");
    }

    #[test]
    fn rejects_malformed_target() {
        assert!(Cli::try_parse_from(["cps", "use", "openai"]).is_err());
        assert!(Cli::try_parse_from(["cps", "use", "openai/"]).is_err());
    }
}
