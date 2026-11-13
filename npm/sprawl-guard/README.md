# @kpozin/sprawl-guard

`sprawl-guard` checks that source files stay short and directories stay small. It records existing
violations in a ratchet file so they can shrink but not grow.

Every language bundled with [Tokei](https://github.com/XAMPPRocky/tokei) is enabled by default.
Sprawl Guard counts the lines in each file and the source files directly inside each directory.

## Install

Add the CLI as a project-local development dependency. The package requires Node.js 22 or newer:

```sh
npm install --save-dev --save-exact @kpozin/sprawl-guard
```

## Quick start

From the repository root:

```sh
npm exec -- sprawl-guard init --write
npm exec -- sprawl-guard check
```

`init --write` creates `sprawl-guard.toml`. Run `init` without `--write` to preview it first. A
passing check prints:

```text
Configuration: sprawl-guard.toml

No sprawl-guard violations found.
Checked 2 source files in 2 directories across 2 languages.
```

If an existing repository exceeds the limits you want, keep the limits and
[record the current violations in the ratchet file](https://kpozin.github.io/sprawl-guard/quick-start/#record-existing-violations).

## Documentation

- [Get started](https://kpozin.github.io/sprawl-guard/quick-start/)
- [Configure checks](https://kpozin.github.io/sprawl-guard/configure-checks/)
- [Run in CI](https://kpozin.github.io/sprawl-guard/run-in-ci/)
- [Work with the ratchet](https://kpozin.github.io/sprawl-guard/ratchet/)
- [Agent skill for coding agents](https://github.com/kpozin/sprawl-guard/blob/main/skills/sprawl-guard/SKILL.md)
  and [how to install it](https://kpozin.github.io/sprawl-guard/ratchet/#guide-coding-agents)
- [CLI and output reference](https://kpozin.github.io/sprawl-guard/reference/cli-and-output/)
- [Source and issue tracker](https://github.com/kpozin/sprawl-guard)
