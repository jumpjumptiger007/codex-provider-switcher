use std::path::Path;

use cps::{
    application::{self, ApplicationError, DiagnosticFinding, DiagnosticSeverity, DoctorReport},
    config::{ConfigError, RestoreResult},
    credential::CredentialStore,
    provider::ProviderRegistry,
};

use super::{error, internal_error, with_platform_store};
use crate::dto::{
    DesktopError, DoctorFindingDto, DoctorReportDto, DoctorSeverityDto, RestoreResultDto,
};

/// Explicit, offline diagnostics; filesystem and Keychain inspection stay off the UI thread.
#[tauri::command]
pub async fn run_doctor() -> Result<DoctorReportDto, DesktopError> {
    tauri::async_runtime::spawn_blocking(|| {
        let config_path =
            application::resolve_user_config_path().map_err(|_| error("doctor_unavailable"))?;
        with_platform_store(|store| Ok(run_doctor_at(store, &config_path)))
    })
    .await
    .map_err(|_| internal_error())?
}

fn run_doctor_at(store: Option<&dyn CredentialStore>, config_path: &Path) -> DoctorReportDto {
    application::doctor(&ProviderRegistry::initial(), store, config_path).into()
}

impl From<DoctorReport> for DoctorReportDto {
    fn from(report: DoctorReport) -> Self {
        Self {
            has_errors: report.has_errors(),
            findings: report
                .findings
                .into_iter()
                .map(DoctorFindingDto::from)
                .collect(),
        }
    }
}

impl From<DiagnosticFinding> for DoctorFindingDto {
    fn from(finding: DiagnosticFinding) -> Self {
        Self {
            check: finding.check.to_owned(),
            severity: match finding.severity {
                DiagnosticSeverity::Ok => DoctorSeverityDto::Ok,
                DiagnosticSeverity::Info => DoctorSeverityDto::Info,
                DiagnosticSeverity::Warning => DoctorSeverityDto::Warning,
                DiagnosticSeverity::Error => DoctorSeverityDto::Error,
            },
            message: finding.message,
        }
    }
}

/// Restore only. A later status read must never turn a completed restore into a failure.
#[tauri::command]
pub async fn restore_config() -> Result<RestoreResultDto, DesktopError> {
    tauri::async_runtime::spawn_blocking(|| {
        let config_path =
            application::resolve_user_config_path().map_err(|_| error("restore_unavailable"))?;
        restore_config_at(&config_path)
    })
    .await
    .map_err(|_| internal_error())?
}

fn restore_config_at(config_path: &Path) -> Result<RestoreResultDto, DesktopError> {
    restore_dto(application::restore(config_path).map_err(map_restore_error)?)
}

fn restore_dto(result: RestoreResult) -> Result<RestoreResultDto, DesktopError> {
    Ok(RestoreResultDto {
        restored_from: safe_filename(&result.restored_from)?,
        recovery_backup: safe_filename(&result.backup_path)?,
    })
}

fn safe_filename(path: &Path) -> Result<String, DesktopError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| {
            !name.is_empty() && !name.contains(['/', '\\']) && !name.chars().any(char::is_control)
        })
        .map(str::to_owned)
        .ok_or_else(internal_error)
}

fn map_restore_error(application_error: ApplicationError) -> DesktopError {
    match application_error {
        ApplicationError::Config(ConfigError::NoRecoveryBackup { .. }) => {
            error("no_recovery_backup")
        }
        ApplicationError::Config(ConfigError::AmbiguousLatestBackup { .. }) => {
            error("ambiguous_recovery_backup")
        }
        ApplicationError::Config(
            ConfigError::BackupUtf8 { .. } | ConfigError::BackupToml { .. },
        ) => error("recovery_invalid"),
        ApplicationError::Config(ConfigError::Conflict { .. }) => error("config_conflict"),
        ApplicationError::Config(
            ConfigError::Missing { .. }
            | ConfigError::MissingParent { .. }
            | ConfigError::MissingFileName { .. }
            | ConfigError::Read { .. }
            | ConfigError::BackupDirectoryRead { .. }
            | ConfigError::BackupMetadata { .. }
            | ConfigError::BackupRead { .. },
        )
        | ApplicationError::ConfigPath(_) => error("restore_unavailable"),
        ApplicationError::Config(_) => error("restore_failed"),
        _ => internal_error(),
    }
}

#[cfg(test)]
#[path = "safety_tests.rs"]
mod tests;
