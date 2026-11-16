import { TaggedError } from "better-result";
import type {
  AbsolutePath,
  ExecutablePath,
} from "./paths.js";
import type {
  NativePackageName,
  WasmPackageName,
} from "./resolve-package.js";

/** Structured operation labels used in wrapper runtime failures. */
export type RuntimeOperation =
  | "execute native binary"
  | "forward child signal"
  | "instantiate WASM module"
  | "load WASI fallback runner"
  | "open WASI directories"
  | "read WASM module"
  | "resolve package root"
  | "spawn native binary"
  | "start WASI module";

export class OverrideBinaryNotUsableError extends TaggedError(
  "OverrideBinaryNotUsableError",
)<{
  binaryPath: AbsolutePath;
  message: string;
}> {
  constructor(binaryPath: AbsolutePath) {
    super({
      binaryPath,
      message: `SPRAWL_GUARD_BINARY does not point to a usable file: ${binaryPath}`,
    });
  }
}

export class NativeBinaryMissingError extends TaggedError("NativeBinaryMissingError")<{
  packageName: NativePackageName;
  binaryPath: ExecutablePath;
  message: string;
}> {
  constructor(packageName: NativePackageName, binaryPath: ExecutablePath) {
    super({
      packageName,
      binaryPath,
      message: `native package ${packageName} is installed but ${binaryPath} is missing`,
    });
  }
}

export class MissingFallbackPackageError extends TaggedError(
  "MissingFallbackPackageError",
)<{
  packageName: WasmPackageName;
  nativePackageName: NativePackageName | null;
  platform: NodeJS.Platform;
  arch: NodeJS.Architecture;
  message: string;
}> {
  constructor(args: {
    packageName: WasmPackageName;
    nativePackageName: NativePackageName | null;
    platform: NodeJS.Platform;
    arch: NodeJS.Architecture;
  }) {
    const nativeDetail = args.nativePackageName
      ? `expected native package ${args.nativePackageName}`
      : `platform ${args.platform}/${args.arch} has no native package`;

    super({
      ...args,
      message: `${nativeDetail}, and fallback package ${args.packageName} is not installed`,
    });
  }
}

export class RuntimeFailureError extends TaggedError("RuntimeFailureError")<{
  operation: RuntimeOperation;
  message: string;
  cause: unknown;
}> {
  constructor(operation: RuntimeOperation, message: string, cause: unknown) {
    super({
      operation,
      message,
      cause,
    });
  }
}

/** Structured error union returned by wrapper setup and execution helpers. */
export type WrapperError =
  | MissingFallbackPackageError
  | NativeBinaryMissingError
  | OverrideBinaryNotUsableError
  | RuntimeFailureError;
