use std::{
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use toml_edit::{Array, DocumentMut, Item, Table, TomlError, value};

use crate::{auth_command::AuthCommand, provider::CodexCustomProviderDefinition};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveSelection {
    pub model: String,
    pub model_provider: String,
}

impl ActiveSelection {
    pub fn new(
        model: impl Into<String>,
        model_provider: impl Into<String>,
    ) -> Result<Self, ConfigError> {
        let selection = Self {
            model: model.into(),
            model_provider: model_provider.into(),
        };
        if selection.model.is_empty() || selection.model_provider.is_empty() {
            return Err(ConfigError::EmptySelection);
        }
        Ok(selection)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionResult {
    pub backup_path: PathBuf,
}

/// Read the explicitly active provider without opening a mutation transaction.
pub fn read_active_provider(config_path: impl AsRef<Path>) -> Result<String, ConfigError> {
    let path = config_path.as_ref();
    let bytes = read_config(path)?;
    let text = std::str::from_utf8(&bytes).map_err(ConfigError::Utf8)?;
    let document = text.parse::<DocumentMut>().map_err(ConfigError::Toml)?;
    let provider = document
        .get("model_provider")
        .ok_or(ConfigError::MissingActiveProvider)?
        .as_str()
        .ok_or(ConfigError::InvalidActiveProvider)?;
    if provider.is_empty() {
        return Err(ConfigError::EmptyActiveProvider);
    }
    Ok(provider.to_owned())
}

pub fn update_active_selection(
    config_path: impl AsRef<Path>,
    selection: &ActiveSelection,
) -> Result<TransactionResult, ConfigError> {
    update_document(
        config_path.as_ref(),
        |document| {
            set_active_selection(document, selection);
            Ok(())
        },
        || {},
    )
}

pub fn update_provider_and_selection(
    config_path: impl AsRef<Path>,
    selection: &ActiveSelection,
    definition: &CodexCustomProviderDefinition,
    auth_command: &AuthCommand,
) -> Result<TransactionResult, ConfigError> {
    update_document(
        config_path.as_ref(),
        |document| {
            ensure_provider_definition(document, definition, auth_command)?;
            set_active_selection(document, selection);
            Ok(())
        },
        || {},
    )
}

fn set_active_selection(document: &mut DocumentMut, selection: &ActiveSelection) {
    document["model"] = value(&selection.model);
    document["model_provider"] = value(&selection.model_provider);
}

fn update_document(
    config_path: &Path,
    mutation: impl FnOnce(&mut DocumentMut) -> Result<(), ConfigError>,
    before_conflict_check: impl FnOnce(),
) -> Result<TransactionResult, ConfigError> {
    let original_bytes = read_config(config_path)?;
    let original_digest = digest(&original_bytes);
    let original_text = std::str::from_utf8(&original_bytes).map_err(ConfigError::Utf8)?;
    let mut document = original_text
        .parse::<DocumentMut>()
        .map_err(ConfigError::Toml)?;
    mutation(&mut document)?;
    let rendered = document.to_string();
    rendered.parse::<DocumentMut>().map_err(ConfigError::Toml)?;

    let parent = config_path
        .parent()
        .ok_or_else(|| ConfigError::MissingParent {
            path: config_path.to_owned(),
        })?;
    let metadata = fs::metadata(config_path).map_err(|source| ConfigError::Read {
        path: config_path.to_owned(),
        source,
    })?;
    let mut temp_file = NamedTempFile::new_in(parent).map_err(ConfigError::TempFile)?;
    temp_file
        .as_file_mut()
        .write_all(rendered.as_bytes())
        .map_err(ConfigError::Write)?;
    temp_file
        .as_file_mut()
        .sync_all()
        .map_err(ConfigError::Write)?;
    fs::set_permissions(temp_file.path(), metadata.permissions()).map_err(ConfigError::Write)?;

    let backup = prepare_recovery_backup(config_path, &original_bytes)?;
    before_conflict_check();
    let current_bytes = match read_config(config_path) {
        Ok(bytes) => bytes,
        Err(ConfigError::Missing { .. }) => {
            return Err(ConfigError::Conflict {
                path: config_path.to_owned(),
            });
        }
        Err(error) => return Err(error),
    };
    if digest(&current_bytes) != original_digest {
        return Err(ConfigError::Conflict {
            path: config_path.to_owned(),
        });
    }

    temp_file
        .persist(config_path)
        .map_err(|error| ConfigError::Persist {
            path: config_path.to_owned(),
            source: error.error,
        })?;
    let (_, backup_path) = backup.keep().map_err(|error| ConfigError::Backup {
        directory: parent.to_owned(),
        source: error.error,
    })?;

    let written = fs::read_to_string(config_path).map_err(|source| ConfigError::Read {
        path: config_path.to_owned(),
        source,
    })?;
    written.parse::<DocumentMut>().map_err(ConfigError::Toml)?;

    Ok(TransactionResult { backup_path })
}

fn read_config(path: &Path) -> Result<Vec<u8>, ConfigError> {
    fs::read(path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            ConfigError::Missing {
                path: path.to_owned(),
            }
        } else {
            ConfigError::Read {
                path: path.to_owned(),
                source,
            }
        }
    })
}

#[doc(hidden)]
pub mod test_support {
    use std::path::Path;

    use super::{
        ActiveSelection, ConfigError, TransactionResult, set_active_selection, update_document,
    };

    pub fn transaction_with_before_commit(
        config_path: &Path,
        selection: &ActiveSelection,
        before_conflict_check: impl FnOnce(),
    ) -> Result<TransactionResult, ConfigError> {
        update_document(
            config_path,
            |document| {
                set_active_selection(document, selection);
                Ok(())
            },
            before_conflict_check,
        )
    }
}

fn ensure_provider_definition(
    document: &mut DocumentMut,
    definition: &CodexCustomProviderDefinition,
    auth_command: &AuthCommand,
) -> Result<(), ConfigError> {
    let provider_id = definition.provider_id.as_str();
    let conflict = || ConfigError::ProviderDefinitionConflict {
        provider_id: provider_id.to_owned(),
    };

    if let Some(existing_providers) = document.get("model_providers") {
        let Some(providers) = existing_providers.as_table() else {
            return Err(conflict());
        };
        if let Some(existing) = providers.get(provider_id) {
            if provider_definition_matches(existing, definition, auth_command) {
                return Ok(());
            }
            return Err(conflict());
        }
    }

    let root = document.as_table_mut();
    if !root.contains_key("model_providers") {
        root.insert("model_providers", Item::Table(Table::new()));
    }
    let providers = root
        .get_mut("model_providers")
        .and_then(Item::as_table_mut)
        .ok_or_else(conflict)?;
    providers.insert(
        provider_id,
        provider_definition_item(definition, auth_command),
    );
    Ok(())
}

fn provider_definition_item(
    definition: &CodexCustomProviderDefinition,
    auth_command: &AuthCommand,
) -> Item {
    let mut args = Array::new();
    for argument in &auth_command.args {
        args.push(argument.clone());
    }

    let mut auth = Table::new();
    auth.insert("command", value(auth_command.command));
    auth.insert("args", value(args));
    auth.insert(
        "refresh_interval_ms",
        value(auth_command.refresh_interval_ms as i64),
    );

    let mut provider = Table::new();
    provider.insert("name", value(&definition.display_name));
    provider.insert("base_url", value(&definition.base_url));
    provider.insert("wire_api", value(definition.wire_api.as_config_value()));
    provider.insert("auth", Item::Table(auth));
    Item::Table(provider)
}

fn provider_definition_matches(
    item: &Item,
    definition: &CodexCustomProviderDefinition,
    auth_command: &AuthCommand,
) -> bool {
    let Some(table) = item.as_table() else {
        return false;
    };
    if table.len() != 4
        || table.get("name").and_then(Item::as_str) != Some(definition.display_name.as_str())
        || table.get("base_url").and_then(Item::as_str) != Some(definition.base_url.as_str())
        || table.get("wire_api").and_then(Item::as_str)
            != Some(definition.wire_api.as_config_value())
    {
        return false;
    }
    let Some(auth) = table.get("auth").and_then(Item::as_table) else {
        return false;
    };
    if auth.len() != 3
        || auth.get("command").and_then(Item::as_str) != Some(auth_command.command)
        || auth.get("refresh_interval_ms").and_then(Item::as_integer)
            != Some(auth_command.refresh_interval_ms as i64)
    {
        return false;
    }
    let expected_args = auth_command
        .args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    auth.get("args")
        .and_then(Item::as_array)
        .is_some_and(|args| {
            args.len() == expected_args.len()
                && args
                    .iter()
                    .zip(expected_args)
                    .all(|(actual, expected)| actual.as_str() == Some(expected))
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigPathError {
    EmptyCodexHome,
    HomeUnavailable,
}

impl fmt::Display for ConfigPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCodexHome => formatter.write_str("CODEX_HOME is set but empty"),
            Self::HomeUnavailable => formatter.write_str("HOME is unavailable or empty"),
        }
    }
}

impl std::error::Error for ConfigPathError {}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn prepare_recovery_backup(
    config_path: &Path,
    contents: &[u8],
) -> Result<NamedTempFile, ConfigError> {
    let parent = config_path
        .parent()
        .ok_or_else(|| ConfigError::MissingParent {
            path: config_path.to_owned(),
        })?;
    let name = config_path
        .file_name()
        .ok_or_else(|| ConfigError::MissingFileName {
            path: config_path.to_owned(),
        })?;
    let prefix = format!("{}.cps-backup-", name.to_string_lossy());
    let mut backup = tempfile::Builder::new()
        .prefix(&prefix)
        .tempfile_in(parent)
        .map_err(|source| ConfigError::Backup {
            directory: parent.to_owned(),
            source,
        })?;
    backup
        .as_file_mut()
        .write_all(contents)
        .map_err(ConfigError::Write)?;
    backup
        .as_file_mut()
        .sync_all()
        .map_err(ConfigError::Write)?;
    Ok(backup)
}

#[derive(Debug)]
pub enum ConfigError {
    EmptySelection,
    MissingActiveProvider,
    InvalidActiveProvider,
    EmptyActiveProvider,
    Missing {
        path: PathBuf,
    },
    ProviderDefinitionConflict {
        provider_id: String,
    },
    MissingParent {
        path: PathBuf,
    },
    MissingFileName {
        path: PathBuf,
    },
    Read {
        path: PathBuf,
        source: io::Error,
    },
    Utf8(std::str::Utf8Error),
    Toml(TomlError),
    TempFile(io::Error),
    Write(io::Error),
    Conflict {
        path: PathBuf,
    },
    Backup {
        directory: PathBuf,
        source: io::Error,
    },
    Persist {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySelection => {
                formatter.write_str("model and model_provider must not be empty")
            }
            Self::MissingActiveProvider => {
                formatter.write_str("Codex config has no active model_provider")
            }
            Self::InvalidActiveProvider => {
                formatter.write_str("active model_provider must be a string")
            }
            Self::EmptyActiveProvider => {
                formatter.write_str("active model_provider must not be empty")
            }
            Self::Missing { path } => write!(
                formatter,
                "Codex config file does not exist: {}",
                path.display()
            ),
            Self::ProviderDefinitionConflict { provider_id } => write!(
                formatter,
                "existing Codex provider definition conflicts with CPS provider {provider_id}"
            ),
            Self::MissingParent { path } => {
                write!(formatter, "config path has no parent: {}", path.display())
            }
            Self::MissingFileName { path } => write!(
                formatter,
                "config path has no file name: {}",
                path.display()
            ),
            Self::Read { path, source } => {
                write!(formatter, "could not read {}: {source}", path.display())
            }
            Self::Utf8(source) => write!(formatter, "config is not UTF-8: {source}"),
            Self::Toml(source) => write!(formatter, "config is invalid TOML: {source}"),
            Self::TempFile(source) => write!(
                formatter,
                "could not create temporary config file: {source}"
            ),
            Self::Write(source) => {
                write!(formatter, "could not write config transaction: {source}")
            }
            Self::Conflict { path } => write!(
                formatter,
                "config changed during transaction: {}",
                path.display()
            ),
            Self::Backup { directory, source } => write!(
                formatter,
                "could not create recovery backup in {}: {source}",
                directory.display()
            ),
            Self::Persist { path, source } => {
                write!(formatter, "could not replace {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ConfigError {}
