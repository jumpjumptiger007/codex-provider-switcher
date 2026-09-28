use serde::Serialize;

/// Desktop-owned serializable projection of the provider registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDto {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub compatibility: String,
    pub discovery: String,
    pub requires_credential: bool,
}

/// Read-only active Codex selection presented to the Desktop frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusDto {
    pub provider: String,
    pub model: String,
    pub target: String,
    pub known_provider: bool,
    pub transport: Option<String>,
}

/// Desktop-owned credential state; credential material is never serialized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatusDto {
    pub provider_id: String,
    pub status: String,
}

/// A provider model ID and the exact CPS target used to switch to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDto {
    pub id: String,
    pub target: String,
}

/// Safe, stable error contract for fallible Desktop commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopError {
    pub code: String,
    pub message: String,
}

/// Severity is a closed Desktop contract, independent of the core Rust enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DoctorSeverityDto {
    Ok,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorFindingDto {
    pub check: String,
    pub severity: DoctorSeverityDto,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReportDto {
    pub findings: Vec<DoctorFindingDto>,
    pub has_errors: bool,
}

/// Both fields contain filenames only, never absolute config or home paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResultDto {
    pub restored_from: String,
    pub recovery_backup: String,
}
