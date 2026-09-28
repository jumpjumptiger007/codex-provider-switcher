# Codex Provider Switcher

Codex Provider Switcher (`cps`) is a local, macOS-oriented CLI for safely switching the provider and model selected by Codex. CPS manages local Codex configuration and credentials; it is not a proxy or an always-running gateway.

## Supported providers

The registry includes these provider types:

- Native, Codex-managed: OpenAI, Ollama, LM Studio.
- Direct Responses API: DeepSeek, xAI, OpenRouter.

The registry records DeepSeek as verified-basic and xAI and OpenRouter as unverified. Native provider entries are marked verified by the registry, while their individual model capabilities remain unknown. Registry status is not a claim that every model has been tested or will work.

## Commands

```text
cps list
cps auth <provider>
cps models [provider]
cps use <provider/model>
cps status
cps doctor
cps restore
```

`auth` stores credentials for a registered direct Responses provider. `models` lists advertised IDs for a direct provider; omit the provider to use the active configured provider. Native providers are managed by Codex and do not use CPS credential storage or CPS model discovery.

A typical setup is to list providers, store a direct-provider credential, inspect its advertised models, switch to one of them, and run the local diagnostics:

```sh
cps list
cps auth openrouter
cps models openrouter
cps use openrouter/openai/<model>
cps status
cps doctor
```

OpenRouter model IDs can contain slashes and provider-like names. The first path segment selects the CPS provider; the remaining model ID is preserved as-is. Replace `<model>` with an ID returned by `cps models openrouter`.

## Local development

Build the executable from this Rust project:

```sh
cargo build --release
```

To install the binary into Cargo's user-level bin directory:

```sh
cargo install --path .
```

## Desktop macOS app

The single-window Tauri 2 Desktop app uses the same CPS core. With Node.js,
Rust, and the macOS build tools available, build a local unsigned release app:

```sh
cd apps/desktop
npm install
npm run tauri build -- --bundles app --no-sign
```

The native build produces
`apps/desktop/src-tauri/target/release/bundle/macos/Codex Provider Switcher.app`
(relative to the repository root). The frontend is embedded locally; running
the app requires neither a development server nor Node.js.

With both `aarch64-apple-darwin` and `x86_64-apple-darwin` Rust targets installed:

```sh
npm run tauri build -- --bundles app --target universal-apple-darwin --no-sign
```

The universal app is produced under
`apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/macos/`.
These commands create only `.app` bundles. Signed/notarized DMG distribution
has not been performed.

RC1 uses the existing development artwork (`icons/icon.png`) and a macOS
`icons/icon.icns` generated from it. **FINAL APP ICON SOURCE REQUIRED**:
original final CPS artwork must be supplied before distribution.

## Security and recovery

CPS-owned direct-provider credentials are stored in macOS Keychain and are not written into Codex configuration. Secret values are redacted from diagnostic formatting. For direct providers, Codex retrieves the credential through a command configured by CPS.

CPS config writes use an atomic transaction with conflict detection. Successful mutations create a recovery backup. `cps restore` restores the newest unambiguous CPS backup and creates a new backup of the current config, so a restore can itself be reversed. If the config changes externally during a CPS write or restore, conflict detection protects that external change.

## Local diagnostics

`cps doctor` checks local configuration, provider setup, CPS credential presence, and recovery readiness without contacting providers. A present credential means only that CPS has one stored locally. Doctor does not verify API key validity with a provider, network connectivity, billing, rate limits, or model availability.

## Architecture

```text
CLI
 ↓
Application/Core
 ├─ Provider Registry
 ├─ Config Transactions
 ├─ Credential Store
 └─ Model Discovery
```

The Rust core and application APIs are separate from CLI parsing and output, so another frontend can reuse them.

## Scope

Bridge and LiteLLM support are not part of the current MVP. They may be considered as a future extension, with no commitment implied.
