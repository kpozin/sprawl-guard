# tokei 15.0.0 local copy

This directory vendors the published `tokei 15.0.0` crate from crates.io.

The initial vendored copy is intentionally unmodified except for omitting Cargo's local registry
marker file. Local changes must be documented in this file with:

- the reason the change exists;
- the upstream issue or pull request, when available;
- the expected removal condition.

## etcetera 0.11

`tokei 15.0.0` depends on `etcetera 0.8`, which depends on `home`. The `home` crate does not
build for the WASI target used by the sprawl-guard npm fallback binary.

This local patch changes `tokei`'s `etcetera` dependency to `0.11.0`, matching upstream work in
<https://github.com/XAMPPRocky/tokei/pull/1253>. The patch can be removed when a published `tokei`
release includes the `etcetera` upgrade or otherwise supports the WASI build.

The crate's own `Cargo.lock` is retained as upstream provenance. The active workspace resolver
state is recorded in the repository lockfiles that consume this path patch.
