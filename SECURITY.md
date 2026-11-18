# Security policy

## Supported versions

Security fixes are provided for the latest published version of `sprawl-guard`.

## Verify a release

The release workflow signs a provenance attestation for each file it publishes: the archives on
the GitHub release page, the crates on crates.io, and the npm packages. An attestation records the
repository, workflow, and commit that built the file, so you can confirm that a download came from
this repository's release workflow and was not changed afterwards. Versions 0.3.0-dev.0 and
earlier were published without attestations.

To verify a file, install the [GitHub CLI](https://cli.github.com/) and sign in with
`gh auth login`. Download the file, then run this command, replacing `FILE` with its name:

```sh
gh attestation verify FILE --repo kpozin/sprawl-guard \
  --signer-workflow kpozin/sprawl-guard/.github/workflows/release.yml
```

Download each kind of file as follows, replacing `VERSION` with the version you installed:

- Crate: `curl -LO https://static.crates.io/crates/sprawl-guard/sprawl-guard-VERSION.crate`
- npm package: `npm pack @kpozin/sprawl-guard@VERSION`. Also download and verify the platform
  package that npm installed with it, such as `@kpozin/sprawl-guard-linux-x64-gnu`.
- Release archives, including the one Homebrew installs:
  `gh release download vVERSION --repo kpozin/sprawl-guard`. Verify the archive for your platform,
  such as `sprawl-guard-VERSION-aarch64-apple-darwin.tar.gz`.

## Report a vulnerability

Do not open a public issue for a suspected vulnerability. Use
[GitHub's private vulnerability reporting form](https://github.com/kpozin/sprawl-guard/security/advisories/new)
and include the affected version, impact, reproduction steps, and any suggested mitigation.
