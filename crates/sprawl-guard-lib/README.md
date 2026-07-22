# sprawl-guard-lib

`sprawl-guard-lib` is the implementation crate behind the `sprawl-guard` CLI.

The library API is not stable. Prefer the CLI unless you can pin an exact library version and adapt
to breaking changes.

## WASI

A workspace that uses this crate on the WebAssembly System Interface (WASI) must override Tokei to
use a compatible `etcetera` 0.11.x release. Cargo reads dependency patches only from the consuming
workspace root. Native builds do not require this override.

WASI does not support `traversal.same_file_system`. The crate accepts the setting but ignores it on
WASI; native builds enforce it when set to `true`. A WASI runtime can limit exposed host paths with
preopens, but a preopen does not prevent traversal into mounted filesystems below the exposed root.

See the [project README](https://github.com/kpozin/sprawl-guard#readme) for CLI usage.
