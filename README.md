# `sprawl-guard`

`sprawl-guard` keeps source trees navigable by enforcing code-line and directory fan-out limits.
It supports every language bundled with Tokei and can baseline existing violations without letting
them grow.

Install with Cargo:

```sh
cargo install sprawl-guard
sprawl-guard languages --detected
sprawl-guard init
sprawl-guard check
```

For a project-local dependency, use npm (or pnpm, etc.). The npm package requires Node.js 22 or
newer:

```sh
npm install --save-dev @kpozin/sprawl-guard
npm exec -- sprawl-guard languages --detected
npm exec -- sprawl-guard init
npm exec -- sprawl-guard check
```

Both installations provide the same CLI.

For usage, see the [documentation](https://kpozin.github.io/sprawl-guard/). Start with the [quick
start](https://kpozin.github.io/sprawl-guard/quick-start/) or the [existing-repository
guide](https://kpozin.github.io/sprawl-guard/adopt-existing-repository/).

## Local development

You need a current stable Rust toolchain, Node.js 24, and pnpm 11.7.0. Fetch the dependencies:

```sh
cargo fetch --locked
pnpm --dir npm install --frozen-lockfile
```

Run the core checks:

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
pnpm --dir npm run check
```

## Links

- [Documentation](https://kpozin.github.io/sprawl-guard/)
- [Releases](https://github.com/kpozin/sprawl-guard/releases)
- [Issues](https://github.com/kpozin/sprawl-guard/issues)
- [Report a security vulnerability](https://github.com/kpozin/sprawl-guard/security/advisories/new)

## License

Copyright (c) 2026 Konstantin Pozin.

`sprawl-guard` is dual-licensed under either:

- MIT
- Apache-2.0

at your option.
