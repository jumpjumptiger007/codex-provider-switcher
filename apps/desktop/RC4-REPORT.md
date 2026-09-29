# Codex Provider Switcher v0.1.0 — RC4 final validation

**Result: PASS with documented environment and signing limitations.** No application source, test, dependency, or lockfile changes were made. Nothing was pushed or published.

## Accepted source and repository state

- Accepted source commit: `ceb33d1f0eed55411ac6b04dc07dba20837b1254` (`feat: polish desktop provider UI`).
- Initial branch: `main`; working tree clean; `main` was one commit ahead and zero behind `origin/main`.
- Visual Polish remained frozen throughout validation.

## Test results

- `cargo fmt --check` — PASS.
- `cargo test` — the sandboxed attempt ran 21 tests and 7 localhost HTTP tests failed to bind sockets (`Operation not permitted`). The unchanged command was rerun with local socket access: PASS, 113 passed, 0 failed; doctests passed.
- `cargo clippy --all-targets -- -D warnings` — PASS.
- `cd apps/desktop && npm run build` — PASS; TypeScript check passed and Vite built 40 modules.
- `cd apps/desktop && npm test` — PASS, 23 tests across 2 files.
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` — PASS, 35 passed, 0 failed; doctests passed.
- `cargo build --release --bin cps` — PASS for CLI compatibility smoke tests.
- `git diff --check` — PASS before report commit.

## Compatibility smoke tests

All CLI cases used temporary isolated `CODEX_HOME` fixtures. Fixture files retained their pre-test SHA-256 values, and no fixture recovery backup was created.

- Omitted top-level `model_provider`, with `model = "gpt-6-luna"`: `cps status` exited 0 and reported provider `openai`, target `openai/gpt-6-luna`.
- Explicit `model_provider = "ollama"`: `cps status` exited 0 and preserved `ollama`.
- Invalid provider type (`7`): safely rejected with exit 2 and `active model_provider must be a string`.
- Blank provider (`""`): safely rejected with exit 2 and `active model_provider must not be empty`.
- Tauri test `commands::tests::status_defaults_an_omitted_provider_to_openai` passed. The installed UI also showed OpenAI active while the read-only user config had no top-level `model_provider`. No config mutation, model switch, diagnostic action, or restore action was performed during UI inspection.

## Provider assets and production frontend

- Bundled local SVG marks: OpenAI Blossom, LM Studio, and OpenRouter.
- Intentional initials remained DeepSeek `DS`, Ollama `OL`, and xAI `XA`; unknown providers retain the first-two-character fallback (`UN` test case).
- Provider mark tests passed for all three local marks and four initials/fallback cases.
- Production JavaScript: `apps/desktop/dist/assets/index-Cm1hgusC.js` (SHA-256 `a20c166bc20126f279bcc46967b69df32a809aa5a190424d353ec38822d229a4`); it contains three inline SVG data URIs and no runtime HTTP(S) provider-logo references.
- Provider SVGs contain no external `href`, script, or foreign-object resources. The universal executable references the current hashed frontend asset path, and the running UI showed the Visual Polish layout, current marks, Diagnostics, and Recovery sections.

## Native macOS app

- Command: `cd apps/desktop && npm run tauri build -- --bundles app --no-sign`.
- Output: `apps/desktop/src-tauri/target/release/bundle/macos/Codex Provider Switcher.app`.
- Bundle identifier/version: `tech.yliu.codex-provider-switcher` / `0.1.0`.
- Executable architecture: `arm64`; SHA-256: `e6b599b55f77b1a44c7506663e87b0378e5541bb6e4c86e1ecca37fab6c0abbd`.
- The native build UI was smoke-launched from a temporary copy with a unique test bundle identifier; its executable hash matched the native output and it displayed OpenAI active for the omitted-provider config.

## Universal macOS app

- Command: `cd apps/desktop && npm run tauri build -- --bundles app --target universal-apple-darwin --no-sign`.
- Output: `apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/macos/Codex Provider Switcher.app`.
- Bundle identifier/version: `tech.yliu.codex-provider-switcher` / `0.1.0`.
- Executable contains `x86_64` and `arm64` slices; SHA-256: `e9038adf1c925c087bdb5e6219f2b13bec1b51bd6617172ec7af608d90e37285`.
- The universal build was smoke-launched from a temporary copy with a unique test bundle identifier to avoid Launch Services resolving its shared identifier to the installed RC3 app. The copy's executable matched the universal output byte-for-byte and rendered the current UI. The app later installed from the final DMG retained this exact hash.
- Tauri removed the intermediate `.app` from the bundle output while completing DMG packaging. The validated app was extracted back from the final DMG to the output path; its executable hash remained identical.

## Final DMG

- Build command: `cd apps/desktop && npm run tauri build -- --verbose --bundles dmg --target universal-apple-darwin --no-sign`.
- Output: `apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/dmg/Codex Provider Switcher_0.1.0_universal.dmg`.
- Size: 13,881,236 bytes. SHA-256: `9d2ef87d9331c1477e6335bcd94e9f9d20b9dd93bc9222152bccda695cb73b93`.
- `hdiutil verify` — PASS, valid checksum (CRC32 `C009C49A`).
- Read-only mount contained the expected `.app`, `Applications` symlink, `.DS_Store`, and volume icon. The packaged executable is universal and has the same SHA-256 as the validated universal app. Bundle identifier/version are correct.

## Installed app

- Installed the validated RC4 universal app in the standard Applications folder after fully quitting the old RC3 process.
- The running process executable path resolved to the installed app; its SHA-256 matches the universal app and DMG (`e9038adf…`).
- The installed UI showed OpenAI active with the omitted-provider config; OpenAI, LM Studio, and OpenRouter marks were visible, as were the intentional DS/OL/XA initials. Selecting LM Studio updated the selected-provider view; selecting OpenAI restored the display. Diagnostics and Recovery rendered.
- The previous installed RC3 app was preserved under a new dated backup name. The existing dated backup was retained because its executable and bundle contents differed from the installed RC3 app.

## Signing, packaging notes, and limitations

- Builds used `--no-sign`. The Mach-O executable has an ad-hoc linker signature; there is no Developer ID identity or Team ID, the app bundle has no sealed resources, and the DMG is unsigned. No notarization, stapling, or Apple submission occurred.
- The first sandboxed Tauri DMG attempt failed at `hdiutil create` with `Device not configured`. The unchanged build command succeeded with the minimum elevated disk-image access. It did not encounter the known resource-busy cleanup issue, so no RC4 DMG compression workaround was needed.
- Tauri cleanup removed the pre-existing RC3 DMG from the bundle output folder. It was reconstructed from the retained RC3 read/write staging image and verified. The reconstructed image has the same 13,877,048-byte size, valid inner checksum, RC3 executable hash, and contents, but recompression changed the outer SHA-256 to `bda31ffa3a47771bac3284362d067b16d07a97070f83f6160b6d79d2a30c20d6` (the prior report recorded `8aba3e14e637fc0c7e4eec76f3695a9e18eb9c222fb837a9a8919685215a87e1`).
- Side-by-side shell `open` attempts for bundles sharing the installed identifier returned `kLSNoExecutableErr`; temporary unique-identifier copies were used for native/universal build-output UI smoke tests. The final installed app launched successfully and its process path was verified.
- Physical Intel-machine execution, older macOS versions, live provider API/Keychain operations, actual model switching, and a restore operation were not exercised. The universal Intel slice was confirmed with `lipo`.
