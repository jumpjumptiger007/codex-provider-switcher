# CPS v0.1.0 Desktop Distribution RC1

**EXECUTED — universal ad-hoc signed DMG built and verified. Stop for independent review.**

## Baseline and scope

| Item | Result |
| --- | --- |
| Accepted branch | `main` |
| Accepted baseline / current HEAD | `8ad7c457ebc2f40a3ccac6cb860fda19dc2c4d5c` |
| Starting working tree | Clean |
| Distribution branch | `codex/distribution-rc1` |
| Remote / upstream | None; none added |
| Commit / push / tag / GitHub release | None |

Only the requested README and Tauri bundle configuration changes were made,
plus this report. Product name (`Codex Provider Switcher`), version (`0.1.0`),
bundle identifier (`tech.yliu.codex-provider-switcher`), icon assets, app code,
tests, provider behavior, credentials, config mutation behavior, command
surface, capabilities, and window architecture remain unchanged.

## Build and candidate

- Installed Tauri CLI: **2.12.0**.
- Configuration: `apps/desktop/src-tauri/tauri.conf.json` sets
  `bundle.macOS.signingIdentity` to `"-"`. No entitlements or explicit
  hardened-runtime/notarization settings were added.
- Successful command, run from `apps/desktop`:

  ```sh
  npm run tauri build -- --verbose --bundles dmg --target universal-apple-darwin
  ```

  The requested universal DMG options were unchanged; `--verbose` was added
  after two non-verbose attempts ended at Tauri's generated DMG script without
  diagnostic output. The verbose run completed the standard Tauri packaging
  flow, including image creation, Finder layout, clean unmount, and compression.
- DMG: `apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/dmg/Codex Provider Switcher_0.1.0_universal.dmg`
- Size: **14,043,583 bytes** (13.39 MiB as reported by Tauri).
- SHA-256: `d78f802eac7d8cf20841df818be410a1e5a59771225e0faf7e103a648268025f`
- Container: UDZO read-only zlib-compressed disk image, HFS+ filesystem when
  mounted. `hdiutil verify` passed; the image CRC32 was `$C418A089`.

Mounted contents were:

```text
.DS_Store
.VolumeIcon.icns
Applications -> /Applications
Codex Provider Switcher.app
```

The app and standard Applications installation link are present. The volume
icon is Tauri's default icon generated from the accepted CPS icon; there is no
custom background artwork or additional payload. The app bundle contains only
`Contents/Info.plist`, `Contents/MacOS/cps-desktop`,
`Contents/Resources/icon.icns`, and the signature seal
`Contents/_CodeSignature/CodeResources`.

Bundle metadata reports the expected display name, bundle identifier, version,
executable name `cps-desktop`, icon `icon.icns`, and package type `APPL`.
`file` and `lipo -info` on the executable from the mounted DMG confirmed both
**x86_64** and **arm64** architectures. The image was mounted read-only for
inspection and unmounted cleanly.

## Signature, Gatekeeper, and notarization

`codesign -dv --verbose=4` on the app from the final DMG reported:

- `Signature=adhoc`
- `TeamIdentifier=not set`
- no `Authority` / Developer ID signing identity
- CodeDirectory flags `adhoc,runtime`
- the installed Tauri bundler invoked `codesign --options runtime`; this was
  its build behavior, not an added Tauri hardened-runtime configuration.

`codesign --verify --deep --strict --verbose=2` passed: the bundle was valid on
disk and satisfied its designated requirement. This checks the ad-hoc signature
structure; it is not Apple identity verification.

Read-only `spctl --assess --type execute --verbose=4` on the app mounted from
the final DMG returned **`rejected` (exit 3)**. That result is expected for this
intentionally ad-hoc signed, unnotarized distribution. No Gatekeeper setting,
quarantine attribute, or system security control was changed.

Tauri logged that app notarization was skipped because no notarization
credentials were available. Read-only `xcrun stapler validate -v` reported that
neither the app nor the DMG has a ticket stapled to it (exit 65 for each).
Nothing was submitted to Apple; no Apple account or signing identity was
accessed, and no Apple credentials were set or used.

```text
Code signature: ad-hoc
Apple identity verification: none
Notarization: none
Stapling: none
```

## Isolated installation smoke test

The app was copied from the mounted DMG into an isolated temporary application
directory, then launched with `CODEX_HOME=<temporary-directory>/CODEX_HOME`.
That isolated home contained only a test `config.toml` selecting
`openai/gpt-5.4`.

- Current setup rendered as OpenAI `openai/gpt-5.4`.
- All six providers rendered; selecting LM Studio changed the selected provider.
- Doctor ran locally and reported no blocking issues, a valid config, the
  expected selection, and the native OpenAI provider.
- Restore confirmation rendered; Cancel dismissed it and returned the UI to idle.
- The app exited normally. No temporary app process remained.
- The isolated config's SHA-256 was identical before and after; no recovery
  backup or other fixture file was created.
- No credential was saved, no provider API was called, and no switch or restore
  was performed. The existing Applications copy, if present, was left untouched.

## Bundle and authority regression

The registered Tauri commands and capability manifest remain exactly these
eight CPS commands:

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

No generic shell, filesystem, process, frontend HTTP, or updater permission is
present. The Desktop Rust manifest adds no Tauri plugins. The bundled app has no
CLI binary, sidecar, updater, `.env`, credential file, test fixture, source map,
or unexpected runtime resource. Frontend distribution assets are limited to
`index.html` and the hashed JavaScript and CSS bundles. The app's `icon.icns`
and Tauri's DMG volume icon both match the accepted repository icon.

## Validation

| Validation | Result |
| --- | --- |
| Root `cargo fmt --check` | PASS |
| Root `cargo test` | PASS — 111 tests |
| Root `cargo clippy --all-targets -- -D warnings` | PASS |
| Desktop `npm install` | PASS — dependencies up to date; lockfile unchanged |
| Desktop `npm run build` | PASS |
| Desktop `npm test` | PASS — 16 tests |
| Desktop Rust `cargo fmt --check --manifest-path apps/desktop/src-tauri/Cargo.toml` | PASS |
| Desktop Rust `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | PASS — 34 tests |
| Desktop Rust `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings` | PASS |
| `git diff --check` | PASS |
| DMG build, mount, structure, checksum, architecture, signature, smoke | PASS |

The first root test attempt could not bind loopback sockets in the restricted
shell; a scoped local-validation retry passed all 111 tests. No test was changed
or weakened.

The report was checked for user-specific absolute paths, usernames, concrete
temporary paths, account details, and credentials; none are included.

## Git state and limitations

Generated `target/`, `dist/`, `node_modules/`, app, and DMG outputs are ignored
and uncommitted. Expected source changes are `README.md`,
`apps/desktop/src-tauri/tauri.conf.json`, and this report. HEAD remains the
accepted baseline and the index is empty. No commit, push, tag, remote/upstream,
GitHub release, or GitHub Actions workflow was created.

This candidate has no Apple identity verification, notarization, or stapled
ticket. macOS can block its first launch; the README documents Apple's
per-app **Open Anyway** workflow. The Intel slice was architecture-verified,
but execution on an Intel Mac and compatibility with other macOS releases were
not tested.
