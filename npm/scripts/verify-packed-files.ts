import { spawnSync, type SpawnSyncReturns } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { z } from "zod/v4";

const USAGE =
  "usage: verify-packed-files.ts <package-dir> (--require <path> | --forbid-suffix <suffix>)...";

const PackageRelativePathSchema = z
  .string()
  .refine(isPackageRelativePath, "must be a normalized package-relative path")
  .brand<"PackageRelativePath">();

const PackagePathSuffixSchema = z
  .string()
  .refine(isPackagePathSuffix, "must be a non-empty path suffix without separators")
  .brand<"PackagePathSuffix">();

const PackDryRunReportSchema = z.object({
  files: z.array(
    z.object({
      path: PackageRelativePathSchema,
    }),
  ),
});

const AbsolutePathBrandSchema = z.string().brand<"Path">().brand<"AbsolutePath">();
const PnpmLauncherPathBrandSchema = AbsolutePathBrandSchema.brand<"PnpmLauncherPath">();
const NodeExecutablePathBrandSchema = AbsolutePathBrandSchema.brand<"ExecutablePath">();
const PnpmExecutablePathBrandSchema = PnpmLauncherPathBrandSchema.brand<"ExecutablePath">();
const PackageDirectoryPathBrandSchema = AbsolutePathBrandSchema.brand<"PackageDirectoryPath">();
const PnpmUserAgentSchema = z
  .string()
  .regex(/^pnpm\/\d/u, "must identify pnpm")
  .brand<"PnpmUserAgent">();

const PNPM_PACK_ARGUMENTS = ["pack", "--json", "--dry-run"] as const;
const PNPM_ON_FAIL_ENVIRONMENT_VARIABLE = "pnpm_config_pm_on_fail";

export type PackageRelativePath = z.infer<typeof PackageRelativePathSchema>;
export type PackagePathSuffix = z.infer<typeof PackagePathSuffixSchema>;
export type PackDryRunReport = z.infer<typeof PackDryRunReportSchema>;

type RequiredPathRule = {
  readonly kind: "required_path";
  readonly path: PackageRelativePath;
};

type ForbiddenSuffixRule = {
  readonly kind: "forbidden_suffix";
  readonly suffix: PackagePathSuffix;
};

export type PackVerificationRule = RequiredPathRule | ForbiddenSuffixRule;

export type PackVerifierArgs = {
  readonly packageDirectory: string;
  readonly rules: NonEmptyArray<PackVerificationRule>;
};

type ParseArgsResult =
  | {
      readonly kind: "ok";
      readonly value: PackVerifierArgs;
    }
  | {
      readonly kind: "invalid_arguments";
      readonly message: string;
    };

type PackViolation =
  | {
      readonly kind: "missing_required_path";
      readonly path: PackageRelativePath;
    }
  | {
      readonly kind: "forbidden_path";
      readonly path: PackageRelativePath;
      readonly suffix: PackagePathSuffix;
    };

type NonEmptyArray<T> = readonly [T, ...T[]];

type VerificationResult =
  | {
      readonly kind: "ok";
    }
  | {
      readonly kind: "violations";
      readonly violations: NonEmptyArray<PackViolation>;
    };

type ParsePackReportResult =
  | {
      readonly kind: "ok";
      readonly report: PackDryRunReport;
    }
  | {
      readonly kind: "invalid_pack_report";
      readonly message: string;
    };

type PnpmPackDryRunCommandResult =
  | {
      readonly kind: "ok";
      readonly command:
        | {
            readonly kind: "javascript_launcher";
            readonly executable: z.infer<typeof NodeExecutablePathBrandSchema>;
            readonly launcher: z.infer<typeof PnpmLauncherPathBrandSchema>;
          }
        | {
            readonly kind: "native_launcher";
            readonly executable: z.infer<typeof PnpmExecutablePathBrandSchema>;
          };
    }
  | {
      readonly kind: "invalid_launcher";
      readonly message: string;
    };

/** Parses verifier CLI arguments into explicit package-content rules. */
export function parseVerifierArgs(args?: readonly string[]): ParseArgsResult {
  let parsed;
  try {
    parsed = parseArgs({
      args: args?.slice(),
      options: { require: { type: "string" }, "forbid-suffix": { type: "string" } },
      allowPositionals: true,
      tokens: true,
    });
  } catch (error) {
    return invalidArguments(String(error));
  }
  const [packageDirectory, unexpected] = parsed.positionals;
  if (packageDirectory === undefined) {
    return invalidArguments(USAGE);
  }
  if (unexpected !== undefined) {
    return invalidArguments(`unexpected argument: ${unexpected}`);
  }

  const rules: PackVerificationRule[] = [];
  for (const token of parsed.tokens) {
    if (token.kind !== "option") {
      continue;
    }
    if (token.name === "require") {
      const parsedPath = PackageRelativePathSchema.safeParse(token.value);
      if (!parsedPath.success) {
        return invalidArguments(`invalid package-relative path: ${token.value}`);
      }
      rules.push({ kind: "required_path", path: parsedPath.data });
    } else {
      const parsedSuffix = PackagePathSuffixSchema.safeParse(token.value);
      if (!parsedSuffix.success) {
        return invalidArguments(`invalid package path suffix: ${token.value}`);
      }
      rules.push({ kind: "forbidden_suffix", suffix: parsedSuffix.data });
    }
  }

  const nonEmptyRules = toNonEmptyArray(rules);
  if (nonEmptyRules === null) {
    return invalidArguments(USAGE);
  }

  return {
    kind: "ok",
    value: {
      packageDirectory,
      rules: nonEmptyRules,
    },
  };
}

/** Parses and validates `pnpm pack --json --dry-run` output. */
export function parsePackDryRunReport(stdout: string): ParsePackReportResult {
  let parsed: unknown;
  try {
    parsed = JSON.parse(stdout);
  } catch {
    return {
      kind: "invalid_pack_report",
      message: "pnpm pack output was not valid JSON",
    };
  }

  const parsedReport = PackDryRunReportSchema.safeParse(parsed);
  return parsedReport.success
    ? {
        kind: "ok",
        report: parsedReport.data,
      }
    : {
        kind: "invalid_pack_report",
        message: `pnpm pack output had an unexpected shape: ${formatZodError(parsedReport.error)}`,
      };
}

/** Checks required and forbidden file rules against a packed-file report. */
export function verifyPackedFiles(
  report: PackDryRunReport,
  rules: NonEmptyArray<PackVerificationRule>,
): VerificationResult {
  const packedPaths = new Set(report.files.map((file) => file.path));
  const violations: PackViolation[] = [];

  for (const rule of rules) {
    switch (rule.kind) {
      case "required_path":
        if (!packedPaths.has(rule.path)) {
          violations.push({
            kind: "missing_required_path",
            path: rule.path,
          });
        }
        break;

      case "forbidden_suffix":
        for (const packedPath of packedPaths) {
          if (packedPath.endsWith(rule.suffix)) {
            violations.push({
              kind: "forbidden_path",
              path: packedPath,
              suffix: rule.suffix,
            });
          }
        }
        break;
    }
  }

  const nonEmptyViolations = toNonEmptyArray(violations);
  return nonEmptyViolations === null
    ? { kind: "ok" }
    : { kind: "violations", violations: nonEmptyViolations };
}

/** Builds a pack invocation from launcher provenance supplied by a pnpm lifecycle process. */
export function pnpmPackDryRunCommand(args: {
  readonly nodeExecutablePath: string;
  readonly packageManagerUserAgent: string | undefined;
  readonly platform: NodeJS.Platform;
  readonly pnpmLauncherPath: string | undefined;
}): PnpmPackDryRunCommandResult {
  const userAgent = PnpmUserAgentSchema.safeParse(args.packageManagerUserAgent);
  if (!userAgent.success) {
    return invalidPnpmLauncher("npm_config_user_agent did not identify pnpm");
  }
  if (args.pnpmLauncherPath === undefined) {
    return invalidPnpmLauncher("npm_execpath did not provide the pnpm launcher path");
  }

  const pathImplementation = args.platform === "win32" ? path.win32 : path.posix;
  const PnpmLauncherPathSchema = PnpmLauncherPathBrandSchema.refine(
    pathImplementation.isAbsolute,
    "must be absolute",
  );
  const NodeExecutablePathSchema = NodeExecutablePathBrandSchema.refine(
    pathImplementation.isAbsolute,
    "must be absolute",
  );
  const launcher = PnpmLauncherPathSchema.safeParse(args.pnpmLauncherPath);
  if (!launcher.success) {
    return invalidPnpmLauncher(`pnpm launcher path must be absolute: ${args.pnpmLauncherPath}`);
  }

  const basename = pathImplementation.basename(launcher.data).toLowerCase();
  const extension = pathImplementation.extname(launcher.data).toLowerCase();
  if (basename === "pnpm.js" || basename === "pnpm.cjs" || basename === "pnpm.mjs") {
    const nodeExecutable = NodeExecutablePathSchema.safeParse(args.nodeExecutablePath);
    if (!nodeExecutable.success) {
      return invalidPnpmLauncher(
        `Node executable path must be absolute: ${args.nodeExecutablePath}`,
      );
    }
    return {
      kind: "ok",
      command: {
        kind: "javascript_launcher",
        executable: nodeExecutable.data,
        launcher: launcher.data,
      },
    };
  }

  const expectedNativeBasename = args.platform === "win32" ? "pnpm.exe" : "pnpm";
  if (basename !== expectedNativeBasename || (args.platform === "win32" && extension !== ".exe")) {
    return invalidPnpmLauncher(`unsupported pnpm launcher: ${launcher.data}`);
  }

  return {
    kind: "ok",
    command: {
      kind: "native_launcher",
      executable: PnpmExecutablePathBrandSchema.parse(launcher.data),
    },
  };
}

function main(): number {
  const parsedArgs = parseVerifierArgs();
  if (parsedArgs.kind !== "ok") {
    console.error(parsedArgs.message);
    return 1;
  }

  const packagePath = PackageDirectoryPathBrandSchema.parse(
    path.resolve(parsedArgs.value.packageDirectory),
  );
  const commandResult = pnpmPackDryRunCommand({
    nodeExecutablePath: process.execPath,
    packageManagerUserAgent: process.env.npm_config_user_agent,
    platform: process.platform,
    pnpmLauncherPath: process.env.npm_execpath,
  });
  if (commandResult.kind !== "ok") {
    console.error(commandResult.message);
    return 1;
  }
  const packed = spawnPnpmPackDryRun(commandResult.command, packagePath);

  if (packed.error !== undefined) {
    writeProcessOutput(process.stdout, packed.stdout);
    writeProcessOutput(process.stderr, packed.stderr);
    console.error(
      `failed to run pnpm pack for ${parsedArgs.value.packageDirectory}: ${packed.error.message}`,
    );
    return 1;
  }

  if (packed.status !== 0) {
    writeProcessOutput(process.stdout, packed.stdout);
    writeProcessOutput(process.stderr, packed.stderr);
    return packed.status ?? 1;
  }

  writeProcessOutput(process.stdout, packed.stdout);

  const parsedReport = parsePackDryRunReport(packed.stdout);
  if (parsedReport.kind !== "ok") {
    console.error(parsedReport.message);
    return 1;
  }

  const verification = verifyPackedFiles(parsedReport.report, parsedArgs.value.rules);
  if (verification.kind === "ok") {
    return 0;
  }

  console.error(renderViolations(parsedArgs.value.packageDirectory, verification.violations));
  return 1;
}

/** Runs the validated launcher without allowing the package manifest to select another pnpm. */
function spawnPnpmPackDryRun(
  command: Extract<PnpmPackDryRunCommandResult, { kind: "ok" }>["command"],
  packagePath: z.infer<typeof PackageDirectoryPathBrandSchema>,
): SpawnSyncReturns<string> {
  let executableArguments: readonly string[];
  switch (command.kind) {
    case "javascript_launcher":
      executableArguments = [command.launcher, ...PNPM_PACK_ARGUMENTS];
      break;

    case "native_launcher":
      executableArguments = PNPM_PACK_ARGUMENTS;
      break;

    default:
      return assertNever(command);
  }
  return spawnSync(command.executable, executableArguments, {
    cwd: packagePath,
    encoding: "utf8",
    env: pnpmPackEnvironment(process.env),
    stdio: ["ignore", "pipe", "pipe"],
  });
}

/** Prevents case-insensitive environment collisions from overriding deterministic pnpm selection. */
function pnpmPackEnvironment(environment: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  const result = Object.fromEntries(
    Object.entries(environment).filter(
      ([name]) => name.toLowerCase() !== PNPM_ON_FAIL_ENVIRONMENT_VARIABLE,
    ),
  );
  result[PNPM_ON_FAIL_ENVIRONMENT_VARIABLE] = "ignore";
  return result;
}

function assertNever(value: never): never {
  throw new Error(`unhandled pnpm command: ${JSON.stringify(value)}`);
}

function invalidPnpmLauncher(message: string): PnpmPackDryRunCommandResult {
  return {
    kind: "invalid_launcher",
    message,
  };
}

function isPackageRelativePath(value: string): boolean {
  if (value.length === 0 || path.isAbsolute(value) || value.includes("\\")) {
    return false;
  }

  const segments = value.split("/");
  return !segments.some((segment) => segment === "" || segment === "." || segment === "..");
}

function isPackagePathSuffix(value: string): boolean {
  return value.length > 0 && !value.includes("/") && !value.includes("\\");
}

function renderViolations(
  packageDirectory: string,
  violations: NonEmptyArray<PackViolation>,
): string {
  return [
    `packed output for ${packageDirectory} failed package-content checks:`,
    ...violations.map(renderViolation),
  ].join("\n");
}

function renderViolation(violation: PackViolation): string {
  switch (violation.kind) {
    case "missing_required_path":
      return `- missing required file: ${violation.path}`;

    case "forbidden_path":
      return `- forbidden file matched suffix ${violation.suffix}: ${violation.path}`;
  }
}

function invalidArguments(message: string): ParseArgsResult {
  return {
    kind: "invalid_arguments",
    message,
  };
}

function toNonEmptyArray<T>(values: readonly T[]): NonEmptyArray<T> | null {
  return values.length === 0 ? null : [values[0] as T, ...values.slice(1)];
}

function writeProcessOutput(
  stream: NodeJS.WritableStream,
  output: string | null | undefined,
): void {
  if (output !== undefined && output !== null && output.length > 0) {
    stream.write(output);
  }
}

function formatZodError(error: z.ZodError): string {
  return error.issues
    .map((issue) => `${issue.path.join(".") || "<root>"}: ${issue.message}`)
    .join("; ");
}

if (process.argv[1] !== undefined) {
  const invokedPath = path.resolve(process.argv[1]);
  const modulePath = fileURLToPath(import.meta.url);
  if (invokedPath === modulePath) {
    process.exit(main());
  }
}
