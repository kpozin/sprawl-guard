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

To release a version, tag the commit on `main` that carries it as `v<version>` and run the
`Release` workflow from that tag, replacing `VERSION` with the version:

```sh
gh workflow run release.yml --ref vVERSION -f release_mode=publish
```

The workflow picks the npm channel from the version: `latest` for a stable version, and `beta` or
`rc` for those prereleases. Choose `npm_channel` yourself for other prereleases. The `draft` mode
builds and checks everything and creates a draft GitHub Release without publishing to any registry,
which is useful after changing the workflow. In `publish` mode, the publish jobs wait for approval
in the `release` environment, and one approval covers crates.io, npm and PyPI. When the registry
checks pass, publish the draft GitHub Release. If a publish job fails, fix the cause and re-run the
failed jobs: the crates.io and npm jobs skip versions they already published.

The Homebrew formula lives in the [kpozin/homebrew-tap](https://github.com/kpozin/homebrew-tap)
repository and installs the release's prebuilt binary on macOS and Linux. When you publish a stable
GitHub Release, the `Homebrew` workflow waits for approval in the `release` environment, then starts
the tap's `Bump` workflow. If the release is GitHub's latest release, `Bump` opens a pull request in
the tap that moves the formula to the new version; a stable release on another npm channel leaves
the formula alone. The tap has no CI, so install and test the pull request locally before you merge
it, replacing `NUMBER` with its number:

```sh
cd "$(brew --repository kpozin/tap)"
gh pr checkout NUMBER
brew reinstall kpozin/tap/sprawl-guard
brew test kpozin/tap/sprawl-guard
git checkout main
```

Merging the pull request ships the release to Homebrew users.

The workflow reads the `HOMEBREW_TAP_TOKEN` secret of the `release` environment: a fine-grained
personal access token with read and write access to the tap's actions, and no other permissions or
repositories. It can start the tap's workflows but can't change the formula. The tap's `Bump`
workflow opens the pull request with its own token, which needs the tap's "Allow GitHub Actions to
create and approve pull requests" setting. The personal access token expires; when it does, the
workflow fails with a 401 error from GitHub, so create a new token and replace the secret. If the
`Homebrew` workflow fails, start the tap's workflow yourself, replacing `VERSION` with the
release's version:

```sh
gh workflow run bump.yml --repo kpozin/homebrew-tap --field tag=vVERSION
```

If the tap's `Bump` workflow fails, read its log, then open the pull request from your own machine:

```sh
cd "$(brew --repository kpozin/tap)"
git switch main
git pull --ff-only
git switch --create bump-sprawl-guard-VERSION
bin/bump-sprawl-guard vVERSION
git commit --message "sprawl-guard VERSION" Formula/sprawl-guard.rb
git push origin bump-sprawl-guard-VERSION
gh pr create --fill
```

[SchemaStore](https://www.schemastore.org/) maps `sprawl-guard.toml` and
`sprawl-guard.ratchet.toml` to one schema URL each, and it can't read `schema_version` from the
file. When a release adds a new configuration or ratchet schema version, wait for the release's
Pages deployment to succeed and check that the new URL serves the schema. Then open a pull request
to SchemaStore that points only the affected entry at the new URL.
