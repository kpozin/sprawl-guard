# `@kpozin/sprawl-guard`

This package provides the `sprawl-guard` command for Node.js projects. It requires Node 22 or newer.

The wrapper runs the installed native package for the current platform. If no native package is
available, it runs the WebAssembly System Interface (WASI) module from the installed
`@kpozin/sprawl-guard-wasm32-wasip1` package.

The WASI fallback preopens only the selected project root. This limits which host paths the module
can access, but it is not a security sandbox and does not block traversal into mounted filesystems
below that root. WASI accepts the `traversal.same_file_system` setting for configuration
compatibility but cannot enforce it; native builds enforce it when set to `true`.

```sh
pnpm add --save-dev @kpozin/sprawl-guard
```

See the [project README](https://github.com/kpozin/sprawl-guard#readme) for configuration, commands,
CI setup, and ratchet workflows.
