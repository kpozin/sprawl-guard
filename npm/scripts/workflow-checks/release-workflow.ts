/**
 * Reads the CI and release workflows through schemas, so tests can compare them with the package
 * metadata and check individual steps.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { parse } from "yaml";
import { z } from "zod";

/** The CI workflow, which smoke-tests each native runtime target. */
export const CI_WORKFLOW_URL = new URL("../../../.github/workflows/ci.yml", import.meta.url);
/** The release workflow, which builds, verifies and publishes each release. */
export const RELEASE_WORKFLOW_URL = new URL(
  "../../../.github/workflows/release.yml",
  import.meta.url,
);
/** The fields of a workflow step that the tests read. */
const WORKFLOW_STEP_SCHEMA = z.object({
  id: z.string().min(1).optional(),
  if: z.string().min(1).optional(),
  name: z.string().min(1),
  run: z.string().min(1).optional(),
  uses: z.string().min(1).optional(),
  env: z.record(z.string(), z.unknown()).optional(),
  with: z.record(z.string(), z.unknown()).optional(),
});

/** The parts of the CI workflow that the tests read. */
export const CI_WORKFLOW_SCHEMA = z.object({
  jobs: z.object({
    "runtime-smoke": z.object({
      strategy: z.object({
        matrix: z.object({
          include: z.string().includes("fromJSON(needs.runtime-targets.outputs.matrix)"),
        }),
      }),
    }),
  }),
});

/** The parts of the release workflow that the tests read, with the values they rely on. */
export const RELEASE_WORKFLOW_SCHEMA = z.object({
  concurrency: z.object({
    group: z.literal("sprawl-guard-release"),
    "cancel-in-progress": z.literal(false),
  }),
  permissions: z.record(z.string(), z.unknown()),
  on: z.object({
    workflow_dispatch: z.object({
      inputs: z.object({
        npm_channel: z.object({
          default: z.literal("auto"),
          options: z.tuple([
            z.literal("auto"),
            z.literal("latest"),
            z.literal("next"),
            z.literal("beta"),
            z.literal("rc"),
            z.literal("canary"),
            z.literal("custom"),
          ]),
          type: z.literal("choice"),
        }),
        custom_npm_tag: z.object({
          default: z.literal(""),
          type: z.literal("string"),
        }),
        verify_crates: z.object({
          default: z.literal(false),
          type: z.literal("boolean"),
        }),
        verify_npm: z.object({
          default: z.literal(false),
          type: z.literal("boolean"),
        }),
        verify_pypi: z.object({
          default: z.literal(false),
          type: z.literal("boolean"),
        }),
        release_mode: z.object({
          default: z.literal("choose"),
          options: z.tuple([z.literal("choose"), z.literal("draft"), z.literal("publish")]),
          required: z.literal(true),
          type: z.literal("choice"),
        }),
      }).strict(),
    }),
  }).strict(),
  jobs: z.object({
    "release-metadata": z.object({
      outputs: z.object({
        github_latest: z.string().min(1),
        github_prerelease: z.string().min(1),
        python_version: z.string().min(1),
      }),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "artifact-build": z.object({
      permissions: z.record(z.string(), z.string()),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
      strategy: z.object({
        matrix: z.object({
          include: z.array(
            z.object({
              container: z.string().min(1).optional(),
              package_target: z.string().min(1),
              rust_target: z.string().min(1),
            }),
          ),
        }),
      }),
    }),
    "artifact-attestation": z.object({
      if: z.string().min(1),
      needs: z.array(z.string().min(1)),
      permissions: z.record(z.string(), z.string()),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "package-assembly": z.object({
      permissions: z.record(z.string(), z.string()),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "cargo-candidate": z.object({
      permissions: z.record(z.string(), z.string()),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "verify-npm-candidate-native": z.object({
      strategy: z.object({
        matrix: z.object({
          include: z.array(
            z.object({
              container: z.string().min(1).optional(),
              package_name: z.string().min(1),
            }),
          ),
        }),
      }),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "verify-npm-candidate-musl": z.object({
      env: z.object({
        NPM_RUNTIME_PACKAGE_NAME: z.string().min(1),
      }),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "verify-npm-candidate-wasi": z.object({
      env: z.object({
        NPM_RUNTIME_PACKAGE_NAME: z.string().min(1),
      }),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "verify-wheel-candidate-native": z.object({
      strategy: z.object({
        matrix: z.object({
          include: z.array(
            z.object({
              container: z.string().min(1).optional(),
              package_target: z.string().min(1),
              wheel_tag: z.string().min(1),
            }),
          ),
        }),
      }),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "verify-wheel-candidate-musl": z.object({
      env: z.object({
        WHEEL_TAG: z.string().min(1),
      }),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "draft-release": z.object({
      if: z.string().min(1),
      needs: z.array(z.string().min(1)),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "publish-cargo": z.object({
      environment: z.literal("release"),
      if: z.string().min(1),
      needs: z.array(z.string().min(1)),
      permissions: z.record(z.string(), z.string()),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "publish-npm": z.object({
      environment: z.literal("release"),
      if: z.string().min(1),
      needs: z.array(z.string().min(1)),
      permissions: z.record(z.string(), z.string()),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "publish-pypi": z.object({
      environment: z.literal("release"),
      if: z.string().min(1),
      needs: z.array(z.string().min(1)),
      permissions: z.record(z.string(), z.string()),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "verify-published-cargo": z.object({
      env: z.record(z.string(), z.unknown()),
      if: z.string().includes("inputs.verify_crates"),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "verify-published-npm-native": z.object({
      if: z.string().includes("inputs.verify_npm"),
      strategy: z.object({
        matrix: z.object({
          include: z.array(
            z.object({
              container: z.string().min(1).optional(),
              package_name: z.string().min(1),
            }),
          ),
        }),
      }),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "verify-published-npm-musl": z.object({
      env: z.object({
        NPM_RUNTIME_PACKAGE_NAME: z.string().min(1),
      }),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "verify-published-npm-wasi": z.object({
      env: z.object({
        NPM_RUNTIME_PACKAGE_NAME: z.string().min(1),
      }),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "verify-published-pypi-native": z.object({
      if: z.string().includes("inputs.verify_pypi"),
      strategy: z.object({
        matrix: z.object({
          include: z.array(
            z.object({
              container: z.string().min(1).optional(),
              package_target: z.string().min(1),
            }),
          ),
        }),
      }),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "verify-published-pypi-musl": z.object({
      if: z.string().includes("inputs.verify_pypi"),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
  }),
});

/** Reads a workflow file and parses it with the given schema, failing the test on a mismatch. */
export function parseWorkflow<TSchema extends z.ZodType>(
  workflowUrl: URL,
  schema: TSchema,
): z.infer<TSchema> {
  const parsedWorkflow = parse(readFileSync(workflowUrl, "utf8"));
  const result = schema.safeParse(parsedWorkflow);
  if (!result.success) {
    assert.fail(z.prettifyError(result.error));
  }
  return result.data;
}

/** Returns the command from the workflow's single step with the requested name. */
export function commandForStep(
  steps: readonly { readonly name: string; readonly run?: string | undefined }[],
  stepName: string,
): string {
  const matchingSteps = steps.filter(({ name }) => name === stepName);
  assert.equal(matchingSteps.length, 1, `expected exactly one ${stepName} step`);
  const command = matchingSteps[0]?.run;
  assert.ok(command !== undefined, `${stepName} step must run a command`);
  return command;
}

/** Returns the workflow's single step with the requested name. */
export function stepWithName<TStep extends { readonly name: string }>(
  steps: readonly TStep[],
  stepName: string,
): TStep {
  const matchingSteps = steps.filter(({ name }) => name === stepName);
  assert.equal(matchingSteps.length, 1, `expected exactly one ${stepName} step`);
  const step = matchingSteps[0];
  assert.ok(step !== undefined);
  return step;
}
