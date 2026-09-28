use std::{fs, path::Path};

use cps::config::{ActiveSelection, ConfigError, update_active_selection};
use tempfile::tempdir;
use toml_edit::DocumentMut;

const FIXTURE: &str = include_str!("fixtures/codex-config.toml");

fn write_fixture(path: &Path) {
    fs::write(path, FIXTURE).unwrap();
}

fn selection() -> ActiveSelection {
    ActiveSelection::new("gpt-5", "custom").unwrap()
}

#[test]
fn transaction_changes_only_active_selection_and_preserves_formatting() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    write_fixture(&config);

    let result = update_active_selection(&config, &selection()).unwrap();
    let updated = fs::read_to_string(&config).unwrap();

    assert!(updated.contains("# Keep this comment and unknown configuration intact."));
    assert!(updated.contains("model = \"gpt-5\""));
    assert!(updated.contains("model_provider = \"custom\""));
    assert!(updated.contains("custom_key = \"unchanged\""));
    assert!(updated.contains("[model_providers.openai]"));
    assert!(updated.contains("base_url = \"https://api.openai.com/v1\""));
    assert!(updated.contains("[mcp_servers.local]"));
    assert!(updated.contains("[projects.\"/tmp/demo\"]"));
    assert!(updated.contains("[features]"));
    assert!(updated.contains("[sandbox_workspace_write]"));
    assert_eq!(fs::read_to_string(result.backup_path).unwrap(), FIXTURE);
}

#[test]
fn conflict_leaves_external_change_intact_and_creates_no_backup() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    write_fixture(&config);

    let original = fs::read(&config).unwrap();
    let external_change = b"model = \"external\"\nmodel_provider = \"other\"\n";
    let conflict =
        cps::config::test_support::transaction_with_before_commit(&config, &selection(), || {
            fs::write(&config, external_change).unwrap()
        })
        .unwrap_err();

    assert!(matches!(conflict, ConfigError::Conflict { .. }));
    assert_eq!(fs::read(&config).unwrap(), external_change);
    assert!(fs::read_dir(directory.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".cps-backup-")
    }));
    assert_ne!(original, external_change);
}

#[test]
fn successful_transaction_is_valid_toml_and_has_complete_output() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    write_fixture(&config);

    let result = update_active_selection(&config, &selection()).unwrap();
    let updated = fs::read_to_string(&config).unwrap();
    let document = updated.parse::<DocumentMut>().unwrap();

    assert_eq!(document["model"].as_str(), Some("gpt-5"));
    assert_eq!(document["model_provider"].as_str(), Some("custom"));
    assert_eq!(fs::read_to_string(result.backup_path).unwrap(), FIXTURE);
}

#[test]
fn consecutive_transactions_create_distinct_recovery_backups() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("config.toml");
    write_fixture(&config);

    let first = update_active_selection(&config, &ActiveSelection::new("gpt-A", "openai").unwrap())
        .unwrap();
    let after_first = fs::read_to_string(&config).unwrap();
    let second = update_active_selection(
        &config,
        &ActiveSelection::new("model-B", "deepseek").unwrap(),
    )
    .unwrap();
    let after_second = fs::read_to_string(&config).unwrap();
    let third =
        update_active_selection(&config, &ActiveSelection::new("model-C", "xai").unwrap()).unwrap();

    assert_eq!(
        fs::read_to_string(&config)
            .unwrap()
            .parse::<DocumentMut>()
            .unwrap()["model"]
            .as_str(),
        Some("model-C")
    );
    let final_document = fs::read_to_string(&config)
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap();
    assert_eq!(final_document["model_provider"].as_str(), Some("xai"));
    assert_eq!(fs::read_to_string(&first.backup_path).unwrap(), FIXTURE);
    assert_eq!(
        fs::read_to_string(&second.backup_path).unwrap(),
        after_first
    );
    assert_eq!(
        fs::read_to_string(&third.backup_path).unwrap(),
        after_second
    );
    assert_ne!(first.backup_path, second.backup_path);
    assert_ne!(second.backup_path, third.backup_path);
    assert_ne!(first.backup_path, third.backup_path);
    assert!(first.backup_path.exists());
    assert!(second.backup_path.exists());
    assert!(third.backup_path.exists());
    assert!(
        fs::read_to_string(&config)
            .unwrap()
            .contains("# Keep this comment and unknown configuration intact.")
    );
    assert!(
        fs::read_to_string(&config)
            .unwrap()
            .contains("[model_providers.openai]")
    );
}
