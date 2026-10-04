# `sprawl-guard`

`sprawl-guard` enforces configurable limits on logical lines of code (LOC) and directory fan-out to
keep source trees navigable. It supports every language bundled with
[Tokei](https://github.com/XAMPPRocky/tokei) and records existing violations as ceilings so they can
shrink but not grow.

Install with Cargo:

```sh
cargo install sprawl-guard
sprawl-guard init
sprawl-guard languages --detected
sprawl-guard check
```

To install the CLI as a project-local development dependency, use npm or another Node package
manager. The npm package requires Node.js 22 or newer:

```sh
npm install --save-dev @kpozin/sprawl-guard
npm exec -- sprawl-guard init
npm exec -- sprawl-guard languages --detected
npm exec -- sprawl-guard check
```

Both installations provide the same CLI. Run `sprawl-guard init` to create a versioned policy file
without scanning repository sources, then use `languages --detected` and `check` for explicit
repository analysis. Commit the policy alongside your source code.

Start with the [quick start](https://kpozin.github.io/sprawl-guard/quick-start/) or the
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
