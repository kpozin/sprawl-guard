/**
 * Checks the shell scripts in every workflow's `run:` steps for mistakes that would otherwise
 * surface only when GitHub runs the workflow: inline programs that linters can't read,
 * environment variables that the step or the scripts it runs read but nothing defines,
 * missing scripts, and install-check arguments the check rejects.
 */

import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { describe, it } from "node:test";
import { parse } from "yaml";
import { z } from "zod";
import { parseNpmInstallCheckArguments } from "./check-npm-install.ts";

const REPOSITORY_ROOT_URL = new URL("../../../", import.meta.url);
const WORKFLOW_DIRECTORY_URL = new URL(".github/workflows/", REPOSITORY_ROOT_URL);
const ENV_SCHEMA = z.record(z.string(), z.unknown()).optional();
const ANY_WORKFLOW_SCHEMA = z.object({
  env: ENV_SCHEMA,
  jobs: z.record(
    z.string(),
    z.object({
      env: ENV_SCHEMA,
      steps: z
        .array(
          z.object({
            env: ENV_SCHEMA,
            name: z.string().optional(),
            run: z.string().optional(),
            uses: z.string().optional(),
            with: z.object({ path: z.string().optional() }).optional(),
            "working-directory": z.string().optional(),
          }),
        )
        .optional(),
      "timeout-minutes": z.number().positive().optional(),
    }),
  ),
});
type AnyWorkflow = z.infer<typeof ANY_WORKFLOW_SCHEMA>;
type AnyWorkflowJob = AnyWorkflow["jobs"][string];
type AnyWorkflowStep = NonNullable<AnyWorkflowJob["steps"]>[number];

/** Variables that GitHub's hosted runners provide to every step. */
const RUNNER_VARIABLES: ReadonlySet<string> = new Set([
  "CI",
  "GITHUB_ACTION",
  "GITHUB_ACTIONS",
  "GITHUB_ACTOR",
  "GITHUB_API_URL",
  "GITHUB_BASE_REF",
  "GITHUB_ENV",
  "GITHUB_EVENT_NAME",
  "GITHUB_EVENT_PATH",
  "GITHUB_HEAD_REF",
  "GITHUB_JOB",
  "GITHUB_OUTPUT",
  "GITHUB_PATH",
  "GITHUB_REF",
  "GITHUB_REF_NAME",
  "GITHUB_REF_TYPE",
  "GITHUB_REPOSITORY",
  "GITHUB_REPOSITORY_OWNER",
  "GITHUB_RUN_ATTEMPT",
  "GITHUB_RUN_ID",
  "GITHUB_RUN_NUMBER",
  "GITHUB_SERVER_URL",
  "GITHUB_SHA",
  "GITHUB_STEP_SUMMARY",
  "GITHUB_WORKFLOW",
  "GITHUB_WORKSPACE",
  "HOME",
  "PWD",
  "RUNNER_ARCH",
  "RUNNER_OS",
  "RUNNER_TEMP",
  "RUNNER_TOOL_CACHE",
]);
/** Matches an upper-case shell variable reference, such as `$NAME` or `${NAME}`. */
const SHELL_VARIABLE_REFERENCE_PATTERN = /(?<!\\)\$\{?([A-Z][A-Z0-9_]*)/gu;
/** Matches an upper-case shell variable assignment, such as `NAME=value`. */
const SHELL_VARIABLE_ASSIGNMENT_PATTERN = /\b([A-Z][A-Z0-9_]*)=/gu;
/** Matches a variable that a step exports to later steps through `$GITHUB_ENV`. */
const GITHUB_ENV_EXPORT_PATTERN = /\b([A-Z][A-Z0-9_]*)=[^\n]*GITHUB_ENV/gu;
/** Matches a Node.js script run by path, with its arguments up to the end of the command. */
const SCRIPT_INVOCATION_PATTERN = /\bnode (\S+\.ts)((?: [^\n|>;&]*)?)/gu;
/** Matches a package script run through pnpm, with the package directory if the command names one. */
const PNPM_RUN_PATTERN = /\bpnpm (?:--dir (\S+) )?run ([\w:.-]+)/gu;
/** Matches a script's read of one named environment variable. */
const PROCESS_ENV_READ_PATTERN = /\bprocess\.env\.([A-Z][A-Z0-9_]*)/gu;
/** Matches the module specifier of a static or dynamic import. */
const IMPORT_SPECIFIER_PATTERN = /\b(?:from|import)\s*\(?\s*"([^"]+)"/gu;
/** Matches a step that installs the workspace's npm dependencies. */
const DEPENDENCY_INSTALL_PATTERN = /\bpnpm install\b/u;
const PACKAGE_SCRIPTS_SCHEMA = z.object({ scripts: z.record(z.string(), z.string()).optional() });

/** Parses every workflow file, keyed by file name. */
function allWorkflows(): readonly (readonly [string, AnyWorkflow])[] {
  return readdirSync(WORKFLOW_DIRECTORY_URL)
    .filter((name) => name.endsWith(".yml"))
    .map((name) => [
      name,
      ANY_WORKFLOW_SCHEMA.parse(parse(readFileSync(new URL(name, WORKFLOW_DIRECTORY_URL), "utf8"))),
    ]);
}

/** Joins a script's backslash-continued lines so each command is on one line. */
function joinContinuedLines(script: string): string {
  return script.replaceAll(/\\\n\s*/gu, " ");
}

/** A script that a run step starts, directly or through a pnpm package script. */
type ScriptInvocation =
  | {
      readonly kind: "node_script";
      /** The script's path relative to the repository root. */
      readonly repositoryPath: string;
      /** The command's arguments, still in shell syntax. */
      readonly argumentText: string;
    }
  | {
      readonly kind: "missing_package_script";
      /** The package directory, relative to the repository root, and the script's name. */
      readonly packageScript: string;
    };

/**
 * Finds the Node.js scripts that a command starts in a repository directory, following
 * `pnpm run` through each package's scripts. A path that starts with one of the job's checkout
 * directories, or with that directory mounted at the container root, names a file in that
 * checkout of this repository; other paths are relative to the directory.
 */
function commandInvocations(
  command: string,
  directory: string,
  checkoutDirectories: readonly string[],
): readonly ScriptInvocation[] {
  const nodeScripts = [...command.matchAll(SCRIPT_INVOCATION_PATTERN)].map(
    ([, scriptPath = "", argumentText = ""]): ScriptInvocation => {
      const [firstSegment = "", ...rest] = scriptPath.replace(/^\//u, "").split("/");
      return {
        kind: "node_script",
        argumentText,
        repositoryPath: checkoutDirectories.includes(firstSegment)
          ? rest.join("/")
          : path.posix.join(directory, scriptPath),
      };
    },
  );
  const packageScripts = [...command.matchAll(PNPM_RUN_PATTERN)].flatMap(
    ([, packageDirectory = ".", scriptName = ""]): readonly ScriptInvocation[] => {
      const packagePath = path.posix.join(directory, packageDirectory);
      const manifestUrl = new URL(`${packagePath}/package.json`, REPOSITORY_ROOT_URL);
      const script = existsSync(manifestUrl)
        ? PACKAGE_SCRIPTS_SCHEMA.parse(JSON.parse(readFileSync(manifestUrl, "utf8"))).scripts?.[scriptName]
        : undefined;
      return script === undefined
        ? [{ kind: "missing_package_script", packageScript: `${packagePath}: ${scriptName}` }]
        : commandInvocations(script, packagePath, []);
    },
  );
  return [...nodeScripts, ...packageScripts];
}

/** Finds the Node.js scripts that a step starts. */
function scriptInvocations(job: AnyWorkflowJob, step: AnyWorkflowStep): readonly ScriptInvocation[] {
  const checkoutDirectories = (job.steps ?? []).flatMap(({ uses, with: options }) =>
    uses?.startsWith("actions/checkout@") === true && options?.path !== undefined
      ? [options.path]
      : [],
  );
  return commandInvocations(
    joinContinuedLines(step.run ?? ""),
    step["working-directory"] ?? ".",
    checkoutDirectories,
  );
}

/** A script, or a repository file it imports, with the module specifiers it imports. */
interface ImportedFile {
  readonly source: string;
  readonly specifiers: readonly string[];
}

/** Returns a script and every repository file it imports, directly or indirectly. */
function importGraph(repositoryPath: string, visited = new Set<string>()): readonly ImportedFile[] {
  const url = new URL(repositoryPath, REPOSITORY_ROOT_URL);
  if (visited.has(repositoryPath) || !existsSync(url)) {
    return [];
  }
  visited.add(repositoryPath);
  const source = readFileSync(url, "utf8");
  const specifiers = [...source.matchAll(IMPORT_SPECIFIER_PATTERN)].map(([, specifier = ""]) => specifier);
  return [
    { source, specifiers },
    ...specifiers
      .filter((specifier) => specifier.startsWith("."))
      .flatMap((specifier) =>
        importGraph(path.posix.join(path.posix.dirname(repositoryPath), specifier), visited),
      ),
  ];
}

/** Returns the environment variables that a script and the repository files it imports read. */
function scriptEnvironmentReads(repositoryPath: string): readonly string[] {
  return importGraph(repositoryPath).flatMap(({ source }) =>
    [...source.matchAll(PROCESS_ENV_READ_PATTERN)].map(([, name = ""]) => name),
  );
}

/** Returns the npm packages that a script and the repository files it imports load. */
function scriptPackageImports(repositoryPath: string): readonly string[] {
  return importGraph(repositoryPath).flatMap(({ specifiers }) =>
    specifiers.filter((specifier) => !specifier.startsWith(".") && !specifier.startsWith("node:")),
  );
}
/**
 * Matches a program passed to an interpreter as a string, such as `sh -euxc '...'` or
 * `node --eval "..."`. Neither shellcheck nor the tests can read such a program, and quotes
 * inside it belong to the surrounding script, so they can silently change its meaning.
 */
const INLINE_PROGRAM_PATTERN =
  /\b(?:(?:ba|z)?sh\s+-[a-z]*c|node\s+(?:--eval|-e|--print|-p)|python3?\s+-c)\s/u;

/**
 * Values for variables that workflows set from `${{ }}` expressions, which only GitHub can
 * evaluate.
 */
const EXPRESSION_VARIABLE_SAMPLES: Readonly<Record<string, string>> = {
  NATIVE_PACKAGE_NAME: "@kpozin/sprawl-guard-linux-x64-gnu",
  NPM_DIST_TAG: "latest",
  SPRAWL_GUARD_VERSION: "1.2.3",
};

/**
 * Splits shell arguments and expands `${NAME}` references from the step and job environment.
 * Workflow commands quote each argument whole, so a token is either a double-quoted string or
 * a bare word.
 */
function expandArguments(
  argumentText: string,
  job: AnyWorkflowJob,
  step: AnyWorkflowStep,
): readonly string[] {
  const environment = { ...job.env, ...step.env };
  return [...argumentText.matchAll(/"([^"]*)"|(\S+)/gu)].map(([, quoted, bare]) =>
    (quoted ?? bare ?? "").replaceAll(/\$\{([A-Z][A-Z0-9_]*)\}/gu, (_reference, name: string) => {
      const value = environment[name];
      assert.ok(typeof value === "string", `${name} must be set in the step or job environment`);
      return value.includes("${{") ? (EXPRESSION_VARIABLE_SAMPLES[name] ?? assert.fail(name)) : value;
    }),
  );
}

describe("workflow run scripts", () => {
  for (const [fileName, workflow] of allWorkflows()) {
    describe(`in ${fileName}`, () => {
      const steps = Object.entries(workflow.jobs).flatMap(([jobName, job]) =>
        (job.steps ?? []).map((step, index) => ({ index, job, jobName, step })),
      );

      it("keeps every program in a file that linters and tests can read", () => {
        const inlinePrograms = steps
          .filter(({ step }) => step.run !== undefined && INLINE_PROGRAM_PATTERN.test(step.run))
          .map(({ jobName, step }) => `${jobName}: ${step.name}`);

        assert.deepEqual(inlinePrograms, []);
      });

      it("defines every environment variable that a script reads", () => {
        const undefinedVariables = steps.flatMap(({ index, job, jobName, step }) => {
          const run = step.run ?? "";
          const defined = new Set([
            ...Object.keys(workflow.env ?? {}),
            ...Object.keys(job.env ?? {}),
            ...Object.keys(step.env ?? {}),
            ...(job.steps ?? [])
              .slice(0, index)
              .flatMap(({ run: earlier }) =>
                [...(earlier ?? "").matchAll(GITHUB_ENV_EXPORT_PATTERN)].map(([, name]) => name),
              ),
            ...[...run.matchAll(SHELL_VARIABLE_ASSIGNMENT_PATTERN)].map(([, name]) => name),
          ]);
          const read = [
            ...[...run.matchAll(SHELL_VARIABLE_REFERENCE_PATTERN)].map(([, name = ""]) => name),
            ...scriptInvocations(job, step).flatMap((invocation) =>
              invocation.kind === "node_script" ? scriptEnvironmentReads(invocation.repositoryPath) : [],
            ),
          ];
          return [...new Set(read)]
            .filter((name) => !defined.has(name) && !RUNNER_VARIABLES.has(name))
            .map((name) => `${jobName}: ${step.name}: ${name}`);
        });

        assert.deepEqual(undefinedVariables, []);
      });

      it("runs scripts that import only Node.js built-ins until dependencies are installed", () => {
        const packageImports = steps.flatMap(({ index, job, jobName, step }) =>
          (job.steps ?? []).slice(0, index).some(({ run }) => DEPENDENCY_INSTALL_PATTERN.test(run ?? ""))
            ? []
            : scriptInvocations(job, step).flatMap((invocation) =>
                invocation.kind === "node_script"
                  ? scriptPackageImports(invocation.repositoryPath).map(
                      (specifier) => `${jobName}: ${invocation.repositoryPath}: ${specifier}`,
                    )
                  : [],
              ),
        );

        assert.deepEqual(packageImports, []);
      });

      it("runs only scripts that exist", () => {
        const missingScripts = steps.flatMap(({ job, jobName, step }) =>
          scriptInvocations(job, step).flatMap((invocation) => {
            switch (invocation.kind) {
              case "node_script":
                return existsSync(new URL(invocation.repositoryPath, REPOSITORY_ROOT_URL))
                  ? []
                  : [`${jobName}: ${invocation.repositoryPath}`];
              case "missing_package_script":
                return [`${jobName}: ${invocation.packageScript}`];
            }
          }),
        );

        assert.deepEqual(missingScripts, []);
      });
    });
  }
});

/** The install check's path relative to the repository root. */
const INSTALL_CHECK_PATH = "npm/scripts/workflow-checks/check-npm-install.ts";

describe("npm install check invocations", () => {
  const invocations = allWorkflows().flatMap(([fileName, workflow]) =>
    Object.entries(workflow.jobs).flatMap(([jobName, job]) =>
      (job.steps ?? []).flatMap((step) =>
        scriptInvocations(job, step).flatMap((invocation) =>
          invocation.kind === "node_script" && invocation.repositoryPath === INSTALL_CHECK_PATH
            ? [
                {
                  args: expandArguments(invocation.argumentText, job, step),
                  job,
                  label: `${fileName}: ${jobName}`,
                },
              ]
            : [],
        ),
      ),
    ),
  );

  it("exist", () => {
    assert.notEqual(invocations.length, 0);
  });

  it("pass arguments that the check accepts", () => {
    for (const { args, label } of invocations) {
      assert.equal(parseNpmInstallCheckArguments(args).kind, "ok", `${label}: ${args.join(" ")}`);
    }
  });

  // npm can stall on a registry request, and GitHub otherwise lets a job run for six hours.
  it("run in jobs with a time limit", () => {
    const unbounded = invocations.filter(({ job }) => job["timeout-minutes"] === undefined).map(({ label }) => label);

    assert.deepEqual(unbounded, []);
  });
});
