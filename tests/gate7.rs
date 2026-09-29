use std::{
    cell::Cell,
    fs,
    path::Path,
    time::{Duration, UNIX_EPOCH},
};

use cps::{
    application::{self, DiagnosticSeverity},
    auth_command::project_keychain_auth_command,
    cli, config,
    credential::{CredentialOperation, CredentialStore, CredentialStoreError, SecretValue},
    provider::{CredentialSlotId, ProviderRegistry},
};
use tempfile::tempdir;

struct FakeStore {
    value: Option<&'static str>,
    fail: bool,
    reads: Cell<usize>,
}

impl FakeStore {
    fn present() -> Self {
        Self {
            value: Some("not-for-output"),
            fail: false,
            reads: Cell::new(0),
        }
    }
}

impl CredentialStore for FakeStore {
    fn set(&self, _: &CredentialSlotId, _: &SecretValue) -> Result<(), CredentialStoreError> {
        Ok(())
    }
    fn get(&self, _: &CredentialSlotId) -> Result<Option<SecretValue>, CredentialStoreError> {
        self.reads.set(self.reads.get() + 1);
        if self.fail {
            return Err(CredentialStoreError::Backend {
                operation: CredentialOperation::Retrieve,
            });
        }
        self.value
            .map(SecretValue::new)
            .transpose()
            .map_err(|_| CredentialStoreError::InvalidStoredValue)
    }
    fn delete(&self, _: &CredentialSlotId) -> Result<(), CredentialStoreError> {
        Ok(())
    }
}

fn write(path: &Path, text: &[u8]) {
    fs::write(path, text).unwrap();
}

fn severities(report: &application::DoctorReport) -> Vec<(&'static str, DiagnosticSeverity)> {
    report
        .findings
        .iter()
        .map(|finding| (finding.check, finding.severity))
        .collect()
}

#[test]
fn native_and_unknown_diagnostics_are_read_only_and_do_not_query_credentials() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    let bytes = b"model = \"gpt-5\"\nmodel_provider = \"openai\"\n";
    write(&config_path, bytes);
    let before_mode = fs::metadata(&config_path).unwrap().permissions();
    let store = FakeStore::present();
    let report = application::doctor(&ProviderRegistry::initial(), Some(&store), &config_path);
    assert_eq!(
        severities(&report),
        vec![
            ("config", DiagnosticSeverity::Ok),
            ("selection", DiagnosticSeverity::Ok),
            ("provider", DiagnosticSeverity::Ok),
            ("recovery", DiagnosticSeverity::Info)
        ]
    );
    assert!(!report.has_errors());
    assert_eq!(store.reads.get(), 0);
    assert_eq!(fs::read(&config_path).unwrap(), bytes);
    assert_eq!(
        fs::metadata(&config_path).unwrap().permissions(),
        before_mode
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);

    write(
        &config_path,
        b"model = \"foo/bar\"\nmodel_provider = \"custom-local\"\n",
    );
    let report = application::doctor(&ProviderRegistry::initial(), Some(&store), &config_path);
    assert_eq!(report.findings[2].severity, DiagnosticSeverity::Warning);
    assert_eq!(store.reads.get(), 0);
}

#[test]
fn direct_provider_checks_projected_definition_and_credential_without_secret_output() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("config.toml");
    write(
        &config_path,
        b"model = \"openai/gpt-5\"\nmodel_provider = \"openrouter\"\n",
    );
    let registry = ProviderRegistry::initial();
    let provider = registry.get("openrouter").unwrap();
    let definition = provider.project_custom_codex_provider().unwrap().unwrap();
    let direct = provider.direct_responses.as_ref().unwrap();
    config::update_provider_and_selection(
        &config_path,
        &config::ActiveSelection::new("openai/gpt-5", "openrouter").unwrap(),
        &definition,
        &project_keychain_auth_command(&direct.credential_slot),
    )
    .unwrap();
    let before = fs::read(&config_path).unwrap();
    let store = FakeStore::present();
    let report = application::doctor(&registry, Some(&store), &config_path);
    assert!(!report.has_errors());
    assert_eq!(report.findings[1].message, "openrouter/openai/gpt-5");
    assert_eq!(report.findings[3].severity, DiagnosticSeverity::Ok);
    assert_eq!(report.findings[4].severity, DiagnosticSeverity::Ok);
    assert_eq!(store.reads.get(), 1);
    assert!(!cli::format_doctor_report(&report).contains("not-for-output"));
    assert!(!format!("{report:?}").contains("not-for-output"));
    assert_eq!(fs::read(&config_path).unwrap(), before);
}

#[test]
fn direct_provider_reports_missing_definition_and_credential_or_backend_errors() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("config.toml");
    write(&path, b"model = \"x\"\nmodel_provider = \"deepseek\"\n");
    let missing = FakeStore {
        value: None,
        fail: false,
        reads: Cell::new(0),
    };
    let report = application::doctor(&ProviderRegistry::initial(), Some(&missing), &path);
    assert_eq!(
        severities(&report)[3..5],
        [
            ("provider_definition", DiagnosticSeverity::Error),
            ("credential", DiagnosticSeverity::Error)
        ]
    );
    assert!(report.has_errors());

    write(
        &path,
        b"model = \"x\"\nmodel_provider = \"deepseek\"\n[model_providers]\ndeepseek = 1\n",
    );
    let backend = FakeStore {
        value: None,
        fail: true,
        reads: Cell::new(0),
    };
    let report = application::doctor(&ProviderRegistry::initial(), Some(&backend), &path);
    assert_eq!(report.findings[3].severity, DiagnosticSeverity::Error);
    assert_eq!(report.findings[4].severity, DiagnosticSeverity::Error);

    write(
        &path,
        b"model = \"x\"\nmodel_provider = \"deepseek\"\nmodel_providers = 1\n",
    );
    let report = application::doctor(&ProviderRegistry::initial(), Some(&backend), &path);
    assert_eq!(report.findings[3].check, "provider_definition");
    assert_eq!(report.findings[3].severity, DiagnosticSeverity::Error);
    assert_eq!(report.findings[3].message, "model_providers is malformed");
}

#[test]
fn config_and_selection_errors_are_reports_not_early_failures() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("config.toml");
    for bytes in [b"\xff".as_slice(), b"not = [valid".as_slice()] {
        write(&path, bytes);
        let report = application::doctor(&ProviderRegistry::initial(), None, &path);
        assert_eq!(report.findings[0].severity, DiagnosticSeverity::Error);
        assert_eq!(report.findings[1].check, "recovery");
    }
    fs::remove_file(&path).unwrap();
    assert!(application::doctor(&ProviderRegistry::initial(), None, &path).has_errors());
    write(&path, b"model_provider = \"openai\"\n");
    let report = application::doctor(&ProviderRegistry::initial(), None, &path);
    assert_eq!(
        report.findings[1],
        application::DiagnosticFinding {
            check: "selection",
            severity: DiagnosticSeverity::Error,
            message: "model is missing".to_owned()
        }
    );

    write(&path, b"model = \"gpt-6-luna\"\n");
    let report = application::doctor(&ProviderRegistry::initial(), None, &path);
    assert_eq!(report.findings[1].check, "selection");
    assert_eq!(report.findings[1].severity, DiagnosticSeverity::Ok);
    assert_eq!(report.findings[1].message, "openai/gpt-6-luna");

    for (contents, message) in [
        (
            b"model = 1\nmodel_provider = \"openai\"\n".as_slice(),
            "model must be a string",
        ),
        (
            b"model = \"  \"\nmodel_provider = \"openai\"\n".as_slice(),
            "model is blank",
        ),
        (
            b"model = \"gpt-5\"\nmodel_provider = false\n".as_slice(),
            "model_provider must be a string",
        ),
        (
            b"model = \"gpt-5\"\nmodel_provider = \" \"\n".as_slice(),
            "model_provider is blank",
        ),
    ] {
        write(&path, contents);
        let report = application::doctor(&ProviderRegistry::initial(), None, &path);
        assert_eq!(report.findings[1].check, "selection");
        assert_eq!(report.findings[1].severity, DiagnosticSeverity::Error);
        assert_eq!(report.findings[1].message, message);
    }
}

#[test]
fn recovery_inspection_is_deterministic_and_warning_only() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("config.toml");
    write(&path, b"model = \"gpt-5\"\nmodel_provider = \"openai\"\n");
    let backup = directory.path().join("config.toml.cps-backup-new");
    write(&backup, b"bad = [toml");
    let report = application::doctor(&ProviderRegistry::initial(), None, &path);
    assert!(!report.has_errors());
    assert_eq!(
        report.findings.last().unwrap().severity,
        DiagnosticSeverity::Warning
    );
    assert_eq!(
        cli::format_doctor_report(&report).lines().last(),
        Some("result: warnings")
    );

    write(&backup, b"model = \"old\"\n");
    let second = directory.path().join("config.toml.cps-backup-tied");
    write(&second, b"model = \"also-old\"\n");
    let time = UNIX_EPOCH + Duration::from_secs(123);
    fs::File::open(&backup)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(time))
        .unwrap();
    fs::File::open(&second)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(time))
        .unwrap();
    assert_eq!(
        application::doctor(&ProviderRegistry::initial(), None, &path)
            .findings
            .last()
            .unwrap()
            .severity,
        DiagnosticSeverity::Warning
    );
}

#[test]
fn invalid_utf8_recovery_backup_is_warning_only_and_is_byte_preserved() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("config.toml");
    write(&path, b"model = \"gpt-5\"\nmodel_provider = \"openai\"\n");
    let backup = directory.path().join("config.toml.cps-backup-invalid-utf8");
    let original_backup = b"\xff\x80not toml";
    write(&backup, original_backup);

    let report = application::doctor(&ProviderRegistry::initial(), None, &path);
    assert!(!report.has_errors());
    assert_eq!(report.findings.last().unwrap().check, "recovery");
    assert_eq!(
        report.findings.last().unwrap().severity,
        DiagnosticSeverity::Warning
    );
    assert_eq!(
        report.findings.last().unwrap().message,
        "latest backup is invalid UTF-8"
    );
    assert_eq!(fs::read(&backup).unwrap(), original_backup);
}
