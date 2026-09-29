# Codex Provider Switcher v0.1.1 — RC6 final validation

**Result: PASS with the signing and platform limits below.** This is a version-only release of the accepted RC5 product. Provider marks remain initials-only: DeepSeek `DS`, LM Studio `LM`, Ollama `OL`, OpenAI `OA`, OpenRouter `OR`, and xAI `XA`. The historical local `v0.1.0` tag was left untouched. Nothing was pushed, tagged, or published.

## Source and version

- v0.1.1 source commit: `688f6263fd45df0f73024807414804761218c0f3` (`chore: bump version to 0.1.1`).
- Product version metadata is `0.1.1` in `Cargo.toml`, `apps/desktop/package.json`, `apps/desktop/src-tauri/Cargo.toml`, and `apps/desktop/src-tauri/tauri.conf.json`; Cargo/npm lock metadata was updated accordingly.
- The CLI version assertion and the current release version in `README.md` were updated. Historical RC1–RC5 reports were not changed.
- The diff contains only version metadata, lock metadata, that assertion, and current release documentation. Provider behavior, UI, registry, configuration handling, credentials, and dependency versions did not change.
- The local annotated `v0.1.0` tag still peels to `7026cbfe97d34563aaed6088eceea6aea5e039e1`. No `v0.1.1` tag was created.

## Tests

- `cargo fmt --check` — PASS.
- `cargo test` — the first sandboxed attempt failed 7 localhost socket tests with `Operation not permitted`; rerunning the unchanged command with local socket permission passed: 113 passed, 0 failed; doctests passed.
- `cargo clippy --all-targets -- -D warnings` — PASS.
- `cd apps/desktop && npm run build` — PASS; TypeScript and Vite production build completed.
- `cd apps/desktop && npm test` — PASS: 23 tests across 2 files.
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` — PASS: 35 tests; doctests passed.
- `git diff --check` — PASS before both commits.

## Compatibility and provider marks

Isolated temporary config fixtures were used for CLI status and the existing Tauri `get_status` path. Each fixture stayed byte-identical and contained no additional recovery backup after inspection.

- Omitted `model_provider` with `model = "gpt-6-luna"` resolved to `openai/gpt-6-luna` in both paths.
- Explicit Ollama remained `ollama` in both paths.
- Non-string provider (`7`) and blank provider (`""`) were safely rejected in both paths. CLI exited 2 with the expected typed errors; Tauri returned `config_invalid`.
- The Tauri unit test `status_defaults_an_omitted_provider_to_openai` passed.
- Provider UI tests and production frontend verification show `DS / LM / OL / OA / OR / XA`; the unknown-provider fallback remains covered. `ProviderMark` renders text, there are no provider SVG imports, no provider SVG assets, and no runtime provider-logo URLs.

## Universal app

- Command: `cd apps/desktop && npm run tauri build -- --bundles app --target universal-apple-darwin --no-sign` — PASS.
- Validated app output: `apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/macos/Codex Provider Switcher.app`. Tauri removed the intermediate bundle while producing the DMG; a validation copy was retained for smoke launch and installation. The DMG executable hash matches that copy.
- Bundle identifier `tech.yliu.codex-provider-switcher`; version `0.1.1`; architectures `x86_64` and `arm64`.
- Executable SHA-256: `8813acdc120072db066be711b66a48e279e26ce1ca8c964b867a6577a957f442`.
- The executable embeds the production hashed frontend asset reference. The bundle contains no provider SVG assets. A smoke launch displayed OpenAI active and all six initials.

## Final DMG

- Command: `cd apps/desktop && npm run tauri build -- --verbose --bundles dmg --target universal-apple-darwin --no-sign` — PASS after disk-image permission retry.
- Output: `apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/dmg/Codex Provider Switcher_0.1.1_universal.dmg`.
- Size: 13,878,776 bytes. SHA-256: `76d37fc49cf85e30f69236b9922a97f80ff07ae082d31c2413fc037ca6964b19`.
- `hdiutil verify` passed (CRC32 `0ED5F0E4`). Read-only inspection found the expected version `0.1.1` app and `Applications` symlink. Its executable hash and universal architectures match the validated app. The packaged frontend is the current initials-only build and no provider SVG assets are present.
- The first sandboxed DMG build failed at `hdiutil create` with `Device not configured`; the unchanged command succeeded with disk-image permission.

## Installed app

- Installed the validated app in the standard Applications folder after fully quitting the prior app. The prior installed RC5 app was preserved as a rollback copy; older backups were retained.
- The running process came from the standard Applications bundle. Installed version, identifier, and executable SHA-256 match the validated app: `0.1.1`, `tech.yliu.codex-provider-switcher`, and `8813acdc120072db066be711b66a48e279e26ce1ca8c964b867a6577a957f442`.
- The installed UI showed `DS / LM / OL / OA / OR / XA` and OpenAI active for the omitted-provider config. Selecting LM Studio updated Selected provider; selecting OpenAI restored it. Diagnostics and Recovery rendered.
- At final inspection, the user config SHA-256 matched the saved pre-inspection baseline; `model_provider` remained omitted and the backup inventory was unchanged. No model switch, config write, diagnostic run, or recovery operation was performed.

## Signing and limitations

- Build commands used `--no-sign`. The Mach-O executable has a linker ad-hoc signature; the app bundle has no sealed resource signature, Developer ID identity, or Team ID. The DMG is unsigned. This release is not notarized or stapled.
- Physical Intel hardware, older macOS versions, live provider API/Keychain operations, an actual model switch, and a restore operation were not exercised. The universal Intel slice was verified with `lipo`.
