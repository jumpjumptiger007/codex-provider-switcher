use std::fmt;

use clap::{Parser, Subcommand};

use crate::{
    application::{self, ApplicationError},
    domain::{ProviderModelTarget, ProviderTransport},
    provider::ProviderRegistry,
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
        command => Err(CliError::NotImplemented(command)),
    }
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
                write!(formatter, "{:?} is not implemented in Gate 4", command)
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
