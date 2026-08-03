# `sprawl-guard`

`sprawl-guard` keeps source trees navigable with configurable LOC and directory fan-out limits. It
supports every language bundled with [Tokei](https://github.com/XAMPPRocky/tokei) and can record
existing violations so they can shrink but not grow.

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

Both installations provide the same CLI. Run `sprawl-guard init` to create a versioned policy file,
then commit the policy alongside your source code.

For usage, see the [documentation](https://kpozin.github.io/sprawl-guard/). Start with the [quick
start](https://kpozin.github.io/sprawl-guard/quick-start/) or the [existing-repository
guide](https://kpozin.github.io/sprawl-guard/adopt-existing-repository/).

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
