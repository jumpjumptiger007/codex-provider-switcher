use std::{
    fs::{self, File, FileTimes},
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

use clap::Parser;
use cps::{
    application,
    cli::{self, Cli, Command},
    config::{self, ConfigError},
    provider::ProviderRegistry,
};
use tempfile::tempdir;

fn write_config(path: &Path, contents: &[u8]) {
    fs::write(path, contents).unwrap();
}

fn backup_path(directory: &Path, suffix: &str) -> PathBuf {
    directory.join(format!("config.toml.cps-backup-{suffix}"))
}

fn set_modified(path: &Path, seconds_since_epoch: u64) {
    File::open(path)
        .unwrap()
        .set_times(
            FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds_since_epoch)),
        )
        .unwrap();
}

fn count_recovery_backups(directory: &Path) -> usize {
    fs::read_dir(directory)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("config.toml.cps-backup-")
                && entry.file_type().unwrap().is_file()
        })
        .count()
}

#[test]
fn status_reads_native_selection_and_formats_registry_transport() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    let original = b"# keep this exactly\nmodel = \"gpt-5\"\nmodel_provider = \"openai\"\n";
    write_config(&config_path, original);

    let status = application::status(&ProviderRegistry::initial(), &config_path).unwrap();

    assert_eq!(status.selection.model, "gpt-5");
    assert_eq!(status.selection.model_provider, "openai");
    assert_eq!(
        status.known_provider,
        Some(cps::domain::ProviderTransport::Native)
    );
    assert_eq!(
        cli::format_status(&status),
        "provider: openai\nmodel: gpt-5\ntarget: openai/gpt-5\nknown_provider: yes\ntransport: native"
    );
    assert_eq!(fs::read(&config_path).unwrap(), original);
    assert_eq!(count_recovery_backups(directory.path()), 0);
}

#[test]
fn status_preserves_nested_model_id_and_reports_responses_transport() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    write_config(
        &config_path,
        b"model = \"openai/gpt-5\"\nmodel_provider = \"openrouter\"\n",
    );

    let status = application::status(&ProviderRegistry::initial(), &config_path).unwrap();

    assert_eq!(status.selection.model, "openai/gpt-5");
    assert_eq!(
        cli::format_status(&status),
        "provider: openrouter\nmodel: openai/gpt-5\ntarget: openrouter/openai/gpt-5\nknown_provider: yes\ntransport: responses"
    );
}

#[test]
fn status_accepts_unknown_user_provider_without_inference_or_rejection() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    write_config(
        &config_path,
        b"model = \"foo/bar\"\nmodel_provider = \"custom-local\"\n",
    );

    let status = application::status(&ProviderRegistry::initial(), &config_path).unwrap();

    assert_eq!(status.selection.model, "foo/bar");
    assert_eq!(status.selection.model_provider, "custom-local");
    assert_eq!(status.known_provider, None);
    assert_eq!(
        cli::format_status(&status),
        "provider: custom-local\nmodel: foo/bar\ntarget: custom-local/foo/bar\nknown_provider: no"
    );
}

#[test]
fn status_preserves_nonblank_model_and_provider_strings_without_trimming() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    write_config(
        &config_path,
        b"model = \" openai/gpt-5 \"\nmodel_provider = \" custom-local \"\n",
    );

    let status = application::status(&ProviderRegistry::initial(), &config_path).unwrap();

    assert_eq!(status.selection.model, " openai/gpt-5 ");
    assert_eq!(status.selection.model_provider, " custom-local ");
    assert_eq!(status.known_provider, None);
}

#[test]
fn status_is_read_only_and_does_not_accept_or_query_credentials() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    let original = b"model = \"gpt-5\"\nmodel_provider = \"openai\"\n";
    write_config(&config_path, original);
    let files_before = fs::read_dir(directory.path()).unwrap().count();

    let _ = application::status(&ProviderRegistry::initial(), &config_path).unwrap();

    assert_eq!(fs::read(&config_path).unwrap(), original);
    assert_eq!(
        fs::read_dir(directory.path()).unwrap().count(),
        files_before
    );
    assert_eq!(count_recovery_backups(directory.path()), 0);
}

#[test]
fn status_reports_missing_model_as_typed_error() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("config.toml");
    write_config(&path, b"model_provider = \"openai\"\n");
    assert!(matches!(
        config::read_active_selection(&path),
        Err(ConfigError::MissingActiveModel)
    ));
}

#[test]
fn status_reports_non_string_model_as_typed_error() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("config.toml");
    write_config(&path, b"model = 42\nmodel_provider = \"openai\"\n");
    assert!(matches!(
        config::read_active_selection(&path),
        Err(ConfigError::InvalidActiveModel)
    ));
}

#[test]
fn status_reports_blank_model_as_typed_error() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("config.toml");
    write_config(&path, b"model = \"  \"\nmodel_provider = \"openai\"\n");
    assert!(matches!(
        config::read_active_selection(&path),
        Err(ConfigError::EmptyActiveModel)
    ));
}

#[test]
fn status_defaults_missing_model_provider_to_openai() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("config.toml");
    write_config(&path, b"model = \"gpt-6-luna\"\n");

    let selection = config::read_active_selection(&path).unwrap();

    assert_eq!(selection.model, "gpt-6-luna");
    assert_eq!(selection.model_provider, "openai");
}

#[test]
fn status_preserves_an_explicit_valid_model_provider() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("config.toml");
    write_config(
        &path,
        b"model = \"gpt-6-luna\"\nmodel_provider = \"custom-local\"\n",
    );

    let selection = config::read_active_selection(&path).unwrap();

    assert_eq!(selection.model, "gpt-6-luna");
    assert_eq!(selection.model_provider, "custom-local");
}

#[test]
fn status_reports_non_string_model_provider_as_typed_error() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("config.toml");
    write_config(&path, b"model = \"gpt-6-luna\"\nmodel_provider = 123\n");
    assert!(matches!(
        config::read_active_selection(&path),
        Err(ConfigError::InvalidActiveProvider)
    ));
}

#[test]
fn status_reports_blank_model_provider_as_typed_error() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("config.toml");
    for provider in ["", " "] {
        write_config(
            &path,
            format!("model = \"gpt-6-luna\"\nmodel_provider = \"{provider}\"\n").as_bytes(),
        );
        assert!(matches!(
            config::read_active_selection(&path),
            Err(ConfigError::EmptyActiveProvider)
        ));
    }
}

#[test]
fn status_reports_invalid_config_utf8_and_toml_as_typed_errors() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("config.toml");
    write_config(&path, &[0xff, 0xfe]);
    assert!(matches!(
        config::read_active_selection(&path),
        Err(ConfigError::InvalidConfigUtf8 { .. })
    ));
    write_config(&path, b"model = [\n");
    assert!(matches!(
        config::read_active_selection(&path),
        Err(ConfigError::InvalidConfigToml { .. })
    ));
}

#[test]
fn status_reports_missing_config_as_typed_error() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("missing.toml");
    assert!(matches!(
        config::read_active_selection(&path),
        Err(ConfigError::Missing { .. })
    ));
}

#[test]
fn backup_discovery_ignores_unrelated_entries_directories_and_symlinks() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    write_config(&config_path, b"model = \"current\"\n");
    fs::write(directory.path().join("unrelated.cps-backup-x"), b"ignored").unwrap();
    fs::write(
        directory.path().join("config.toml.old-cps-backup-x"),
        b"ignored",
    )
    .unwrap();
    fs::create_dir(backup_path(directory.path(), "directory")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let target = directory.path().join("symlink-target");
        fs::write(&target, b"ignored").unwrap();
        symlink(target, backup_path(directory.path(), "symlink")).unwrap();
    }

    assert!(matches!(
        config::latest_recovery_backup(&config_path),
        Err(ConfigError::NoRecoveryBackup { .. })
    ));
}

#[test]
fn no_recovery_backup_returns_typed_error() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    write_config(&config_path, b"model = \"current\"\n");
    assert!(matches!(
        config::restore_config(&config_path),
        Err(ConfigError::NoRecoveryBackup { .. })
    ));
}

#[test]
fn newest_recovery_backup_is_selected_by_modification_time() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    write_config(&config_path, b"model = \"current\"\n");
    let older = backup_path(directory.path(), "older");
    let newer = backup_path(directory.path(), "newer");
    fs::write(&older, b"model = \"older\"\n").unwrap();
    fs::write(&newer, b"model = \"newer\"\n").unwrap();
    set_modified(&older, 1_600_000_000);
    set_modified(&newer, 1_700_000_000);

    assert_eq!(config::latest_recovery_backup(&config_path).unwrap(), newer);
}

#[test]
fn equal_newest_recovery_timestamps_fail_as_ambiguous() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    write_config(&config_path, b"model = \"current\"\n");
    let first = backup_path(directory.path(), "first");
    let second = backup_path(directory.path(), "second");
    fs::write(&first, b"model = \"first\"\n").unwrap();
    fs::write(&second, b"model = \"second\"\n").unwrap();
    set_modified(&first, 1_700_000_000);
    set_modified(&second, 1_700_000_000);

    assert!(matches!(
        config::latest_recovery_backup(&config_path),
        Err(ConfigError::AmbiguousLatestBackup { .. })
    ));
}

#[test]
fn invalid_utf8_backup_fails_before_mutation_or_new_backup() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    let current = b"model = \"current\"\nmodel_provider = \"openai\"\n";
    write_config(&config_path, current);
    let candidate = backup_path(directory.path(), "invalid-utf8");
    fs::write(&candidate, [0xff, 0xfe]).unwrap();
    set_modified(&candidate, 1_700_000_000);

    assert!(matches!(
        config::restore_config(&config_path),
        Err(ConfigError::BackupUtf8 { .. })
    ));
    assert_eq!(fs::read(&config_path).unwrap(), current);
    assert_eq!(fs::read(&candidate).unwrap(), [0xff, 0xfe]);
    assert_eq!(count_recovery_backups(directory.path()), 1);
}

#[test]
fn invalid_toml_backup_fails_before_mutation_or_new_backup() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    let current = b"model = \"current\"\nmodel_provider = \"openai\"\n";
    write_config(&config_path, current);
    let candidate = backup_path(directory.path(), "invalid-toml");
    fs::write(&candidate, b"model = [\n").unwrap();
    set_modified(&candidate, 1_700_000_000);

    assert!(matches!(
        config::restore_config(&config_path),
        Err(ConfigError::BackupToml { .. })
    ));
    assert_eq!(fs::read(&config_path).unwrap(), current);
    assert_eq!(fs::read(&candidate).unwrap(), b"model = [\n");
    assert_eq!(count_recovery_backups(directory.path()), 1);
}

#[test]
fn restore_copies_exact_bytes_and_creates_reversible_backup_preserving_permissions() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    let current =
        b"# current comments\nmodel = \"current\" # inline\nmodel_provider = \"openai\"\n";
    let historical =
        b"# historical formatting\nmodel='openai/gpt-5'\nmodel_provider = 'openrouter'\n";
    let older_contents = b"# older state\nmodel = \"older\"\nmodel_provider = \"openai\"\n";
    write_config(&config_path, current);
    let selected = backup_path(directory.path(), "selected");
    let older = backup_path(directory.path(), "older");
    fs::write(&selected, historical).unwrap();
    fs::write(&older, older_contents).unwrap();
    set_modified(&selected, 1_700_000_000);
    set_modified(&older, 1_600_000_000);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = fs::Permissions::from_mode(0o640);
        fs::set_permissions(&config_path, permissions).unwrap();
        let result = config::restore_config(&config_path).unwrap();
        assert_eq!(fs::read(&config_path).unwrap(), historical);
        assert_eq!(fs::read(&result.backup_path).unwrap(), current);
        assert_eq!(fs::read(&selected).unwrap(), historical);
        assert_eq!(fs::read(&older).unwrap(), older_contents);
        assert_ne!(result.backup_path, selected);
        assert_eq!(count_recovery_backups(directory.path()), 3);
        assert_eq!(
            fs::metadata(&config_path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(result.restored_from, selected);
    }
    #[cfg(not(unix))]
    {
        let result = config::restore_config(&config_path).unwrap();
        assert_eq!(fs::read(&config_path).unwrap(), historical);
        assert_eq!(fs::read(&result.backup_path).unwrap(), current);
        assert_eq!(fs::read(&selected).unwrap(), historical);
        assert_eq!(fs::read(&older).unwrap(), older_contents);
        assert_ne!(result.backup_path, selected);
        assert_eq!(count_recovery_backups(directory.path()), 3);
        assert_eq!(result.restored_from, selected);
    }
}

#[test]
fn consecutive_restores_reverse_the_prior_restore() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    let current = b"model = \"current\"\nmodel_provider = \"openai\"\n";
    let historical = b"# historical\nmodel='older/model'\nmodel_provider='openrouter'\n";
    write_config(&config_path, current);
    let selected = backup_path(directory.path(), "historical");
    fs::write(&selected, historical).unwrap();
    set_modified(&selected, 1_600_000_000);

    let first = config::restore_config(&config_path).unwrap();
    assert_eq!(fs::read(&config_path).unwrap(), historical);
    let second = config::restore_config(&config_path).unwrap();

    assert_eq!(second.restored_from, first.backup_path);
    assert_eq!(fs::read(&config_path).unwrap(), current);
    assert!(selected.exists());
}

#[test]
fn restore_conflict_preserves_external_change_and_leaves_no_new_backup() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    let current = b"model = \"current\"\nmodel_provider = \"openai\"\n";
    let external = b"model = \"external\"\nmodel_provider = \"custom\"\n";
    let historical = b"model = \"historical\"\nmodel_provider = \"openrouter\"\n";
    write_config(&config_path, current);
    let selected = backup_path(directory.path(), "historical");
    fs::write(&selected, historical).unwrap();
    set_modified(&selected, 1_600_000_000);

    let result = config::test_support::restore_with_before_commit(&config_path, || {
        fs::write(&config_path, external).unwrap();
    });

    assert!(matches!(result, Err(ConfigError::Conflict { .. })));
    assert_eq!(fs::read(&config_path).unwrap(), external);
    assert_eq!(fs::read(&selected).unwrap(), historical);
    assert_eq!(count_recovery_backups(directory.path()), 1);
}

#[test]
fn cli_status_and_restore_parsers_remain_accepted() {
    assert_eq!(
        Cli::try_parse_from(["cps", "status"]).unwrap().command,
        Command::Status
    );
    assert_eq!(
        Cli::try_parse_from(["cps", "restore"]).unwrap().command,
        Command::Restore
    );
}

#[test]
fn restore_success_output_contains_only_file_names() {
    let result = config::RestoreResult {
        restored_from: PathBuf::from("/private/example/config.toml.cps-backup-old"),
        backup_path: PathBuf::from("/private/example/config.toml.cps-backup-new"),
    };
    assert_eq!(
        cli::format_restore_success(&result),
        "Restored config from config.toml.cps-backup-old.\nRecovery backup: config.toml.cps-backup-new"
    );
}
