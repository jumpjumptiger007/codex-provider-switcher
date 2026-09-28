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
