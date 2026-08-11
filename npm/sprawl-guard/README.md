# @kpozin/sprawl-guard

`sprawl-guard` keeps source trees navigable with configurable LOC and directory fan-out limits. It
can record existing violations so they can shrink but not grow.

Every language bundled with [Tokei](https://github.com/XAMPPRocky/tokei) is enabled by default. File
limits count configurable logical line types, and directory limits count direct source files by
production/test role and language group.

## Install

Add the CLI as a project-local development dependency. The package requires Node.js 22 or newer:

```sh
npm install --save-dev --save-exact @kpozin/sprawl-guard
npm exec -- sprawl-guard --version
```

## Run your first check

From the repository root:

```sh
npm exec -- sprawl-guard languages --detected
npm exec -- sprawl-guard init --dry-run
npm exec -- sprawl-guard init
npm exec -- sprawl-guard check
```

`init` creates a versioned `sprawl-guard.toml` policy file. A passing check prints:

```text
No sprawl-guard violations found.
Checked 2 source files in 2 directories across 2 languages.
```

If an existing repository exceeds the desired limits, keep the policy strict and
[record the current violations in a ratchet](https://kpozin.github.io/sprawl-guard/adopt-existing-repository/).

## Documentation

- [Quick start](https://kpozin.github.io/sprawl-guard/quick-start/)
- [Configure checks](https://kpozin.github.io/sprawl-guard/configure-checks/)
- [Run in CI](https://kpozin.github.io/sprawl-guard/run-in-ci/)
- [CLI and output reference](https://kpozin.github.io/sprawl-guard/reference/cli-and-output/)
- [Source and issue tracker](https://github.com/kpozin/sprawl-guard)
