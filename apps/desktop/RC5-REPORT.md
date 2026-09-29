# Codex Provider Switcher v0.1.0 — RC5 final validation

**Result: PASS, with signing and platform limitations below.** The final provider marks are initials-only: DeepSeek `DS`, LM Studio `LM`, Ollama `OL`, OpenAI `OA`, OpenRouter `OR`, and xAI `XA`. Unknown providers retain the two-character fallback. No source changes followed the initials commit. Nothing was pushed, tagged, or published.

## Source and tests

- Final source commit and initials commit: `257e8553c27252d314dd0ac414be077ce6767d60` (`refactor: use uniform provider initials`).
- Before the commit, `npm run build`, `npm test`, and `git diff --check` passed. The full working-tree diff contained only the accepted initials changes, three provider SVG deletions, and removal of the now-unused Vite SVG type reference. No Rust or Tauri behavior changed.
- From that commit, `cargo fmt --check` passed.
- `cargo test` initially failed seven localhost socket tests under sandbox restrictions (`Operation not permitted`); the unchanged command passed with local socket permission: 113 tests passed, 0 failed; doctests passed.
- `cargo clippy --all-targets -- -D warnings` passed.
- `cd apps/desktop && npm run build` passed; TypeScript check and Vite production build completed, producing `dist/assets/index-jriKaRkF.js` (SHA-256 `b6e3d89112b7138d498451c1f8e4a0074abeedd06d65f5cc575db3a7aa8c84e7`).
- `cd apps/desktop && npm test` passed: 23 tests across 2 files.
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` passed: 35 tests; doctests passed.
- `cargo build --release --bin cps` passed for CLI compatibility checks; `git diff --check` passed.

## Omitted-provider compatibility

Isolated temporary `CODEX_HOME` fixtures were used for four CLI status cases and direct invocation of the existing Tauri `get_status` path. The Tauri check compiled the existing command and DTO code into a temporary diagnostic harness without changing repository source. Each fixture retained its original bytes and produced no recovery backup.

- `model = "gpt-6-luna"` without top-level `model_provider` resolved to `openai/gpt-6-luna` in CLI and Tauri status.
- Explicit `model_provider = "ollama"` remained `ollama` in both paths.
- Non-string provider (`7`) and blank provider (`""`) were safely rejected in both paths. CLI exited 2 with the appropriate typed error; Tauri returned `config_invalid`.
- The Tauri test `status_defaults_an_omitted_provider_to_openai` passed. The installed app displayed OpenAI active for the actual read-only omitted-provider configuration.

## Final provider marks and frontend

- `ProviderMark` renders initials as text, never `<img>`; no provider SVG imports, bundled provider SVG files, logo-only CSS, OpenAI optical scaling, or runtime provider-logo URL remain.
- The production frontend has all six initials; the fallback test covers an unknown provider (`UN`).
- The universal executable embeds the current hashed frontend asset reference. A smoke launch of an identical universal app copy showed all six initials, OpenAI active, Selected provider, Diagnostics, and Recovery.

## Universal application

- Command: `cd apps/desktop && npm run tauri build -- --bundles app --target universal-apple-darwin --no-sign` — PASS.
- Validated output: `apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/macos/Codex Provider Switcher.app`. Tauri later removes this intermediate `.app` while bundling the DMG; an identical copy was retained for comparison.
- Bundle identifier `tech.yliu.codex-provider-switcher`, version `0.1.0`; executable architecture `x86_64` and `arm64`.
- Executable SHA-256: `3b804a463d114c05623c9c96922d82b6aa3a294b185b023c0607ebd831b1f12b`.
- The smoke-launched copy's running executable came from that copy and its UI showed the final initials layout.

## Final DMG

- Command: `cd apps/desktop && npm run tauri build -- --verbose --bundles dmg --target universal-apple-darwin --no-sign` — PASS after permission retry.
- Output: `apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/dmg/Codex Provider Switcher_0.1.0_universal.dmg`.
- Size: 13,878,059 bytes; SHA-256: `785bf13dc82f364ba6564d7bf969c82816a4e4c94ff394921165ac1d8941daf9`.
- `hdiutil verify` passed (CRC32 `E37AA672`). A read-only mount contained the expected app and `Applications` symlink. The packaged executable SHA-256 matched the validated universal app, referenced the initials-only frontend, and the package contained no provider SVG assets.

## Installed app

- The app already installed at `/Applications/Codex Provider Switcher.app` matched the final validated universal app byte-for-byte across all three bundle files, including the executable SHA-256 above. No replacement or duplicate backup was needed; existing backups were preserved.
- The running process path resolved to the standard Applications app. The UI displayed `DS / LM / OL / OA / OR / XA`, OpenAI active, and the expected Selected provider, Diagnostics, and Recovery sections. Selecting LM Studio updated the Selected provider view; selecting OpenAI restored that view. No model switch, configuration change, diagnostic action, or restore action was performed. The user configuration SHA-256 and backup inventory were unchanged after inspection.

## Signing, sandbox notes, and limitations

- Both builds used `--no-sign`. The Mach-O executable has a linker ad-hoc signature; the app bundle has no sealed resource signature, Developer ID identity, or Team ID. The DMG is unsigned. No notarization or stapling occurred.
- The first sandboxed DMG build failed at `hdiutil create` with `Device not configured`. The exact same command succeeded with disk-image permission. The initial `cargo test` socket restriction and unchanged permission retry are recorded above.
- Physical Intel-machine execution, older macOS versions, live provider API/Keychain operations, actual model switching, and a restore operation remain unverified. The universal Intel slice was verified with `lipo`.
