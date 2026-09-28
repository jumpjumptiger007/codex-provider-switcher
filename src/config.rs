use std::{
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use toml_edit::{DocumentMut, TomlError, value};

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

pub fn update_active_selection(
    config_path: impl AsRef<Path>,
    selection: &ActiveSelection,
) -> Result<TransactionResult, ConfigError> {
    update_active_selection_inner(config_path.as_ref(), selection, || {})
}

fn update_active_selection_inner(
    config_path: &Path,
    selection: &ActiveSelection,
    before_conflict_check: impl FnOnce(),
) -> Result<TransactionResult, ConfigError> {
    let original_bytes = fs::read(config_path).map_err(|source| ConfigError::Read {
        path: config_path.to_owned(),
        source,
    })?;
    let original_digest = digest(&original_bytes);
    let original_text = std::str::from_utf8(&original_bytes).map_err(ConfigError::Utf8)?;
    let mut document = original_text
        .parse::<DocumentMut>()
        .map_err(ConfigError::Toml)?;

    document["model"] = value(&selection.model);
    document["model_provider"] = value(&selection.model_provider);
    let rendered = document.to_string();

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

    before_conflict_check();
    let current_bytes = fs::read(config_path).map_err(|source| ConfigError::Read {
        path: config_path.to_owned(),
        source,
    })?;
    if digest(&current_bytes) != original_digest {
        return Err(ConfigError::Conflict {
            path: config_path.to_owned(),
        });
    }

    let backup_path = create_recovery_backup(config_path, &original_bytes)?;
    temp_file
        .persist(config_path)
        .map_err(|error| ConfigError::Persist {
            path: config_path.to_owned(),
            source: error.error,
        })?;

    let written = fs::read_to_string(config_path).map_err(|source| ConfigError::Read {
        path: config_path.to_owned(),
        source,
    })?;
    written.parse::<DocumentMut>().map_err(ConfigError::Toml)?;

    Ok(TransactionResult { backup_path })
}

#[doc(hidden)]
pub mod test_support {
    use std::path::Path;

    use super::{ActiveSelection, ConfigError, TransactionResult, update_active_selection_inner};

    pub fn transaction_with_before_commit(
        config_path: &Path,
        selection: &ActiveSelection,
        before_conflict_check: impl FnOnce(),
    ) -> Result<TransactionResult, ConfigError> {
        update_active_selection_inner(config_path, selection, before_conflict_check)
    }
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn create_recovery_backup(config_path: &Path, contents: &[u8]) -> Result<PathBuf, ConfigError> {
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
    let (_, path) = backup.keep().map_err(|error| ConfigError::Backup {
        directory: parent.to_owned(),
        source: error.error,
    })?;
    Ok(path)
}

#[derive(Debug)]
pub enum ConfigError {
    EmptySelection,
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
