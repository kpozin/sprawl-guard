# `sprawl-guard`

`sprawl-guard` checks that source files stay short and directories stay small. It supports every
language bundled with [Tokei](https://github.com/XAMPPRocky/tokei) and records existing violations
in a ratchet file so they can shrink but not grow.

## Install

Install with Cargo:

```sh
cargo install sprawl-guard
```

Or install it as a project-local development dependency with npm or another Node package manager.
The npm package requires Node.js 22 or newer:

```sh
npm install --save-dev @kpozin/sprawl-guard
```

## Quick start

From the repository root:

```sh
sprawl-guard init --write
sprawl-guard check
```

With npm, run the same commands through `npm exec -- sprawl-guard`. `init --write` creates
`sprawl-guard.toml`; commit it alongside your source code. Run `init` without `--write` to preview
it first.

Continue with the [quick start](https://kpozin.github.io/sprawl-guard/quick-start/) or the
[existing-repository guide](https://kpozin.github.io/sprawl-guard/adopt-existing-repository/).

## Links

- [Documentation](https://kpozin.github.io/sprawl-guard/)
- [Build and test from source](docs/DEVELOPMENT.md)
- [Releases](https://github.com/kpozin/sprawl-guard/releases)
- [Issues](https://github.com/kpozin/sprawl-guard/issues)
- [Report a security vulnerability](https://github.com/kpozin/sprawl-guard/security/advisories/new)

## License

Copyright (c) 2026 Konstantin Pozin.

`sprawl-guard` is dual-licensed under either:

- MIT
- Apache-2.0

at your option.
