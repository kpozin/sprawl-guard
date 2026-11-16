# same-file 1.0.6 local copy

This directory vendors the published `same-file 1.0.6` crate from crates.io. It omits Cargo's
local registry marker file and the crate's `.gitignore`, which would hide the retained
`Cargo.lock` from Git. Local changes must be documented in this file with:

- the reason the change exists;
- the upstream issue or pull request, when available;
- the expected removal condition.

## WASI file identity

`walkdir` uses `same-file` to detect symbolic link loops while it follows directory links. On
WASI, `same-file 1.0.6` compiles a placeholder whose every call fails, so the sprawl-guard npm
fallback binary could not follow directory links at all.

This local patch builds the Unix implementation on WASI, where files also have device and inode
numbers. It reads them with wasi-libc's `fstat`, because the standard library's WASI metadata
extension is unstable. No upstream issue or pull request exists. The patch can be removed when a
published `same-file` release supports WASI.
