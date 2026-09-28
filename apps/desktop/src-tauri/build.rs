fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_providers",
            "get_status",
            "get_credential_status",
            "save_credential",
            "get_models",
            "switch_model",
            "run_doctor",
            "restore_config",
        ]),
    ))
    .expect("failed to build Tauri application metadata");
}
