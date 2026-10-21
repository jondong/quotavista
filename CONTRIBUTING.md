# Contributing to QuotaVista

QuotaVista has reviewed requirements and architecture and a Trellis-managed prototype backlog. The Phase 1 prototype now integrates a four-screen Tauri 2 desktop app over a SQLite backend with synthetic providers only: overview, statistics, connections, and settings screens, bilingual (en/zh-CN) catalogs, session-only or platform credential stores, and lifecycle-aware refresh. Real provider integrations and release acceptance are not implemented; see [traceability](docs/traceability.md) for what the synthetic evidence does and does not cover.

Read [task management](docs/task-management.md) and the [Phase 1 plan](docs/phase-one-plan.md) before implementation. Initialize your local Trellis identity, select an existing task, verify its prerequisites, and follow its acceptance criteria.

## Development setup

The prototype is a Cargo workspace plus a Vite/React desktop frontend. All development data is synthetic; no provider credential or QuotaVista account is needed.

### Toolchain (pinned)

- Rust `1.99.1` via [rust-toolchain.toml](rust-toolchain.toml) (rustup installs it automatically, including `rustfmt` and `clippy`).
- Node.js `24` via [.nvmrc](.nvmrc) with an engines floor of `>=22.12.0` (the lowest release compatible with Vite 7, so every advertised Node works) and npm `>=11`, selected as `npm@11.19.1` in `apps/desktop/package.json` `packageManager`. `apps/desktop/.npmrc` sets `engine-strict=true`, so npm errors on incompatible Node/npm instead of warning.
- Native prerequisites follow the [official Tauri 2 guide](https://v2.tauri.app/start/prerequisites/):
  - macOS: Xcode Command Line Tools (`xcode-select --install`).
  - Ubuntu/Debian: `sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`.

### Install from lockfiles

Provision the pinned npm once (and whenever `packageManager` changes); plain npm does not select its own version from `packageManager`:

```sh
npm install -g "npm@$(node -p "require('./apps/desktop/package.json').packageManager.split('@')[1]")"
```

Run everything from the repository root:

```sh
(cd apps/desktop && npm ci)             # frontend, from package-lock.json
cargo fetch                             # Rust, from the committed Cargo.lock
```

### Checks and builds

Run everything from the repository root:

```sh
./scripts/check-core-boundaries.sh                           # core dependency direction
./scripts/test-check-core-boundaries.sh                      # boundary gate regression checks
cargo fmt --all -- --check                                   # formatting
cargo clippy --locked --workspace --all-targets -- -D warnings  # lints
cargo test --locked --workspace                              # Rust tests
cargo run -p quotavista-contracts --bin generate-contracts -- --check  # Rust/TypeScript contract drift
(cd apps/desktop && npm run lint)                            # ESLint
(cd apps/desktop && npm run i18n:check)                      # en/zh-CN catalog key parity
(cd apps/desktop && npm test)                                # renderer unit/component tests (Vitest)
(cd apps/desktop && npm run typecheck)                       # strict TypeScript
(cd apps/desktop && npm run build)                           # production frontend build
(cd apps/desktop && npm run tauri dev)                       # launch the prototype window (synthetic data)
```

CI (`.github/workflows/ci.yml`) runs these checks without credentials. A green CI build is not evidence of a native Linux run; platform behavior needs separate validation evidence.

### Local query benchmark (P1-12)

The integration benchmark seeds ninety settled days of synthetic usage history — exactly 100,000 facts, the storage bound for one query — and measures the statistics/overview query mix through the same validated request path the renderer uses. It is env-gated so the default suite stays fast:

```sh
QUOTAVISTA_BENCHMARK=1 cargo test --locked -p quotavista-desktop statistics_query_mix_benchmark -- --nocapture
```

It writes a JSON artifact to `apps/desktop/src-tauri/target/p1-12-benchmark.json` (cargo sets the test working directory to the package root; override with `QUOTAVISTA_BENCHMARK_OUT`) recording five warmups, thirty measured queries, per-mix and overall p50/p95/max against the 1-second prototype target, cancellation behavior, and failures. A target miss is recorded in the artifact, not asserted away.

### SQLite note

SQLx is pinned to `0.9.0` and `libsqlite3-sys = 0.37.0` (bundled) links SQLite `3.51.3`; startup and tests assert the linked version, and the WAL-reset fix is included. Do not enable SQLx's unbundled SQLite feature.
## Documentation language

Write all repository documentation in English so the project is accessible to a global audience. This applies to README files, requirements, architecture decisions, development plans, installation instructions, contributor guidance, and diagram labels.

Preserve official provider and product names. Keep examples and explanations understandable without access to private discussions. The first-release UI supports English and Simplified Chinese; documentation and task artifacts remain English.

## Proposing changes

- Use GitHub Issues to describe a use case, missing capability, or problem.
- Keep changes focused and update related documentation when scope or behavior changes.
- Identify assumptions and distinguish planned behavior from validated functionality.
- For provider work, document permissions, measurement units, historical coverage, and unsupported capabilities.
- Never include real keys, tokens, cookies, or identifying account data in issues, fixtures, logs, or commits.

## Validation

For documentation changes, check local links, code fences, diagram labels, and consistency with the requirements. Run git diff --check before submitting.

Once the application exists, follow the documented checks relevant to the change. Do not claim platform support from compilation alone or real provider integration from demo data.

## License

Original application and documentation contributions are made under the project's [MIT License](LICENSE). Retain notices required for third-party material and record its source. Changes to installed Trellis tooling must retain its [upstream terms](.trellis/NOTICE.md).
