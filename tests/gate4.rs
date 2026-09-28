use std::{collections::HashMap, ffi::OsStr, fs, path::Path, sync::Mutex};

use cps::{
    application::{
        ApplicationError, resolve_user_config_path_from, store_provider_credential,
        use_provider_model,
    },
    auth_command::{SECURITY_EXECUTABLE, project_keychain_auth_command},
    config::{ConfigError, ConfigPathError},
    credential::{CredentialStore, CredentialStoreError, SecretValue},
    domain::ProviderModelTarget,
    provider::{CredentialSlotId, ProviderRegistry},
};
use tempfile::tempdir;
use toml_edit::DocumentMut;

const FIXTURE: &str = include_str!("fixtures/codex-config.toml");

#[derive(Default)]
struct FakeStore {
    entries: Mutex<HashMap<CredentialSlotId, String>>,
    fail_retrieve: bool,
}

impl FakeStore {
    fn with_credential(slot: &str, value: &str) -> Self {
        let store = Self::default();
        store
            .set(
                &CredentialSlotId::new(slot).unwrap(),
                &SecretValue::new(value).unwrap(),
            )
            .unwrap();
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
            .insert(slot.clone(), value.expose_secret().to_owned());
        Ok(())
    }

    fn get(&self, slot: &CredentialSlotId) -> Result<Option<SecretValue>, CredentialStoreError> {
        if self.fail_retrieve {
            return Err(CredentialStoreError::Backend {
                operation: cps::credential::CredentialOperation::Retrieve,
            });
        }
        self.entries
            .lock()
            .unwrap()
            .get(slot)
            .cloned()
            .map(|value| {
                SecretValue::new(value).map_err(|_| CredentialStoreError::InvalidStoredValue)
            })
            .transpose()
    }

    fn delete(&self, slot: &CredentialSlotId) -> Result<(), CredentialStoreError> {
        self.entries.lock().unwrap().remove(slot);
        Ok(())
    }
}

fn write_config(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
}

fn has_backup(directory: &Path) -> bool {
    fs::read_dir(directory).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".cps-backup-")
    })
}

fn run_direct_use(
    registry: &ProviderRegistry,
    store: &dyn CredentialStore,
    config: &Path,
    target: &str,
) -> Result<cps::config::TransactionResult, ApplicationError> {
    let target = target.parse::<ProviderModelTarget>().unwrap();
    use_provider_model(registry, Some(store), config, &target)
}

#[test]
fn auth_application_stores_direct_provider_secret_in_the_registered_slot() {
    let registry = ProviderRegistry::initial();
    let store = FakeStore::default();

    store_provider_credential(&registry, &store, "deepseek", "private-value").unwrap();

    assert_eq!(
        store
            .get(&CredentialSlotId::new("deepseek").unwrap())
            .unwrap()
            .unwrap()
            .expose_secret(),
        "private-value"
    );
}

#[test]
fn native_provider_rejects_cps_credential_storage() {
    let error = store_provider_credential(
        &ProviderRegistry::initial(),
        &FakeStore::default(),
        "openai",
        "private-value",
    )
    .unwrap_err();

    assert!(matches!(error, ApplicationError::CredentialNotApplicable(id) if id == "openai"));
}

#[test]
fn unknown_provider_auth_fails_with_the_registry_error() {
    let error = store_provider_credential(
        &ProviderRegistry::initial(),
        &FakeStore::default(),
        "unknown",
        "private-value",
    )
    .unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::Registry(cps::provider::RegistryError::UnknownProvider(id))
            if id.as_str() == "unknown"
    ));
}

#[test]
fn missing_direct_credential_leaves_config_and_backup_directory_untouched() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    write_config(&config, FIXTURE);
    let original = fs::read(&config).unwrap();

    let error = run_direct_use(
        &ProviderRegistry::initial(),
        &FakeStore::default(),
        &config,
        "deepseek/deepseek-chat",
    )
    .unwrap_err();

    assert!(matches!(error, ApplicationError::MissingProviderCredential(id) if id == "deepseek"));
    assert_eq!(fs::read(&config).unwrap(), original);
    assert!(!has_backup(directory.path()));
}

#[test]
fn direct_use_provisions_auth_command_and_selection_in_one_config_transaction() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    write_config(&config, FIXTURE);
    let store = FakeStore::with_credential("deepseek", "never-write-this-secret");

    run_direct_use(
        &ProviderRegistry::initial(),
        &store,
        &config,
        "deepseek/deepseek-chat",
    )
    .unwrap();

    let updated = fs::read_to_string(&config).unwrap();
    let document = updated.parse::<DocumentMut>().unwrap();
    assert_eq!(document["model"].as_str(), Some("deepseek-chat"));
    assert_eq!(document["model_provider"].as_str(), Some("deepseek"));
    let provider = &document["model_providers"]["deepseek"];
    assert_eq!(provider["name"].as_str(), Some("DeepSeek"));
    assert_eq!(
        provider["base_url"].as_str(),
        Some("https://api.deepseek.com")
    );
    assert_eq!(provider["wire_api"].as_str(), Some("responses"));
    assert_eq!(
        provider["auth"]["command"].as_str(),
        Some(SECURITY_EXECUTABLE)
    );
    assert_eq!(
        provider["auth"]["refresh_interval_ms"].as_integer(),
        Some(0)
    );
    assert!(!updated.contains("never-write-this-secret"));
}

#[test]
fn consecutive_direct_switches_keep_unique_backups_of_each_pre_switch_config() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    write_config(&config, FIXTURE);
    let original_bytes = fs::read(&config).unwrap();
    let registry = ProviderRegistry::initial();
    let store = FakeStore::default();
    store
        .set(
            &CredentialSlotId::new("deepseek").unwrap(),
            &SecretValue::new("deepseek-test-secret").unwrap(),
        )
        .unwrap();
    store
        .set(
            &CredentialSlotId::new("openrouter").unwrap(),
            &SecretValue::new("openrouter-test-secret").unwrap(),
        )
        .unwrap();

    let first = run_direct_use(&registry, &store, &config, "deepseek/deepseek-chat").unwrap();
    let after_first_bytes = fs::read(&config).unwrap();
    let second = run_direct_use(&registry, &store, &config, "openrouter/openai/gpt-5").unwrap();

    assert!(first.backup_path.exists());
    assert!(second.backup_path.exists());
    assert_ne!(first.backup_path, second.backup_path);
    assert_eq!(fs::read(&first.backup_path).unwrap(), original_bytes);
    assert_eq!(fs::read(&second.backup_path).unwrap(), after_first_bytes);

    let final_bytes = fs::read(&config).unwrap();
    let final_text = String::from_utf8(final_bytes).unwrap();
    let final_document = final_text.parse::<DocumentMut>().unwrap();
    assert_eq!(final_document["model"].as_str(), Some("openai/gpt-5"));
    assert_eq!(
        final_document["model_provider"].as_str(),
        Some("openrouter")
    );
    for (provider, display_name, base_url) in [
        ("deepseek", "DeepSeek", "https://api.deepseek.com"),
        ("openrouter", "OpenRouter", "https://openrouter.ai/api/v1"),
    ] {
        let definition = &final_document["model_providers"][provider];
        assert_eq!(definition["name"].as_str(), Some(display_name));
        assert_eq!(definition["base_url"].as_str(), Some(base_url));
        assert_eq!(definition["wire_api"].as_str(), Some("responses"));
        assert_eq!(
            definition["auth"]["command"].as_str(),
            Some(SECURITY_EXECUTABLE)
        );
        assert_eq!(
            definition["auth"]["refresh_interval_ms"].as_integer(),
            Some(0)
        );
    }
    for preserved in [
        "# Keep this comment and unknown configuration intact.",
        "custom_key = \"unchanged\"",
        "[model_providers.openai]",
        "[mcp_servers.local]",
        "[projects.\"/tmp/demo\"]",
        "[features]",
        "[sandbox_workspace_write]",
    ] {
        assert!(
            final_text.contains(preserved),
            "missing preserved content: {preserved}"
        );
    }
    assert!(!final_text.contains("deepseek-test-secret"));
    assert!(!final_text.contains("openrouter-test-secret"));
}

#[test]
fn projected_auth_command_contains_service_and_slot_but_no_secret() {
    let command = project_keychain_auth_command(&CredentialSlotId::new("xai").unwrap());
    let serialized = format!(
        "{} {:?} {}",
        command.command, command.args, command.refresh_interval_ms
    );

    assert_eq!(command.command, "/usr/bin/security");
    assert!(serialized.contains("find-generic-password"));
    assert!(serialized.contains("codex-provider-switcher"));
    assert!(serialized.contains("xai"));
    assert!(!serialized.contains("secret"));
}

#[test]
fn direct_provider_config_uses_responses_wire_api_and_expected_keychain_args() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    write_config(&config, FIXTURE);

    run_direct_use(
        &ProviderRegistry::initial(),
        &FakeStore::with_credential("openrouter", "test-secret"),
        &config,
        "openrouter/openai/gpt-5",
    )
    .unwrap();

    let document = fs::read_to_string(&config)
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap();
    let provider = &document["model_providers"]["openrouter"];
    assert_eq!(provider["wire_api"].as_str(), Some("responses"));
    let args = provider["auth"]["args"].as_array().unwrap();
    assert_eq!(
        args.get(0).and_then(toml_edit::Value::as_str),
        Some("find-generic-password")
    );
    assert_eq!(args.get(1).and_then(toml_edit::Value::as_str), Some("-s"));
    assert_eq!(
        args.get(2).and_then(toml_edit::Value::as_str),
        Some("codex-provider-switcher")
    );
    assert_eq!(
        args.get(4).and_then(toml_edit::Value::as_str),
        Some("openrouter")
    );
    assert!(!fs::read_to_string(&config).unwrap().contains("test-secret"));
}

#[test]
fn nested_provider_model_id_is_preserved_after_first_slash() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    write_config(&config, FIXTURE);

    run_direct_use(
        &ProviderRegistry::initial(),
        &FakeStore::with_credential("openrouter", "private"),
        &config,
        "openrouter/openai/gpt-5",
    )
    .unwrap();

    let document = fs::read_to_string(&config)
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap();
    assert_eq!(document["model"].as_str(), Some("openai/gpt-5"));
    assert_eq!(document["model_provider"].as_str(), Some("openrouter"));
}

#[test]
fn native_switch_updates_only_active_selection_without_custom_table() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    write_config(&config, FIXTURE);
    let before = fs::read_to_string(&config).unwrap();

    let target = "ollama/qwen3".parse::<ProviderModelTarget>().unwrap();
    use_provider_model(&ProviderRegistry::initial(), None, &config, &target).unwrap();

    let after = fs::read_to_string(&config).unwrap();
    let document = after.parse::<DocumentMut>().unwrap();
    assert_eq!(document["model"].as_str(), Some("qwen3"));
    assert_eq!(document["model_provider"].as_str(), Some("ollama"));
    assert_eq!(
        document["model_providers"]["openai"].to_string(),
        before.parse::<DocumentMut>().unwrap()["model_providers"]["openai"].to_string()
    );
    assert!(!after.contains("[model_providers.ollama]"));
}

#[test]
fn equivalent_existing_provider_definition_is_accepted_without_normalizing_it() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    let existing = "model = \"old\"\nmodel_provider = \"openai\"\n\n# Keep this hand-formatting.\n[model_providers.deepseek]\nname =  \"DeepSeek\"\nbase_url = \"https://api.deepseek.com\" # url note\nwire_api = \"responses\"\n\n[model_providers.deepseek.auth]\ncommand = \"/usr/bin/security\"\nargs = [\"find-generic-password\", \"-s\", \"codex-provider-switcher\", \"-a\", \"deepseek\", \"-w\"]\nrefresh_interval_ms = 0\n";
    write_config(&config, existing);

    run_direct_use(
        &ProviderRegistry::initial(),
        &FakeStore::with_credential("deepseek", "private"),
        &config,
        "deepseek/deepseek-chat",
    )
    .unwrap();

    let after = fs::read_to_string(&config).unwrap();
    assert!(after.contains("# Keep this hand-formatting."));
    assert!(after.contains("name =  \"DeepSeek\""));
    assert!(after.contains("# url note"));
    assert_eq!(after.matches("[model_providers.deepseek]").count(), 1);
}

#[test]
fn differing_provider_definitions_are_rejected_byte_for_byte_without_backup() {
    for (base_url, provider_extra, auth_command, auth_extra) in [
        ("https://other.example/v1", "", "/usr/bin/security", ""),
        (
            "https://api.deepseek.com",
            "",
            "/usr/bin/security",
            "env_key = \"DEEPSEEK_API_KEY\"\n",
        ),
        ("https://api.deepseek.com", "", "/bin/other", ""),
        (
            "https://api.deepseek.com",
            "custom_setting = true\n",
            "/usr/bin/security",
            "",
        ),
    ] {
        let directory = tempdir().unwrap();
        let config = directory.path().join("config.toml");
        let original = format!(
            "model = \"old\"\nmodel_provider = \"openai\"\n\n[model_providers.deepseek]\nname = \"DeepSeek\"\nbase_url = \"{base_url}\"\nwire_api = \"responses\"\n{provider_extra}\n\n[model_providers.deepseek.auth]\ncommand = \"{auth_command}\"\nargs = [\"find-generic-password\", \"-s\", \"codex-provider-switcher\", \"-a\", \"deepseek\", \"-w\"]\nrefresh_interval_ms = 0\n{auth_extra}"
        );
        write_config(&config, &original);

        let error = run_direct_use(
            &ProviderRegistry::initial(),
            &FakeStore::with_credential("deepseek", "private"),
            &config,
            "deepseek/deepseek-chat",
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ApplicationError::Config(ConfigError::ProviderDefinitionConflict { ref provider_id })
                if provider_id == "deepseek"
        ));
        assert_eq!(fs::read_to_string(&config).unwrap(), original);
        assert!(!has_backup(directory.path()));
    }
}

#[test]
fn unrelated_tables_and_unknown_config_survive_direct_provisioning() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    write_config(&config, FIXTURE);

    run_direct_use(
        &ProviderRegistry::initial(),
        &FakeStore::with_credential("deepseek", "private"),
        &config,
        "deepseek/deepseek-chat",
    )
    .unwrap();

    let after = fs::read_to_string(&config).unwrap();
    for preserved in [
        "# Keep this comment and unknown configuration intact.",
        "custom_key = \"unchanged\"",
        "[model_providers.openai]",
        "[mcp_servers.local]",
        "[projects.\"/tmp/demo\"]",
        "[features]",
        "[sandbox_workspace_write]",
    ] {
        assert!(
            after.contains(preserved),
            "missing preserved content: {preserved}"
        );
    }
}

#[test]
fn credential_backend_failure_does_not_touch_config_or_create_backup() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    write_config(&config, FIXTURE);
    let before = fs::read(&config).unwrap();
    let store = FakeStore {
        fail_retrieve: true,
        ..FakeStore::default()
    };

    let error = run_direct_use(
        &ProviderRegistry::initial(),
        &store,
        &config,
        "deepseek/deepseek-chat",
    )
    .unwrap_err();

    assert!(matches!(error, ApplicationError::Credential(_)));
    assert_eq!(fs::read(&config).unwrap(), before);
    assert!(!has_backup(directory.path()));
}

#[test]
fn missing_user_config_is_typed_and_does_not_create_directories() {
    let directory = tempdir().unwrap();
    let missing_parent = directory.path().join("absent").join(".codex");
    let config = missing_parent.join("config.toml");
    let store = FakeStore::with_credential("deepseek", "private");

    let error = run_direct_use(
        &ProviderRegistry::initial(),
        &store,
        &config,
        "deepseek/deepseek-chat",
    )
    .unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::Config(ConfigError::Missing { .. })
    ));
    assert!(!missing_parent.exists());
}

#[test]
fn invalid_toml_fails_before_mutation_or_backup() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    let original = "model = [\n";
    write_config(&config, original);

    let target = "openai/gpt-5".parse::<ProviderModelTarget>().unwrap();
    let error =
        use_provider_model(&ProviderRegistry::initial(), None, &config, &target).unwrap_err();

    assert!(matches!(
        error,
        ApplicationError::Config(ConfigError::Toml(_))
    ));
    assert_eq!(fs::read_to_string(&config).unwrap(), original);
    assert!(!has_backup(directory.path()));
}

#[test]
fn direct_use_without_a_credential_backend_fails_without_touching_config() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    write_config(&config, FIXTURE);
    let original = fs::read(&config).unwrap();
    let target = "deepseek/deepseek-chat"
        .parse::<ProviderModelTarget>()
        .unwrap();

    let error =
        use_provider_model(&ProviderRegistry::initial(), None, &config, &target).unwrap_err();

    assert!(matches!(error, ApplicationError::UnsupportedPlatform));
    assert_eq!(fs::read(&config).unwrap(), original);
    assert!(!has_backup(directory.path()));
}

#[test]
fn config_path_prefers_explicit_codex_home_and_defaults_to_user_codex_directory() {
    assert_eq!(
        resolve_user_config_path_from(
            Some(OsStr::new("/custom/codex")),
            Some(OsStr::new("/home/user"))
        )
        .unwrap(),
        Path::new("/custom/codex/config.toml")
    );
    assert_eq!(
        resolve_user_config_path_from(None, Some(OsStr::new("/home/user"))).unwrap(),
        Path::new("/home/user/.codex/config.toml")
    );
}

#[test]
fn config_path_rejects_empty_codex_home_and_missing_home() {
    assert_eq!(
        resolve_user_config_path_from(Some(OsStr::new("")), Some(OsStr::new("/home/user"))),
        Err(ConfigPathError::EmptyCodexHome)
    );
    assert_eq!(
        resolve_user_config_path_from(None, None),
        Err(ConfigPathError::HomeUnavailable)
    );
}

#[test]
fn failed_concurrent_config_change_is_detected_without_backup() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    write_config(&config, FIXTURE);
    let selection = cps::config::ActiveSelection::new("gpt-5", "openai").unwrap();
    let external = b"model = \"external\"\nmodel_provider = \"external\"\n";

    let error =
        cps::config::test_support::transaction_with_before_commit(&config, &selection, || {
            fs::write(&config, external).unwrap()
        })
        .unwrap_err();

    assert!(matches!(error, ConfigError::Conflict { .. }));
    assert_eq!(fs::read(&config).unwrap(), external);
    assert!(!has_backup(directory.path()));
}
