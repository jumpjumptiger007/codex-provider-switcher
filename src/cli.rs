use std::fmt;

use clap::{Parser, Subcommand};

use crate::domain::ProviderModelTarget;

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
    Err(CliError::NotImplemented(cli.command))
}

#[derive(Debug)]
pub enum CliError {
    NotImplemented(Command),
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
                write!(formatter, "{:?} is not implemented in Gate 1", command)
            }
        }
    }
}

impl std::error::Error for CliError {}

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
