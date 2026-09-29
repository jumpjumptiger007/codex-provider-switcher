# CPS Desktop RC3 — Post-fix validation candidate

**State: EXECUTED — PASS, with a DMG cleanup workaround documented below.** This RC3 candidate is built from the post-fix baseline at e0251b7df220e7154820cc8d67e79cfedb0cf870. No application source, tests, dependencies, or lockfiles changed. Nothing was released or published.

## Baseline and Git state

- Branch: main; upstream: origin/main; ahead/behind: 0/0.
- Initial HEAD: e0251b7df220e7154820cc8d67e79cfedb0cf870.
- Initial tree and index: clean.
- Host: arm64 Mac.
- The user’s installed-app quit/reopen verification supplied in the request is accepted external evidence.

At completion, HEAD remains the accepted baseline and only this report is untracked. The index is empty. Build and packaging outputs are ignored.

## Validation

Commands run from the repository root:

- cargo fmt --check — PASS.
- cargo test — PASS, 113 passed, 0 failed; doctests 0.
- cargo clippy --all-targets -- -D warnings — PASS.
- git diff --check — PASS; a separate trailing-whitespace scan of the new report also passed.

The first sandboxed cargo test attempt stopped before running tests because Cargo could not open target/debug/.cargo-build-lock (Operation not permitted). The unchanged command was rerun with project-local build-artifact access and passed. No localhost or socket test failed, and no test was changed.

Commands run from apps/desktop:

- npm run build — PASS; TypeScript check and Vite build passed, 37 modules transformed.
- npm test — PASS, 16 tests passed in 1 file.
- cargo fmt --check --manifest-path apps/desktop/src-tauri/Cargo.toml — PASS.
- cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml — PASS, 35 passed, 0 failed; doctests 0.
- cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings — PASS.
- npm run tauri build -- --bundles app --no-sign — PASS.
- npm run tauri build -- --bundles app --target universal-apple-darwin --no-sign — PASS.

The native app is at apps/desktop/src-tauri/target/release/bundle/macos/Codex Provider Switcher.app. Tauri reported 19.22 MiB; its executable is thin arm64 with SHA-256 dcb32268cf664ae1f12f2bb4745439e01549b6db29385a79c91379ea65f2c3b7.

The universal app is at apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/macos/Codex Provider Switcher.app. Tauri reported 37.33 MiB; its executable contains x86_64 and arm64 with SHA-256 e218340539a4310c7bdfd17629ffce92244d2c474db59d795178c6dda3d69094.

Both bundles report product name Codex Provider Switcher, identifier tech.yliu.codex-provider-switcher, executable cps-desktop, version 0.1.0, icon icon.icns, and package type APPL. Each bundle contains exactly Contents/Info.plist, Contents/MacOS/cps-desktop, and Contents/Resources/icon.icns. The icon is present. Production dist contains index.html and hashed CSS/JS only; no source maps, .env files, credentials, sidecars, plugins, updater, or extra app resources were found. The app renders on tauri://localhost from embedded frontend assets; it does not depend on a development server.

## Post-fix release smoke and CLI regression

The universal release app was launched with isolated CODEX_HOME fixtures. The real Codex config.toml and Keychain credentials were not changed.

Fixture contents were:
- omitted: model = "gpt-6-luna"
- explicit: model = "gpt-6-luna"; model_provider = "ollama"
- invalid-type: model = "gpt-6-luna"; model_provider = 7
- blank: model = "gpt-6-luna"; model_provider = ""

Exact launch commands (run from the repository root):
    open -n --env 'CODEX_HOME=<temporary-directory>/omitted' --stderr '<temporary-directory>/omitted-app-open.log' '<repo>/apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/macos/Codex Provider Switcher.app'
    open -n --env 'CODEX_HOME=<temporary-directory>/explicit' --stderr '<temporary-directory>/explicit-app.log' '<repo>/apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/macos/Codex Provider Switcher.app'
    open -n --env 'CODEX_HOME=<temporary-directory>/invalid-type' --stderr '<temporary-directory>/invalid-type-app.log' '<repo>/apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/macos/Codex Provider Switcher.app'
    open -n --env 'CODEX_HOME=<temporary-directory>/blank' --stderr '<temporary-directory>/blank-app.log' '<repo>/apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/macos/Codex Provider Switcher.app'
- CODEX_HOME=<temporary-directory>/omitted, stderr <temporary-directory>/omitted-app-open.log
- CODEX_HOME=<temporary-directory>/explicit, stderr <temporary-directory>/explicit-app.log
- CODEX_HOME=<temporary-directory>/invalid-type, stderr <temporary-directory>/invalid-type-app.log
- CODEX_HOME=<temporary-directory>/blank, stderr <temporary-directory>/blank-app.log


- Omitted provider: fixture contained only model = "gpt-6-luna". The app showed Current setup OpenAI, openai/gpt-6-luna, Active, and all 6 providers.
- Explicit provider: model_provider = "ollama" remained selected; the app showed ollama/gpt-6-luna and Active.
- Invalid provider type (integer) and blank provider: both showed the safe “Current setup is unavailable” message, “Codex configuration is missing a valid active provider or model.”, while the 6-provider inventory remained visible.
- The invalid fixtures displayed “Credential status has not been checked yet.” No credential check or model-discovery action was invoked.

The UI startup calls get_providers and get_status. The status path reads config and the static registry; it does not call the credential store or network. The passing backend test status_read_only_path_has_no_credential_or_network_dependencies covers that boundary. Release logs were empty.

Each fixture directory contains only config.toml. Before/after SHA-256 values were unchanged, and no backup was created:

- omitted: 2b9e1f122f6cf3dcdfc7bbd58976a76af0169a37bf802753d8e7db0ac76a6f79
- explicit: 0aa37af6b0feb469e0f64a605b5f6379330e56d736c71b805cf63bf7b92f4dcf
- invalid type: 86538ea35dffaf5d17e840ce5986ccdcf566818d2794465b6bea9a2d9accaa75
- blank: 73038777724a3f6ad72c2023a4396e8c41455836ead7fd405ae4d20da1390621

CLI regression commands:

- cargo build --release --bin cps — PASS.
- CODEX_HOME='<temporary-directory>/omitted' ./target/release/cps status exited 0 and printed provider openai, model gpt-6-luna, target openai/gpt-6-luna, known_provider yes, transport native.
- CODEX_HOME='<temporary-directory>/omitted' ./target/release/cps models exited 2 with “CPS does not enumerate models for Codex-managed provider openai in Gate 5.” This confirms default resolution reached openai without MissingActiveProvider. No provider API discovery was invoked.

## DMG

The old default-path distribution artifact was preserved as apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/dmg/Codex Provider Switcher_0.1.0_universal_RC1-preserved.dmg before packaging. Its measured size and SHA-256 are 14,043,583 bytes and d78f802eac7d8cf20841df818be410a1e5a59771225e0faf7e103a648268025f.

The documented Tauri packaging command was run:

    npm run tauri build -- --verbose --bundles dmg --target universal-apple-darwin --no-sign

Tauri built the current universal app and populated its writable staging image, including the app and Applications link. Its Finder-layout script then failed to unmount the staging image because macOS reported “Resource busy.” The staging image was detached cleanly with hdiutil after the command exited. The final compression step from Tauri’s generated bundle_dmg.sh flow was then run to a unique RC3 path:

    hdiutil convert "src-tauri/target/universal-apple-darwin/release/bundle/macos/<generated-staging-image>.dmg" -format UDZO -imagekey zlib-level=9 -o "src-tauri/target/universal-apple-darwin/release/bundle/dmg/Codex Provider Switcher_0.1.0_universal_RC3.dmg"

This cleanup-only workaround produced the final artifact. The verification command was hdiutil verify "src-tauri/target/universal-apple-darwin/release/bundle/dmg/Codex Provider Switcher_0.1.0_universal_RC3.dmg"; it reported a valid checksum, CRC32 9F8319B5.

- DMG: apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/dmg/Codex Provider Switcher_0.1.0_universal_RC3.dmg
- Size: 13,877,048 bytes.
- SHA-256: 8aba3e14e637fc0c7e4eec76f3695a9e18eb9c222fb837a9a8919685215a87e1.
- Mounted read-only contents: .DS_Store, .VolumeIcon.icns, Applications -> system Applications directory, and Codex Provider Switcher.app.
- The packaged app reports bundle ID tech.yliu.codex-provider-switcher, version, executable, and icon. Its executable is universal x86_64/arm64 and has the same SHA-256 as the source universal app executable: e218340539a4310c7bdfd17629ffce92244d2c474db59d795178c6dda3d69094.

## Installed app and signing state

Read-only inspection of the installed Applications copy found the expected name, bundle ID tech.yliu.codex-provider-switcher, executable, icon, and version 0.1.0. Its universal executable SHA-256 is e218340539a4310c7bdfd17629ffce92244d2c474db59d795178c6dda3d69094, byte-identical to the fresh RC3 universal executable. This confirms the installed executable matches the validated post-fix build. The installed app and its dated backup were not modified.

Both app build commands and the DMG packaging command used --no-sign. The main Mach-O executable has an ad-hoc linker signature, with no Team ID, no bound Info.plist, and no sealed resources; the app bundle is not Developer-ID-signed. The DMG has no code signature. Read-only stapler validation found no ticket on the app or DMG (exit 65 for each). No Developer ID signing, notarization, stapling, or Apple submission occurred.

## Security / authority

Tauri command registration and the default capability remain exactly: get_providers, get_status, get_credential_status, save_credential, get_models, switch_model, run_doctor, restore_config. No Tauri plugin registration or generic shell, filesystem, process, or network permission was found. Existing commands retain only their intended application behavior.

## Remaining unverified items and final Git state

Intel-machine execution, older macOS releases, live provider API discovery, credential writes, actual model switching, and restore execution were not exercised.

Final git status --short --branch:

    ## main...origin/main
    ?? apps/desktop/RC3-REPORT.md

No commit, push, tag, release, or publication was made.

