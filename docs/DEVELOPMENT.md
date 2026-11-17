# Development

The public repository contains the Rust CLI, npm wrapper, documentation site, and release tooling.
Install a stable Rust toolchain, Node.js 24, and pnpm 11.28.11 before building the project.

## Rust CLI

Build and test the native CLI from the repository root:

```sh
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
```

Run `just test-linter-import` to exercise config and ratchet import with the locked ESLint and
Oxlint versions used in CI. On Windows, pass `target/debug/sprawl-guard.exe` to the recipe.

Run the development binary directly through Cargo:

```sh
cargo run --locked --package sprawl-guard -- --version
```

CI checks this repository with the CLI built from it, using `sprawl-guard.toml` and
`sprawl-guard.ratchet.toml` at the repository root. When it reports a violation, split the file or
directory that grew. Run the same check locally:

```sh
cargo run --locked --package sprawl-guard -- check
```

## Bazel

The repository is also a Bazel module. Build and test it, then build the workspace in
`testing/consumer-workspace`, which depends on this module the way other workspaces do:

```sh
bazel test //...
cd testing/consumer-workspace && USE_BAZEL_VERSION="$(cat ../../.bazelversion)" bazel build //...
```

`cargo-bazel-lock.json` and `third_party/rust/tokei-15.0.0-sprawl-guard/BUILD.bazel` are pinned
from `Cargo.lock`. Maintainers regenerate them whenever `Cargo.lock` changes. Bazel doesn't detect
when they're out of date, because their digest covers the pinned dependency graph rather than
`Cargo.lock` itself.

## npm wrapper

Install the frozen workspace dependencies, then build and test the wrapper and release scripts:

```sh
pnpm --dir npm install --frozen-lockfile
pnpm --dir npm run check
```

The npm release contains one wrapper package and platform-specific runtime packages. Native CLI
binaries are compiled on their target platforms; the release workflow then assembles those binaries
into the runtime packages. Building the TypeScript wrapper does not cross-compile native binaries.

## Documentation site

When changing or removing a command or option, regenerate the CLI reference:

```sh
cargo run --locked --package sprawl-guard -- machine generate-cli-reference-json > site/src/generated/cli-reference.json
```

Then review the handwritten command reference, quick start, CI recipes, and coding-agent guidance.
Generated option tables do not validate those instructions. Run affected commands against both the
native CLI and the npm/WASI wrapper.

Install the frozen site dependencies and run its formatting, lint, type, and production-build
checks:

```sh
pnpm --dir site install --frozen-lockfile
pnpm --dir site run check
```

For a local preview, run:

```sh
pnpm --dir site run dev
```

## Release tooling

The checked-in `justfile` exposes the Cargo release preparation and publication commands. GitHub
Actions builds native artifacts on each target platform, assembles the npm packages, and verifies
the release candidates. See the workflows in [`.github/workflows`](../.github/workflows/) for the
complete release graph.
