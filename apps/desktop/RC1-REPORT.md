# CPS Desktop Finalization RC1

EXECUTED — native and universal unsigned `.app` builds passed local validation.
Final icon artwork and independent review are still required before distribution.

Verified on 2026-09-28, on an arm64 Mac running macOS 15.7.4.

| Git state | Result |
| --- | --- |
| Accepted starting branch | `main` |
| Baseline / current HEAD | `1ee71e831a41d022fbf7ea71bb9ae0e57cd416db` |
| Starting working tree | Clean, exact baseline verified before editing |
| Working branch | `codex/desktop-finalization-rc1` |
| Remote / upstream | None; none added |
| Index | Empty; no files staged |
| Final working tree | Five modified files and two new files, listed below |

Changed/new files:

- `README.md`: unsigned native/universal build instructions, output locations,
  temporary icon notice, and distribution status.
- `apps/desktop/package.json`: concise product description.
- `apps/desktop/src-tauri/Cargo.toml`: same product description replaces scaffold wording.
- `apps/desktop/src-tauri/tauri.conf.json`: enable app-only bundling, utility category,
  explicit icon resources, and default window height.
- `apps/desktop/src/styles/app.css`: readable disabled controls, no disabled-button
  hover styling, wrapping workflow headings/messages, and shrinkable/wrapping
  Doctor finding text.
- `apps/desktop/src-tauri/icons/icon.icns` (new): macOS packaging of the existing
  development artwork.
- `apps/desktop/RC1-REPORT.md` (new): this review record.

No business functionality changed. Core/CLI source, provider registry, credentials,
Keychain namespace, model discovery, transactions, conflicts, backups, restore,
Doctor semantics, Tauri command contracts, and Desktop error contracts are unchanged.
No frontend TypeScript or Rust implementation changed. Existing tests are unchanged.

| Release configuration | Value |
| --- | --- |
| Product name | `Codex Provider Switcher` |
| Identifier | `tech.yliu.codex-provider-switcher` |
| Description (Desktop npm and Rust packages) | Switch Codex providers and models securely from a native macOS app. |
| `Cargo.toml` version | `0.1.0` |
| `apps/desktop/package.json` version | `0.1.0` |
| `apps/desktop/src-tauri/Cargo.toml` version | `0.1.0` |
| `apps/desktop/src-tauri/tauri.conf.json` version | `0.1.0` |
| Bundle | `active: true`, `targets: ["app"]`, `category: "Utility"` |
| Icons | `icons/icon.icns`, `icons/icon.png` |
| Window | One window; 760 × 800, previously 760 × 620 |
| Minimum window | Unchanged: 360 × 420 |

The extra 180px makes the native model input and switch button visible at the
default size. Diagnostics and Recovery remain accessible by normal vertical
scrolling. The existing responsive grid and single-window workflow are retained;
no tray, login item, navigation framework, or additional window was added.
Default and approximately 360px-wide layouts were inspected in the release app.
Long native model input, a 342-character Doctor model ID, disabled controls, and
keyboard focus rendered without horizontal layout overflow. Restore filenames
retain their existing `overflow-wrap: anywhere` rule; actual restore-result layout
was not exercised in this smoke test.

Icon status: **TEMPORARY — FINAL APP ICON SOURCE REQUIRED**. The only artwork in
the repository is the 128px development PNG introduced by the scaffold commit.
No intentional final CPS source artwork was found. No new branding or third-party
artwork was introduced. Tauri CLI generated assets into a temporary directory:

```sh
npm run tauri -- icon src-tauri/icons/icon.png --output <temporary-directory>/cps-rc1-development-icons
```

Only the generated `icon.icns` was copied into the repository. macOS `iconutil`
successfully decoded it, and both bundles contain the expected ICNS resource.
This does not qualify the development artwork as a final distribution icon.

Tauri CLI 2.12.0 locally reported and accepted these exact unsigned commands,
run from `apps/desktop`:

```sh
npm run tauri build -- --bundles app --no-sign
npm run tauri build -- --bundles app --target universal-apple-darwin --no-sign
```

| Build | Result / executable architecture |
| --- | --- |
| Host | `uname -m`: `arm64` |
| Native | PASS; `file` / `lipo`: thin `arm64`; bundle 13.80 MiB |
| Universal | PASS; `file` / `lipo`: `x86_64 arm64`; bundle 27.63 MiB |

Exact native output:

```text
apps/desktop/src-tauri/target/release/bundle/macos/Codex Provider Switcher.app
```

Exact universal output:

```text
apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/macos/Codex Provider Switcher.app
```

Initially only `aarch64-apple-darwin` was installed. The missing
`x86_64-apple-darwin` standard target was installed through normal rustup access,
as permitted by this task. The first universal attempt was blocked by sandbox
network access while fetching an existing target-specific crate. A tool-approved
retry completed successfully. No Rosetta build script, external binary, new
project dependency, or architecture strategy was introduced.

Relevant effective Info.plist values are identical in both bundles:

| Key | Value |
| --- | --- |
| `CFBundleName` / `CFBundleDisplayName` | `Codex Provider Switcher` |
| `CFBundleIdentifier` | `tech.yliu.codex-provider-switcher` |
| `CFBundleShortVersionString` / `CFBundleVersion` | `0.1.0` |
| `CFBundleExecutable` | `cps-desktop` |
| `CFBundleIconFile` | `icon.icns` |
| `CFBundlePackageType` | `APPL` |
| `LSApplicationCategoryType` | `public.app-category.utilities` |
| `LSMinimumSystemVersion` | `10.13` (Tauri default) |
| `NSHighResolutionCapable` | `true` |

`vtool` reports minimum macOS 11.0 for the ARM64 slice and 10.13 for the x86_64
slice. These values are build metadata, not evidence of testing on older macOS.

Each bundle contains exactly:

```text
Contents/Info.plist
Contents/MacOS/cps-desktop
Contents/Resources/icon.icns
```

Production frontend files (`index.html`, hashed JS, hashed CSS) are embedded in
the executable through Tauri's asset embedding. The app rendered from
`tauri://localhost` with no development server started. No Node executable,
CLI binary, sidecar, extra resource directory, updater framework, `.env`, source
map, credential file, or test fixture was bundled. `otool -L` showed only system
macOS libraries/frameworks. Production frontend checks found no obvious private
key/API-key, source-map, or Vite development-server markers. Info.plist contains
no user-specific absolute paths. Rust source/panic-location strings still include
local project and Cargo source paths in the executable; these are not runtime
resource/configuration paths and were not remapped in RC1.

Both Tauri build logs explicitly skipped signing. Read-only `codesign -dv` found
an automatic linker ad-hoc executable signature, no Team ID, no sealed resources,
and no bound Info.plist. No Developer ID signing operation was performed; these
are unsigned distribution candidates, not signed/notarized releases.

Production capability artifacts were inspected for native, ARM64, and x86_64
release builds. Each grants only this capability to the local `main` window:

```text
get_providers
get_status
get_credential_status
save_credential
get_models
switch_model
run_doctor
restore_config
```

No ninth application command or generic frontend shell/filesystem/process/HTTP
authority was introduced. No generic Tauri plugin was registered. Generated
schemas enumerate available core permissions but do not grant them; the effective
capability still contains exactly the eight application permissions above.

Release-mode smoke tests used the actual bundle executables with:

```text
CODEX_HOME=<temporary-directory>/cps-rc1-smoke
```

- Native app started normally and rendered the isolated OpenAI setup and all six
  providers. Every provider was selected and its native/direct workflow inspected.
- Native model editing enabled the switch button; no switch was executed.
- Direct-provider credential UI and disabled model actions rendered correctly;
  no API key was entered/saved and no model-discovery request was triggered.
- Doctor returned config/selection/provider OK and informational missing backup
  findings. The long isolated model fixture wrapped correctly at minimum width.
- Restore confirmation rendered and was cancelled; no restore was executed.
- Universal app launched successfully on this ARM64 Mac, rendered Current setup,
  ran Doctor, and displayed/cancelled Restore confirmation. The running executable
  path was verified as the universal bundle; a socket inspection found no app
  network sockets.
- Real config hashes remained unchanged. The isolated config hash also remained
  unchanged by UI actions; only the deliberate test-fixture update introduced
  the long model ID. No backup was created. Both smoke-test processes exited 0.
  Native stderr had macOS input-service diagnostics, with no application panic.

| Required validation | Result |
| --- | --- |
| Root `cargo fmt --check` | PASS |
| Root `cargo test` | PASS — 131 tests |
| Root `cargo clippy --all-targets -- -D warnings` | PASS |
| Desktop `npm install` | PASS; lockfile unchanged |
| Desktop `npm run build` | PASS |
| Desktop `npm test` | PASS — 16 tests |
| Desktop Rust `cargo fmt --check --manifest-path apps/desktop/src-tauri/Cargo.toml` | PASS |
| Desktop Rust `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | PASS — 34 tests |
| Desktop Rust `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings` | PASS |
| `git diff --check` | PASS |
| Native and universal unsigned app builds | PASS |

Root tests initially failed because sandbox policy blocked local loopback test
servers. A tool-approved run passed the unchanged suite; tests were not weakened.

UNVERIFIED: Intel-machine execution of the x86_64 slice, older macOS versions,
real API-key/network model discovery, actual production switch/restore, and final
icon artwork. Live provider operations and real-config mutations were deliberately
excluded by the RC1 request. Signing, notarization, and distribution are pending.

Final `git status --short`:

```text
 M README.md
 M apps/desktop/package.json
 M apps/desktop/src-tauri/Cargo.toml
 M apps/desktop/src-tauri/tauri.conf.json
 M apps/desktop/src/styles/app.css
?? apps/desktop/RC1-REPORT.md
?? apps/desktop/src-tauri/icons/icon.icns
```

Generated `target/`, `dist/`, and `node_modules/` outputs remain ignored and were
not staged. No commit, push, tag, remote/upstream, GitHub release, GitHub Actions,
DMG, Developer ID signing, certificate/Apple credential access, notarization,
stapling, or provisioning-profile operation occurred. Stop here for independent
review; no commit has been made.
