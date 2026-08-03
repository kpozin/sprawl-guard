import path from "node:path";
import { Result } from "better-result";
import { z } from "zod/v4";
import { MissingFlagValueError, RuntimeFailureError, type WrapperError } from "./errors.js";
import {
  MACHINE_PROTOCOL_VERSION,
  MachineHostPathStyle,
  MachineHostPathStyleSchema,
} from "./machine-contract.js";
import {
  resolveAbsolutePath,
  unsafeHostRootPathFromAbsolutePath,
  type HostRootPath,
} from "./paths.js";

declare const guestWorkspaceRootBrand: unique symbol;

/** Absolute path exposed inside the WASI module. */
type GuestWorkspaceRootPath = string & {
  readonly [guestWorkspaceRootBrand]: "GuestWorkspaceRootPath";
};

const GUEST_WORKSPACE_ROOT_VALUE = "/workspace";
const GuestWorkspaceRootPathSchema = z
  .literal(GUEST_WORKSPACE_ROOT_VALUE)
  .transform(brandGuestWorkspaceRootPath);
const HostRootPathSchema = z.custom<HostRootPath>(
  (value) => typeof value === "string" && path.isAbsolute(value),
  "host root path must be absolute",
);

const MachineHostPathMappingSchema = z
  .object({
    kind: z.literal("alias"),
    host_root: HostRootPathSchema,
    style: MachineHostPathStyleSchema,
  })
  .strict();

const MachineRequestSchema = z
  .object({
    protocol_version: z.literal(MACHINE_PROTOCOL_VERSION),
    workspace_root: GuestWorkspaceRootPathSchema,
    host_path_mapping: MachineHostPathMappingSchema,
    argv: z.array(z.string()),
  })
  .strict();

type MachineRequest = z.infer<typeof MachineRequestSchema>;

/** Syntactically valid machine-mode request plus the host root to preopen. */
export type MachineRunPlan = {
  hostPathStyle: MachineHostPathStyle;
  hostRoot: HostRootPath;
  requestJson: string;
};

type NormalizedRootArgv = {
  argv: string[];
  hostRoot: HostRootPath;
};

const GUEST_WORKSPACE_ROOT = GuestWorkspaceRootPathSchema.parse(GUEST_WORKSPACE_ROOT_VALUE);
const KNOWN_VALUE_OPTIONS = new Set([
  "--color",
  "--config",
  "--format",
  "--traversal-error-policy",
]);
const KNOWN_FLAG_OPTIONS = new Set([
  "--all",
  "--allow-raises",
  "--detected",
  "--dry-run",
  "--no-require-ratchet-current",
  "--quiet",
  "--require-ratchet-current",
  "--write",
]);

/**
 * Converts wrapper argv into the machine transport request used by the WASI fallback.
 *
 * This deliberately parses only enough argv to choose the host directory mounted at
 * `/workspace`. Rust owns the command grammar and receives every non-root token unchanged.
 */
export function buildMachineRunRequest(
  userArgv: readonly string[],
): Result<MachineRunPlan, WrapperError> {
  const normalized = normalizeRootForMachineRequest(userArgv);
  if (Result.isError(normalized)) {
    return normalized;
  }

  const hostPathStyle = hostPathStyleForCurrentPlatform();
  const request = MachineRequestSchema.safeParse({
    protocol_version: MACHINE_PROTOCOL_VERSION,
    workspace_root: GUEST_WORKSPACE_ROOT,
    host_path_mapping: {
      kind: "alias",
      host_root: normalized.value.hostRoot,
      style: hostPathStyle,
    },
    argv: normalized.value.argv,
  } satisfies MachineRequest);
  if (!request.success) {
    return Result.err(
      new RuntimeFailureError(
        "build machine request",
        "generated machine request failed internal schema validation",
        request.error,
      ),
    );
  }

  return Result.ok({
    hostPathStyle,
    hostRoot: normalized.value.hostRoot,
    requestJson: JSON.stringify(request.data),
  });
}

function normalizeRootForMachineRequest(
  userArgv: readonly string[],
): Result<NormalizedRootArgv, MissingFlagValueError> {
  let hostRoot = unsafeHostRootPathFromAbsolutePath(resolveAbsolutePath(process.cwd()));
  const argv = [...userArgv];

  for (let index = 0; index < userArgv.length; index += 1) {
    const argument = userArgv[index];
    if (argument === undefined) {
      continue;
    }

    if (argument === "--") {
      break;
    }

    if (argument === "--root") {
      const rawRootValue = userArgv[index + 1];
      if (rawRootValue === undefined || rawRootValue.length === 0) {
        return Result.err(new MissingFlagValueError("--root"));
      }
      hostRoot = unsafeHostRootPathFromAbsolutePath(resolveAbsolutePath(rawRootValue));
      argv[index + 1] = GUEST_WORKSPACE_ROOT;
      index += 1;
      continue;
    }

    if (argument.startsWith("--root=")) {
      const rawRootValue = argument.slice("--root=".length);
      if (rawRootValue.length === 0) {
        return Result.err(new MissingFlagValueError("--root"));
      }
      hostRoot = unsafeHostRootPathFromAbsolutePath(resolveAbsolutePath(rawRootValue));
      argv[index] = `--root=${GUEST_WORKSPACE_ROOT}`;
      continue;
    }

    if (KNOWN_VALUE_OPTIONS.has(argument)) {
      index += 1;
      continue;
    }

    const optionName = argument.split("=", 1)[0] ?? argument;
    if (KNOWN_VALUE_OPTIONS.has(optionName) || KNOWN_FLAG_OPTIONS.has(optionName)) {
      continue;
    }

    if (argument.startsWith("-")) {
      break;
    }
  }

  return Result.ok({
    argv,
    hostRoot,
  });
}

function hostPathStyleForCurrentPlatform(): MachineHostPathStyle {
  return process.platform === "win32"
    ? MachineHostPathStyle.Windows
    : MachineHostPathStyle.Posix;
}

function brandGuestWorkspaceRootPath<const TValue extends string>(
  value: TValue,
): TValue & GuestWorkspaceRootPath {
  return value as TValue & GuestWorkspaceRootPath;
}
