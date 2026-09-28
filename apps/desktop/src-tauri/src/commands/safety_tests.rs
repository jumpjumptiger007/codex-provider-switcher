use std::{
    cell::Cell,
    fs::{self, File, FileTimes},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, UNIX_EPOCH},
};

use cps::{
    application::{self, DiagnosticFinding, DiagnosticSeverity, DoctorReport},
    config::{self, ConfigError},
    credential::{CredentialStore, CredentialStoreError, SecretValue},
    provider::{CredentialSlotId, ProviderRegistry},
};
use serde_json::json;

use super::{map_restore_error, restore_config_at, restore_dto, run_doctor_at, safe_filename};
use crate::dto::DoctorReportDto;

struct Fixture {
    directory: PathBuf,
    config: PathBuf,
}
impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "cps-desktop-safety-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let config = directory.join("config.toml");
        fs::write(&config, bytes).unwrap();
        Self { directory, config }
    }
    fn backup(&self, suffix: &str, bytes: &[u8]) -> PathBuf {
        let path = self
            .directory
            .join(format!("config.toml.cps-backup-{suffix}"));
        fs::write(&path, bytes).unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(
                FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(1_600_000_000)),
            )
            .unwrap();
        path
    }
    fn count(&self) -> usize {
        fs::read_dir(&self.directory).unwrap().count()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

struct FakeStore {
    value: Option<&'static str>,
    reads: Cell<usize>,
}
impl CredentialStore for FakeStore {
    fn get(&self, _: &CredentialSlotId) -> Result<Option<SecretValue>, CredentialStoreError> {
        self.reads.set(self.reads.get() + 1);
        Ok(self.value.map(|value| SecretValue::new(value).unwrap()))
    }
    fn set(&self, _: &CredentialSlotId, _: &SecretValue) -> Result<(), CredentialStoreError> {
        panic!("diagnostics must not save credentials")
    }
    fn delete(&self, _: &CredentialSlotId) -> Result<(), CredentialStoreError> {
        panic!("diagnostics must not delete credentials")
    }
}

const CURRENT: &[u8] = b"model = \"current\"\nmodel_provider = \"openai\"\n";
const HISTORICAL: &[u8] =
    b"# exact bytes\r\nmodel = \"historical\"\r\nmodel_provider = \"openai\"\r\n\n";

#[test]
fn doctor_native_is_read_only_and_does_not_access_credentials() {
    let fixture = Fixture::new(CURRENT);
    let store = FakeStore {
        value: Some("never-output-this"),
        reads: Cell::new(0),
    };
    let before = fs::metadata(&fixture.config).unwrap().permissions();
    let report = run_doctor_at(Some(&store), &fixture.config);
    assert!(!report.has_errors);
    assert_eq!(store.reads.get(), 0);
    assert_eq!(fs::read(&fixture.config).unwrap(), CURRENT);
    assert_eq!(fs::metadata(&fixture.config).unwrap().permissions(), before);
    assert_eq!(fixture.count(), 1);
    assert_eq!(
        serde_json::to_value(&report).unwrap()["findings"],
        json!([
            {"check":"config", "severity":"ok", "message":"valid"},
            {"check":"selection", "severity":"ok", "message":"openai/current"},
            {"check":"provider", "severity":"ok", "message":"openai (native)"},
            {"check":"recovery", "severity":"info", "message":"no CPS recovery backup"}
        ])
    );
    assert!(!run_doctor_at(None, &fixture.config).has_errors);
}

#[test]
fn doctor_direct_present_credential_and_definition_are_ok_without_mutation_or_secrets() {
    let fixture = Fixture::new(CURRENT);
    let registry = ProviderRegistry::initial();
    let store = FakeStore {
        value: Some("never-output-this"),
        reads: Cell::new(0),
    };
    application::use_provider_model(
        &registry,
        Some(&store),
        &fixture.config,
        &cps::domain::ProviderModelTarget {
            provider: "openrouter".into(),
            model: "test/model".into(),
        },
    )
    .unwrap();
    store.reads.set(0);
    let before = fs::read(&fixture.config).unwrap();
    let count = fixture.count();
    let report = run_doctor_at(Some(&store), &fixture.config);
    assert!(!report.has_errors);
    assert_eq!(store.reads.get(), 1);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(
        json["findings"][4],
        json!({"check":"credential", "severity":"ok", "message":"present"})
    );
    assert!(!json.to_string().contains("never-output-this"));
    assert!(!json
        .to_string()
        .contains(fixture.directory.to_str().unwrap()));
    assert_eq!(fs::read(&fixture.config).unwrap(), before);
    assert_eq!(fixture.count(), count);
}

#[test]
fn doctor_direct_missing_credential_is_an_error() {
    let fixture = Fixture::new(b"model = \"test\"\nmodel_provider = \"deepseek\"\n");
    let store = FakeStore {
        value: None,
        reads: Cell::new(0),
    };
    let report = run_doctor_at(Some(&store), &fixture.config);
    assert!(report.has_errors);
    assert_eq!(
        serde_json::to_value(&report).unwrap()["findings"][4],
        json!({"check":"credential", "severity":"error", "message":"missing"})
    );
    assert_eq!(store.reads.get(), 1);
}

#[test]
fn doctor_unknown_provider_preserves_unmanaged_warning_without_blocking_failure() {
    let fixture = Fixture::new(b"model = \"test\"\nmodel_provider = \"custom\"\n");
    let store = FakeStore {
        value: None,
        reads: Cell::new(0),
    };
    let report = run_doctor_at(Some(&store), &fixture.config);
    assert!(!report.has_errors);
    assert_eq!(
        serde_json::to_value(&report).unwrap()["findings"][2],
        json!({"check":"provider", "severity":"warning", "message":"not managed by CPS"})
    );
    assert_eq!(store.reads.get(), 0);
}

#[test]
fn doctor_invalid_config_is_structured_and_still_reports_recovery() {
    for bytes in [b"model = [".as_slice(), &[0xff]] {
        let fixture = Fixture::new(bytes);
        let report = run_doctor_at(None, &fixture.config);
        assert!(report.has_errors);
        assert_eq!(report.findings.len(), 2);
        let json = serde_json::to_value(report).unwrap();
        assert_eq!(json["findings"][0]["severity"], "error");
        assert_eq!(json["findings"][1]["check"], "recovery");
        assert_eq!(fs::read(&fixture.config).unwrap(), bytes);
        assert_eq!(fixture.count(), 1);
    }
}

#[test]
fn doctor_dto_preserves_order_all_severities_and_core_error_semantics() {
    let findings = [
        DiagnosticSeverity::Warning,
        DiagnosticSeverity::Info,
        DiagnosticSeverity::Error,
        DiagnosticSeverity::Ok,
    ]
    .into_iter()
    .map(|severity| DiagnosticFinding {
        check: "test",
        severity,
        message: "safe".into(),
    })
    .collect();
    let dto: DoctorReportDto = DoctorReport { findings }.into();
    let json = serde_json::to_value(dto).unwrap();
    assert_eq!(json["hasErrors"], true);
    assert_eq!(
        json["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["severity"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["warning", "info", "error", "ok"]
    );
}

fn assert_safe_error(error: crate::dto::DesktopError, code: &str, directory: &Path) {
    assert_eq!(error.code, code);
    let json = serde_json::to_string(&error).unwrap();
    assert!(!json.contains(directory.to_str().unwrap()));
    assert!(!json.contains("/Users/"));
}

#[test]
fn restore_no_backup_maps_to_safe_code() {
    let fixture = Fixture::new(CURRENT);
    assert_safe_error(
        restore_config_at(&fixture.config).unwrap_err(),
        "no_recovery_backup",
        &fixture.directory,
    );
    assert_eq!(fs::read(&fixture.config).unwrap(), CURRENT);
}

#[test]
fn restore_ambiguous_newest_maps_to_safe_code() {
    let fixture = Fixture::new(CURRENT);
    fixture.backup("one", HISTORICAL);
    fixture.backup("two", HISTORICAL);
    assert_safe_error(
        restore_config_at(&fixture.config).unwrap_err(),
        "ambiguous_recovery_backup",
        &fixture.directory,
    );
    assert_eq!(fs::read(&fixture.config).unwrap(), CURRENT);
}

#[test]
fn restore_invalid_utf8_and_toml_map_to_recovery_invalid() {
    for bytes in [b"model = [".as_slice(), &[0xff]] {
        let fixture = Fixture::new(CURRENT);
        fixture.backup("invalid", bytes);
        assert_safe_error(
            restore_config_at(&fixture.config).unwrap_err(),
            "recovery_invalid",
            &fixture.directory,
        );
        assert_eq!(fs::read(&fixture.config).unwrap(), CURRENT);
        assert_eq!(fixture.count(), 2);
    }
}

#[test]
fn restore_exact_bytes_safe_filenames_recovery_backup_permissions_and_second_restore() {
    let fixture = Fixture::new(CURRENT);
    fixture.backup("historical", HISTORICAL);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&fixture.config, fs::Permissions::from_mode(0o640)).unwrap();
    }
    let permissions = fs::metadata(&fixture.config).unwrap().permissions();
    let result = restore_config_at(&fixture.config).unwrap();
    assert_eq!(fs::read(&fixture.config).unwrap(), HISTORICAL);
    assert_eq!(
        fs::metadata(&fixture.config).unwrap().permissions(),
        permissions
    );
    assert_eq!(result.restored_from, "config.toml.cps-backup-historical");
    assert!(result
        .recovery_backup
        .starts_with("config.toml.cps-backup-"));
    assert_eq!(
        fs::read(fixture.directory.join(&result.recovery_backup)).unwrap(),
        CURRENT
    );
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json.as_object().unwrap().len(), 2);
    for key in ["restoredFrom", "recoveryBackup"] {
        let name = json[key].as_str().unwrap();
        assert!(!name.contains('/'));
        assert!(!name.contains('\\'));
        assert!(!Path::new(name).is_absolute());
    }
    let second = restore_config_at(&fixture.config).unwrap();
    assert_eq!(second.restored_from, result.recovery_backup);
    assert_eq!(fs::read(&fixture.config).unwrap(), CURRENT);
    assert_eq!(
        fs::read(fixture.directory.join(second.recovery_backup)).unwrap(),
        HISTORICAL
    );
}

#[test]
fn restore_success_does_not_depend_on_later_status_read() {
    let fixture = Fixture::new(CURRENT);
    fixture.backup(
        "no-selection",
        b"# valid TOML without an active selection\n",
    );
    assert!(restore_config_at(&fixture.config).is_ok());
    assert!(crate::commands::get_status_at(&fixture.config).is_err());
}

#[test]
fn restore_conflict_mapping_preserves_external_state() {
    let fixture = Fixture::new(CURRENT);
    fixture.backup("historical", HISTORICAL);
    let external = b"model = \"external\"\nmodel_provider = \"custom\"\n";
    let core_error = config::test_support::restore_with_before_commit(&fixture.config, || {
        fs::write(&fixture.config, external).unwrap();
    })
    .unwrap_err();
    let error = map_restore_error(core_error.into());
    assert!(error.message.contains("CPS did not overwrite it"));
    assert_safe_error(error, "config_conflict", &fixture.directory);
    assert_eq!(fs::read(&fixture.config).unwrap(), external);
    assert_eq!(fixture.count(), 2);
}

#[test]
fn restore_io_failures_and_unexpected_errors_are_safe() {
    let fixture = Fixture::new(CURRENT);
    let io = || std::io::Error::other("sensitive /Users/private/config detail");
    for error in [
        ConfigError::BackupRead {
            path: fixture.config.clone(),
            source: io(),
        },
        ConfigError::BackupMetadata {
            path: fixture.config.clone(),
            source: io(),
        },
        ConfigError::Read {
            path: fixture.config.clone(),
            source: io(),
        },
        ConfigError::BackupDirectoryRead {
            directory: fixture.directory.clone(),
            source: io(),
        },
    ] {
        assert_safe_error(
            map_restore_error(error.into()),
            "restore_unavailable",
            &fixture.directory,
        );
    }
    for error in [
        ConfigError::RestoreWrite(io()),
        ConfigError::RestoreTempFile(io()),
        ConfigError::RestorePersist {
            path: fixture.config.clone(),
            source: io(),
        },
        ConfigError::Backup {
            directory: fixture.directory.clone(),
            source: io(),
        },
    ] {
        assert_safe_error(
            map_restore_error(error.into()),
            "restore_failed",
            &fixture.directory,
        );
    }
    assert_safe_error(
        map_restore_error(application::ApplicationError::UnsupportedPlatform),
        "internal_error",
        &fixture.directory,
    );
}

#[test]
fn restore_dto_rejects_missing_non_utf8_or_unsafe_filename() {
    for path in [
        Path::new("/"),
        Path::new(".."),
        Path::new("bad\\name"),
        Path::new("bad\nname"),
    ] {
        assert_eq!(safe_filename(path).unwrap_err().code, "internal_error");
    }
    assert_eq!(
        restore_dto(config::RestoreResult {
            restored_from: "/".into(),
            backup_path: "safe".into()
        })
        .unwrap_err()
        .code,
        "internal_error"
    );
    #[cfg(unix)]
    {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let path = PathBuf::from(OsString::from_vec(vec![0xff]));
        assert_eq!(safe_filename(&path).unwrap_err().code, "internal_error");
    }
}
