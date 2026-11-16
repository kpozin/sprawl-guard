# sprawl-guard-lib

`sprawl-guard-lib` implements the `sprawl-guard` CLI. Its library API is unstable. For normal use,
install the CLI instead. If you depend on the library directly, pin an exact version and expect
breaking API changes.

## WASI

A workspace that uses this crate on the WebAssembly System Interface (WASI) must override Tokei to
use a compatible `etcetera` 0.11.x release. Cargo reads dependency patches only from the consuming
workspace root. Native builds do not require this override.

On WASI Preview 2 (`wasm32-wasip2`), the crate ignores `traversal.same_file_system`, because its
file status has no device number. WASI Preview 1 and native builds enforce it.

See the [project README](https://github.com/kpozin/sprawl-guard#readme) for CLI usage.
