use std::{error::Error, fmt, time::Duration};

use reqwest::{StatusCode, Url, blocking::Client, header};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    credential::SecretValue,
    provider::{ModelDiscoverySchema, ProviderModelsEndpoint},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredModel {
    pub id: String,
}

impl DiscoveredModel {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidModelResponse {
    TopLevelNotObject,
    InvalidListObject,
    MissingData,
    InvalidData,
    MissingId,
    InvalidId,
    EmptyId,
}

impl fmt::Display for InvalidModelResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TopLevelNotObject => "top-level response must be an object",
            Self::InvalidListObject => "response object must identify a model list",
            Self::MissingData => "response is missing the model data list",
            Self::InvalidData => "response model data must be an array of objects",
            Self::MissingId => "a model entry is missing its ID",
            Self::InvalidId => "a model ID must be a string",
            Self::EmptyId => "a model ID must not be blank",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelDiscoveryError {
    InvalidEndpoint {
        provider_id: String,
    },
    InvalidAuthorizationHeader {
        provider_id: String,
    },
    HttpTransport {
        provider_id: String,
    },
    HttpStatus {
        provider_id: String,
        status: u16,
    },
    InvalidJson {
        provider_id: String,
    },
    InvalidResponse {
        provider_id: String,
        reason: InvalidModelResponse,
    },
}

impl fmt::Display for ModelDiscoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEndpoint { provider_id } => {
                write!(
                    formatter,
                    "invalid model discovery endpoint for provider {provider_id}"
                )
            }
            Self::InvalidAuthorizationHeader { provider_id } => write!(
                formatter,
                "credential cannot be represented as an Authorization header for provider {provider_id}"
            ),
            Self::HttpTransport { provider_id } => {
                write!(
                    formatter,
                    "HTTP model discovery failed for provider {provider_id}"
                )
            }
            Self::HttpStatus {
                provider_id,
                status,
            } => write!(
                formatter,
                "model discovery for provider {provider_id} returned HTTP status {status}"
            ),
            Self::InvalidJson { provider_id } => {
                write!(
                    formatter,
                    "model discovery for provider {provider_id} returned invalid JSON"
                )
            }
            Self::InvalidResponse {
                provider_id,
                reason,
            } => write!(
                formatter,
                "model discovery response for provider {provider_id} is invalid: {reason}"
            ),
        }
    }
}

impl Error for ModelDiscoveryError {}

/// Append a provider-owned discovery suffix to its configured base URL.
///
/// This deliberately does not use URL joining: `/models` is a suffix, so a
/// versioned base such as `https://api.x.ai/v1` remains `/v1/models`.
pub fn endpoint_url(
    provider_id: &str,
    base_url: &str,
    path: &str,
) -> Result<Url, ModelDiscoveryError> {
    let invalid = || ModelDiscoveryError::InvalidEndpoint {
        provider_id: provider_id.to_owned(),
    };
    if path.is_empty()
        || !path.starts_with('/')
        || path.contains('?')
        || path.contains('#')
        || base_url.contains('?')
        || base_url.contains('#')
    {
        return Err(invalid());
    }
    let base = Url::parse(base_url).map_err(|_| invalid())?;
    if !matches!(base.scheme(), "http" | "https")
        || base.host_str().is_none()
        || !base.username().is_empty()
        || base.password().is_some()
    {
        return Err(invalid());
    }
    let endpoint = format!("{}{}", base_url.trim_end_matches('/'), path);
    let url = Url::parse(&endpoint).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(invalid());
    }
    Ok(url)
}

pub fn discover_models(
    provider_id: &str,
    base_url: &str,
    endpoint: ProviderModelsEndpoint,
    credential: &SecretValue,
) -> Result<Vec<DiscoveredModel>, ModelDiscoveryError> {
    let url = endpoint_url(provider_id, base_url, endpoint.path)?;
    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .map_err(|_| ModelDiscoveryError::HttpTransport {
            provider_id: provider_id.to_owned(),
        })?;
    let authorization =
        header::HeaderValue::from_str(&format!("Bearer {}", credential.expose_secret())).map_err(
            |_| ModelDiscoveryError::InvalidAuthorizationHeader {
                provider_id: provider_id.to_owned(),
            },
        )?;
    let response = client
        .get(url)
        .header(header::ACCEPT, "application/json")
        .header(header::AUTHORIZATION, authorization)
        .send()
        .map_err(|_| ModelDiscoveryError::HttpTransport {
            provider_id: provider_id.to_owned(),
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(status_error(provider_id, status));
    }
    let body = response
        .bytes()
        .map_err(|_| ModelDiscoveryError::HttpTransport {
            provider_id: provider_id.to_owned(),
        })?;
    parse_models(provider_id, endpoint.schema, &body)
}

fn status_error(provider_id: &str, status: StatusCode) -> ModelDiscoveryError {
    ModelDiscoveryError::HttpStatus {
        provider_id: provider_id.to_owned(),
        status: status.as_u16(),
    }
}

#[derive(Deserialize)]
struct DeepSeekResponse {
    object: Option<Value>,
    data: Option<Value>,
}

#[derive(Deserialize)]
struct DeepSeekModel {
    id: Option<Value>,
}

#[derive(Deserialize)]
struct XaiResponse {
    object: Option<Value>,
    data: Option<Value>,
}

#[derive(Deserialize)]
struct XaiModel {
    id: Option<Value>,
}

#[derive(Deserialize)]
struct OpenRouterResponse {
    data: Option<Value>,
}

#[derive(Deserialize)]
struct OpenRouterModel {
    id: Option<Value>,
}

pub fn parse_models(
    provider_id: &str,
    schema: ModelDiscoverySchema,
    body: &[u8],
) -> Result<Vec<DiscoveredModel>, ModelDiscoveryError> {
    let value =
        serde_json::from_slice::<Value>(body).map_err(|_| ModelDiscoveryError::InvalidJson {
            provider_id: provider_id.to_owned(),
        })?;
    if !value.is_object() {
        return Err(invalid_response(
            provider_id,
            InvalidModelResponse::TopLevelNotObject,
        ));
    }

    match schema {
        ModelDiscoverySchema::DeepSeek => parse_deepseek(provider_id, value),
        ModelDiscoverySchema::Xai => parse_xai(provider_id, value),
        ModelDiscoverySchema::OpenRouter => parse_openrouter(provider_id, value),
    }
}

fn parse_deepseek(
    provider_id: &str,
    value: Value,
) -> Result<Vec<DiscoveredModel>, ModelDiscoveryError> {
    let response: DeepSeekResponse = decode_response(provider_id, value)?;
    require_list_object(provider_id, response.object)?;
    let data = response
        .data
        .ok_or_else(|| invalid_response(provider_id, InvalidModelResponse::MissingData))?;
    let entries = decode_data::<DeepSeekModel>(provider_id, data)?;
    collect_ids(provider_id, entries.into_iter().map(|model| model.id))
}

fn parse_xai(provider_id: &str, value: Value) -> Result<Vec<DiscoveredModel>, ModelDiscoveryError> {
    let response: XaiResponse = decode_response(provider_id, value)?;
    require_list_object(provider_id, response.object)?;
    let data = response
        .data
        .ok_or_else(|| invalid_response(provider_id, InvalidModelResponse::MissingData))?;
    let entries = decode_data::<XaiModel>(provider_id, data)?;
    collect_ids(provider_id, entries.into_iter().map(|model| model.id))
}

fn parse_openrouter(
    provider_id: &str,
    value: Value,
) -> Result<Vec<DiscoveredModel>, ModelDiscoveryError> {
    let response: OpenRouterResponse = decode_response(provider_id, value)?;
    let data = response
        .data
        .ok_or_else(|| invalid_response(provider_id, InvalidModelResponse::MissingData))?;
    let entries = decode_data::<OpenRouterModel>(provider_id, data)?;
    collect_ids(provider_id, entries.into_iter().map(|model| model.id))
}

fn decode_response<T: for<'de> Deserialize<'de>>(
    provider_id: &str,
    value: Value,
) -> Result<T, ModelDiscoveryError> {
    serde_json::from_value(value)
        .map_err(|_| invalid_response(provider_id, InvalidModelResponse::TopLevelNotObject))
}

fn decode_data<T: for<'de> Deserialize<'de>>(
    provider_id: &str,
    data: Value,
) -> Result<Vec<T>, ModelDiscoveryError> {
    serde_json::from_value(data)
        .map_err(|_| invalid_response(provider_id, InvalidModelResponse::InvalidData))
}

fn require_list_object(
    provider_id: &str,
    object: Option<Value>,
) -> Result<(), ModelDiscoveryError> {
    if object.as_ref().and_then(Value::as_str) == Some("list") {
        Ok(())
    } else {
        Err(invalid_response(
            provider_id,
            InvalidModelResponse::InvalidListObject,
        ))
    }
}

fn collect_ids(
    provider_id: &str,
    ids: impl IntoIterator<Item = Option<Value>>,
) -> Result<Vec<DiscoveredModel>, ModelDiscoveryError> {
    ids.into_iter()
        .map(|id| {
            let id =
                id.ok_or_else(|| invalid_response(provider_id, InvalidModelResponse::MissingId))?;
            let id = id
                .as_str()
                .ok_or_else(|| invalid_response(provider_id, InvalidModelResponse::InvalidId))?;
            if id.trim().is_empty() {
                return Err(invalid_response(provider_id, InvalidModelResponse::EmptyId));
            }
            Ok(DiscoveredModel::new(id))
        })
        .collect()
}

fn invalid_response(provider_id: &str, reason: InvalidModelResponse) -> ModelDiscoveryError {
    ModelDiscoveryError::InvalidResponse {
        provider_id: provider_id.to_owned(),
        reason,
    }
}
