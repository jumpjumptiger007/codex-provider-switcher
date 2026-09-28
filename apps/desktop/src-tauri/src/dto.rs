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

/// Safe, stable error contract for fallible Desktop commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopError {
    pub code: String,
    pub message: String,
}
