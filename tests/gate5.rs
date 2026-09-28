use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
};

use cps::{
    application::{ApplicationError, discover_provider_models},
    cli::{format_provider_inventory, format_qualified_model_target, resolve_models_provider},
    config::{self, ConfigError},
    credential::{CredentialStore, CredentialStoreError, SecretValue},
    domain::{CompatibilityStatus, ProviderTransport},
    model_discovery::{
        DiscoveredModel, InvalidModelResponse, ModelDiscoveryError, endpoint_url, parse_models,
    },
    provider::{
        CredentialSlotId, DirectResponsesSpec, ModelDiscoverySchema, ModelDiscoveryStrategy,
        ProviderCapabilities, ProviderModelsEndpoint, ProviderRegistry, ProviderSpec,
    },
};
use tempfile::tempdir;

const FAKE_SECRET: &str = "gate5-secret-marker-do-not-print";

#[derive(Default)]
struct FakeStore {
    entries: Mutex<HashMap<String, String>>,
    get_calls: AtomicUsize,
    fail_get: bool,
}

impl FakeStore {
    fn with_credential(provider_id: &str, secret: &str) -> Self {
        let store = Self::default();
        store
            .entries
            .lock()
            .unwrap()
            .insert(provider_id.to_owned(), secret.to_owned());
        store
    }
}

impl CredentialStore for FakeStore {
    fn set(
        &self,
        slot: &CredentialSlotId,
        value: &SecretValue,
    ) -> Result<(), CredentialStoreError> {
        self.entries
            .lock()
            .unwrap()
            .insert(slot.as_str().to_owned(), value.expose_secret().to_owned());
        Ok(())
    }

    fn get(&self, slot: &CredentialSlotId) -> Result<Option<SecretValue>, CredentialStoreError> {
        self.get_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_get {
            return Err(CredentialStoreError::Backend {
                operation: cps::credential::CredentialOperation::Retrieve,
            });
        }
        self.entries
            .lock()
            .unwrap()
            .get(slot.as_str())
            .cloned()
            .map(|value| {
                SecretValue::new(value).map_err(|_| CredentialStoreError::InvalidStoredValue)
            })
            .transpose()
    }

    fn delete(&self, slot: &CredentialSlotId) -> Result<(), CredentialStoreError> {
        self.entries.lock().unwrap().remove(slot.as_str());
        Ok(())
    }
}

struct ServedRequest {
    url: String,
    request: JoinHandle<String>,
}

fn serve_once(status: u16, extra_headers: &[(&str, &str)], body: &str) -> ServedRequest {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let body = body.to_owned();
    let extra_headers = extra_headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect::<Vec<_>>();
    let request = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request_headers(&mut stream);
        write!(stream, "HTTP/1.1 {status} Test\r\n").unwrap();
        for (name, value) in extra_headers {
            write!(stream, "{name}: {value}\r\n").unwrap();
        }
        write!(stream, "Content-Type: application/json\r\n").unwrap();
        write!(stream, "Content-Length: {}\r\n", body.len()).unwrap();
        write!(stream, "Connection: close\r\n\r\n{body}").unwrap();
        stream.flush().unwrap();
        request
    });
    ServedRequest {
        url: format!("http://{address}"),
        request,
    }
}

fn unused_listener() -> (String, TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    (url, listener)
}

fn read_request_headers(stream: &mut TcpStream) -> String {
    let mut reader = BufReader::new(stream);
    let mut request = String::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line.is_empty() {
            break;
        }
        let done = line == "\r\n" || line == "\n";
        request.push_str(&line);
        if done {
            break;
        }
    }
    request
}

fn registry_with_provider(
    provider_id: &str,
    base_url: &str,
    schema: ModelDiscoverySchema,
) -> ProviderRegistry {
    let slot = CredentialSlotId::new(provider_id).unwrap();
    let direct = DirectResponsesSpec::new(base_url, slot).unwrap();
    let provider = ProviderSpec::new(
        provider_id,
        provider_id,
        ProviderTransport::Responses,
        CompatibilityStatus::Unverified,
        Some(direct),
        ProviderCapabilities::UNKNOWN,
        ModelDiscoveryStrategy::ProviderModelsEndpoint(ProviderModelsEndpoint {
            path: "/models",
            schema,
        }),
    )
    .unwrap();
    let mut registry = ProviderRegistry::new();
    registry.register(provider).unwrap();
    registry
}

fn sample_response(schema: ModelDiscoverySchema, model_id: &str) -> &'static str {
    match schema {
        ModelDiscoverySchema::DeepSeek => {
            r#"{"object":"list","data":[{"id":"deepseek-flash","owned_by":"deepseek","extra":{"new":true}}],"extra":"ignored"}"#
        }
        ModelDiscoverySchema::Xai => {
            r#"{"object":"list","data":[{"id":"grok-4","aliases":["grok-latest"],"context_length":256000,"extra":"ignored"}],"extra":"ignored"}"#
        }
        ModelDiscoverySchema::OpenRouter => {
            if model_id == "openai/gpt-5" {
                r#"{"data":[{"id":"openai/gpt-5","name":"GPT 5","pricing":{"prompt":"1"},"extra":true}],"extra":"ignored"}"#
            } else {
                r#"{"data":[{"id":"openrouter/model","extra":true}],"extra":"ignored"}"#
            }
        }
    }
}

#[test]
fn provider_endpoint_urls_preserve_configured_version_prefixes() {
    let registry = ProviderRegistry::initial();
    let expected = [
        ("deepseek", "https://api.deepseek.com/models"),
        ("xai", "https://api.x.ai/v1/models"),
        ("openrouter", "https://openrouter.ai/api/v1/models"),
    ];
    for (provider_id, url) in expected {
        let provider = registry.lookup(provider_id).unwrap();
        let direct = provider.direct_responses.as_ref().unwrap();
        let ModelDiscoveryStrategy::ProviderModelsEndpoint(endpoint) = provider.model_discovery
        else {
            panic!("direct provider should use an endpoint");
        };
        assert_eq!(
            endpoint_url(provider_id, &direct.base_url, endpoint.path)
                .unwrap()
                .as_str(),
            url
        );
    }
}

#[test]
fn each_provider_schema_extracts_only_exact_model_ids_and_ignores_metadata() {
    for (provider, schema, expected) in [
        ("deepseek", ModelDiscoverySchema::DeepSeek, "deepseek-flash"),
        ("xai", ModelDiscoverySchema::Xai, "grok-4"),
        (
            "openrouter",
            ModelDiscoverySchema::OpenRouter,
            "openai/gpt-5",
        ),
    ] {
        let body = sample_response(schema, expected);
        let models = parse_models(provider, schema, body.as_bytes()).unwrap();
        assert_eq!(models, vec![DiscoveredModel::new(expected)]);
    }
}

#[test]
fn openrouter_nested_model_id_and_cli_target_are_preserved_exactly() {
    let models = parse_models(
        "openrouter",
        ModelDiscoverySchema::OpenRouter,
        sample_response(ModelDiscoverySchema::OpenRouter, "openai/gpt-5").as_bytes(),
    )
    .unwrap();
    assert_eq!(models[0].id, "openai/gpt-5");
    let target = format_qualified_model_target("openrouter", &models[0]);
    assert_eq!(target, "openrouter/openai/gpt-5");
    assert_eq!(
        target
            .parse::<cps::domain::ProviderModelTarget>()
            .unwrap()
            .model,
        "openai/gpt-5"
    );
}

#[test]
fn empty_valid_model_list_is_successful() {
    let models = parse_models(
        "openrouter",
        ModelDiscoverySchema::OpenRouter,
        br#"{"data":[]}"#,
    )
    .unwrap();
    assert!(models.is_empty());
}

#[test]
fn schema_errors_are_typed_and_deterministic() {
    let cases = [
        (
            br#"{"object":"list"}"#.as_slice(),
            InvalidModelResponse::MissingData,
        ),
        (
            br#"{"object":"list","data":{}}"#.as_slice(),
            InvalidModelResponse::InvalidData,
        ),
        (
            br#"{"object":"list","data":[{}]}"#.as_slice(),
            InvalidModelResponse::MissingId,
        ),
        (
            br#"{"object":"list","data":[{"id":42}]}"#.as_slice(),
            InvalidModelResponse::InvalidId,
        ),
        (
            br#"{"object":"list","data":[{"id":"  \t"}]}"#.as_slice(),
            InvalidModelResponse::EmptyId,
        ),
        (
            br#"{"object":"not-list","data":[]}"#.as_slice(),
            InvalidModelResponse::InvalidListObject,
        ),
    ];
    for (body, reason) in cases {
        assert_eq!(
            parse_models("deepseek", ModelDiscoverySchema::DeepSeek, body),
            Err(ModelDiscoveryError::InvalidResponse {
                provider_id: "deepseek".to_owned(),
                reason,
            })
        );
    }
    assert!(matches!(
        parse_models("deepseek", ModelDiscoverySchema::DeepSeek, b"{"),
        Err(ModelDiscoveryError::InvalidJson { provider_id }) if provider_id == "deepseek"
    ));
}

#[test]
fn malformed_top_level_is_a_schema_error() {
    assert_eq!(
        parse_models("xai", ModelDiscoverySchema::Xai, br#"[]"#),
        Err(ModelDiscoveryError::InvalidResponse {
            provider_id: "xai".to_owned(),
            reason: InvalidModelResponse::TopLevelNotObject,
        })
    );
}

#[test]
fn discovery_sends_bearer_authorization_and_json_accept_headers() {
    let schema = ModelDiscoverySchema::DeepSeek;
    let server = serve_once(200, &[], sample_response(schema, "unused"));
    let registry = registry_with_provider("deepseek", &server.url, schema);
    let store = FakeStore::with_credential("deepseek", FAKE_SECRET);
    let models = discover_provider_models(&registry, Some(&store), "deepseek").unwrap();
    let request = server.request.join().unwrap();
    assert!(request.starts_with("GET /models HTTP/1.1\r\n"));
    assert!(
        request
            .lines()
            .any(|line| line.eq_ignore_ascii_case(&format!("authorization: Bearer {FAKE_SECRET}")))
    );
    assert!(
        request
            .lines()
            .any(|line| line.eq_ignore_ascii_case("accept: application/json"))
    );
    assert_eq!(models[0].id, "deepseek-flash");
}

#[test]
fn missing_credential_fails_before_http_request() {
    let (url, listener) = unused_listener();
    let registry = registry_with_provider("deepseek", &url, ModelDiscoverySchema::DeepSeek);
    let store = FakeStore::default();
    let error = discover_provider_models(&registry, Some(&store), "deepseek").unwrap_err();
    assert!(
        matches!(error, ApplicationError::MissingProviderCredential(provider) if provider == "deepseek")
    );
    assert_eq!(store.get_calls.load(Ordering::SeqCst), 1);
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[test]
fn native_provider_fails_before_credential_lookup_or_http() {
    let store = FakeStore::default();
    let error =
        discover_provider_models(&ProviderRegistry::initial(), Some(&store), "openai").unwrap_err();
    assert!(
        matches!(error, ApplicationError::CodexManagedModelDiscovery(provider) if provider == "openai")
    );
    assert_eq!(store.get_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn unavailable_credential_backend_is_typed_before_http() {
    let registry = registry_with_provider(
        "deepseek",
        "https://example.invalid",
        ModelDiscoverySchema::DeepSeek,
    );
    assert!(matches!(
        discover_provider_models(&registry, None, "deepseek"),
        Err(ApplicationError::UnsupportedPlatform)
    ));
}

#[test]
fn credential_backend_failures_remain_typed() {
    let registry = registry_with_provider(
        "deepseek",
        "https://example.invalid",
        ModelDiscoverySchema::DeepSeek,
    );
    let store = FakeStore {
        fail_get: true,
        ..FakeStore::default()
    };
    assert!(matches!(
        discover_provider_models(&registry, Some(&store), "deepseek"),
        Err(ApplicationError::Credential(
            CredentialStoreError::Backend {
                operation: cps::credential::CredentialOperation::Retrieve
            }
        ))
    ));
}

#[test]
fn non_success_statuses_are_deterministic_and_never_include_response_bodies() {
    for status in [401, 403, 429, 500] {
        let server = serve_once(status, &[], FAKE_SECRET);
        let registry =
            registry_with_provider("deepseek", &server.url, ModelDiscoverySchema::DeepSeek);
        let store = FakeStore::with_credential("deepseek", FAKE_SECRET);
        let error = discover_provider_models(&registry, Some(&store), "deepseek").unwrap_err();
        let diagnostic = format!("{error:?} {error}");
        assert!(matches!(
            error,
            ApplicationError::ModelDiscovery(ModelDiscoveryError::HttpStatus { status: actual, .. })
                if actual == status
        ));
        assert!(!diagnostic.contains(FAKE_SECRET));
        server.request.join().unwrap();
    }
}

#[test]
fn redirects_are_not_followed_and_redirect_target_never_receives_credential() {
    let second = TcpListener::bind("127.0.0.1:0").unwrap();
    second.set_nonblocking(true).unwrap();
    let second_url = format!("http://{}", second.local_addr().unwrap());
    let server = serve_once(302, &[("Location", &second_url)], "redirect-body");
    let registry = registry_with_provider("deepseek", &server.url, ModelDiscoverySchema::DeepSeek);
    let store = FakeStore::with_credential("deepseek", FAKE_SECRET);
    let error = discover_provider_models(&registry, Some(&store), "deepseek").unwrap_err();
    assert!(matches!(
        error,
        ApplicationError::ModelDiscovery(ModelDiscoveryError::HttpStatus { status: 302, .. })
    ));
    assert!(
        server
            .request
            .join()
            .unwrap()
            .contains(&format!("Bearer {FAKE_SECRET}"))
    );
    assert!(
        matches!(second.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[test]
fn secret_is_absent_from_discovery_and_application_error_debug_and_display() {
    let server = serve_once(500, &[], FAKE_SECRET);
    let registry = registry_with_provider("deepseek", &server.url, ModelDiscoverySchema::DeepSeek);
    let store = FakeStore::with_credential("deepseek", FAKE_SECRET);
    let error = discover_provider_models(&registry, Some(&store), "deepseek").unwrap_err();
    let discovery_error = match error {
        ApplicationError::ModelDiscovery(error) => error,
        other => panic!("unexpected error: {other:?}"),
    };
    assert!(!format!("{discovery_error:?} {discovery_error}").contains(FAKE_SECRET));
    let application_error = ApplicationError::ModelDiscovery(discovery_error);
    assert!(!format!("{application_error:?} {application_error}").contains(FAKE_SECRET));
    server.request.join().unwrap();
}

#[test]
fn discovery_parsers_keep_provider_models_in_response_order() {
    let body = br#"{"data":[{"id":"z-last"},{"id":"a-first"}]}"#;
    let models = parse_models("openrouter", ModelDiscoverySchema::OpenRouter, body).unwrap();
    assert_eq!(
        models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["z-last", "a-first"]
    );
}

#[test]
fn active_provider_read_is_byte_preserving_and_creates_no_backup() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    let contents = b"# keep comments and spacing\nmodel = 'openai/gpt-5'\nmodel_provider = \"openrouter\"   # active\n";
    std::fs::write(&config_path, contents).unwrap();
    let before = std::fs::read(&config_path).unwrap();
    assert_eq!(
        config::read_active_provider(&config_path).unwrap(),
        "openrouter"
    );
    assert_eq!(std::fs::read(&config_path).unwrap(), before);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn active_provider_read_reports_missing_invalid_and_empty_values() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    for (contents, expected) in [
        ("model = 'gpt-5'\n", 0),
        ("model_provider = 42\n", 1),
        ("model_provider = ''\n", 2),
    ] {
        std::fs::write(&config_path, contents).unwrap();
        let error = config::read_active_provider(&config_path).unwrap_err();
        assert!(matches!(
            (expected, error),
            (0, ConfigError::MissingActiveProvider)
                | (1, ConfigError::InvalidActiveProvider)
                | (2, ConfigError::EmptyActiveProvider)
        ));
    }
}

#[test]
fn explicit_models_provider_does_not_read_config_and_default_uses_active_provider() {
    let directory = tempdir().unwrap();
    let missing_config = directory.path().join("does-not-exist.toml");
    assert_eq!(
        resolve_models_provider(Some("openrouter"), || {
            config::read_active_provider(&missing_config).map_err(ApplicationError::Config)
        })
        .unwrap(),
        "openrouter"
    );
    let config_path = directory.path().join("config.toml");
    std::fs::write(&config_path, "model_provider = 'xai'\n").unwrap();
    let before = std::fs::read(&config_path).unwrap();
    assert_eq!(
        resolve_models_provider(None, || {
            config::read_active_provider(&config_path).map_err(ApplicationError::Config)
        })
        .unwrap(),
        "xai"
    );
    assert_eq!(std::fs::read(&config_path).unwrap(), before);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn explicit_provider_discovery_contacts_only_that_registry_provider() {
    let (unused_url, unused_listener) = unused_listener();
    let response_server = serve_once(200, &[], r#"{"data":[{"id":"openai/gpt-5"}]}"#);
    let mut registry =
        registry_with_provider("deepseek", &unused_url, ModelDiscoverySchema::DeepSeek);
    let openrouter = registry_with_provider(
        "openrouter",
        &response_server.url,
        ModelDiscoverySchema::OpenRouter,
    );
    for provider in openrouter.iter().cloned() {
        registry.register(provider).unwrap();
    }
    let store = FakeStore::with_credential("openrouter", FAKE_SECRET);
    let selected = resolve_models_provider(Some("openrouter"), || {
        panic!("explicit provider must not read the active config")
    })
    .unwrap();
    let models = discover_provider_models(&registry, Some(&store), &selected).unwrap();
    assert_eq!(models, vec![DiscoveredModel::new("openai/gpt-5")]);
    assert_eq!(store.get_calls.load(Ordering::SeqCst), 1);
    assert!(
        response_server
            .request
            .join()
            .unwrap()
            .starts_with("GET /models HTTP/1.1\r\n")
    );
    assert!(
        matches!(unused_listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[test]
fn unknown_active_provider_uses_existing_registry_error_path() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    std::fs::write(&config_path, "model_provider = 'missing'\n").unwrap();
    let provider = resolve_models_provider(None, || {
        config::read_active_provider(&config_path).map_err(ApplicationError::Config)
    })
    .unwrap();
    assert!(matches!(
        discover_provider_models(&ProviderRegistry::initial(), None, &provider),
        Err(ApplicationError::Registry(cps::provider::RegistryError::UnknownProvider(id)))
            if id.as_str() == "missing"
    ));
}

#[test]
fn list_inventory_has_stable_order_and_explicit_lowercase_values() {
    let rows = format_provider_inventory(&ProviderRegistry::initial());
    assert_eq!(rows[0], "ID\tNAME\tTRANSPORT\tCOMPATIBILITY\tDISCOVERY");
    let ids = rows
        .iter()
        .skip(1)
        .map(|row| row.split('\t').next().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "deepseek",
            "lmstudio",
            "ollama",
            "openai",
            "openrouter",
            "xai"
        ]
    );
    assert!(
        rows.iter()
            .any(|row| row == "deepseek\tDeepSeek\tresponses\tverified-basic\tprovider-models")
    );
    assert!(
        rows.iter()
            .any(|row| row == "openai\tOpenAI\tnative\tverified\tcodex-managed")
    );
}

#[test]
fn provider_registry_selects_independent_discovery_schemas() {
    let registry = ProviderRegistry::initial();
    for (provider, expected) in [
        ("deepseek", ModelDiscoverySchema::DeepSeek),
        ("xai", ModelDiscoverySchema::Xai),
        ("openrouter", ModelDiscoverySchema::OpenRouter),
    ] {
        assert!(matches!(
            registry.lookup(provider).unwrap().model_discovery,
            ModelDiscoveryStrategy::ProviderModelsEndpoint(ProviderModelsEndpoint { schema, .. })
                if schema == expected
        ));
    }
}

#[test]
fn discovery_strategy_requires_schema_specific_list_metadata_where_documented() {
    assert!(matches!(
        parse_models("xai", ModelDiscoverySchema::Xai, br#"{"data":[]}"#),
        Err(ModelDiscoveryError::InvalidResponse {
            reason: InvalidModelResponse::InvalidListObject,
            ..
        })
    ));
    assert!(
        parse_models(
            "openrouter",
            ModelDiscoverySchema::OpenRouter,
            br#"{"data":[]}"#
        )
        .is_ok()
    );
}

#[test]
fn endpoint_rejects_invalid_url_without_exposing_other_data() {
    assert!(matches!(
        endpoint_url("deepseek", "not a URL", "/models"),
        Err(ModelDiscoveryError::InvalidEndpoint { provider_id }) if provider_id == "deepseek"
    ));
    assert!(endpoint_url("xai", "https://api.x.ai/v1/", "/models").is_ok());
}

#[test]
fn local_http_response_order_empty_lists_and_exact_url_suffix_are_real_requests() {
    let schema = ModelDiscoverySchema::OpenRouter;
    let server = serve_once(
        200,
        &[],
        r#"{"data":[{"id":"openai/gpt-5"},{"id":"anthropic/claude"}]}"#,
    );
    let registry = registry_with_provider("openrouter", &server.url, schema);
    let store = FakeStore::with_credential("openrouter", FAKE_SECRET);
    let models = discover_provider_models(&registry, Some(&store), "openrouter").unwrap();
    assert_eq!(models[0].id, "openai/gpt-5");
    assert_eq!(models[1].id, "anthropic/claude");
    assert!(
        server
            .request
            .join()
            .unwrap()
            .starts_with("GET /models HTTP/1.1\r\n")
    );
}

#[test]
fn discovery_missing_store_reports_unsupported_platform_concept_without_request() {
    let error = discover_provider_models(
        &registry_with_provider(
            "xai",
            "https://example.invalid/v1",
            ModelDiscoverySchema::Xai,
        ),
        None,
        "xai",
    )
    .unwrap_err();
    assert!(matches!(error, ApplicationError::UnsupportedPlatform));
}

#[test]
fn active_provider_read_reports_config_missing_error() {
    let directory = tempdir().unwrap();
    assert!(matches!(
        resolve_models_provider(None, || {
            config::read_active_provider(directory.path().join("missing.toml"))
                .map_err(ApplicationError::Config)
        }),
        Err(ApplicationError::Config(ConfigError::Missing { .. }))
    ));
}
