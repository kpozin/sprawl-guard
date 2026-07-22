import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";
import { parse } from "yaml";
import { z } from "zod";
import { RUNTIME_PACKAGE_DEFINITIONS } from "./package-manifests.ts";

const CI_WORKFLOW_URL = new URL("../../.github/workflows/ci.yml", import.meta.url);
const RELEASE_WORKFLOW_URL = new URL(
  "../../.github/workflows/release.yml",
  import.meta.url,
);
const WORKFLOW_STEP_SCHEMA = z.object({
  id: z.string().min(1).optional(),
  name: z.string().min(1),
  run: z.string().min(1).optional(),
  uses: z.string().min(1).optional(),
  env: z.record(z.string(), z.unknown()).optional(),
  with: z.record(z.string(), z.unknown()).optional(),
});

const CI_WORKFLOW_SCHEMA = z.object({
  jobs: z.object({
    "runtime-smoke": z.object({
      strategy: z.object({
        matrix: z.object({
          include: z.array(
            z.object({
              rust_target: z.string().min(1),
            }),
          ),
        }),
      }),
    }),
  }),
});

const WORKFLOW_AUTHORITY_SCHEMA = z.object({
  permissions: z.record(z.string(), z.unknown()),
  jobs: z.record(z.string(), z.unknown()),
});
const REGISTRY_PUBLISH_COMMAND_PATTERN =
  /(?:^|[\n;&|])\s*(?:npm|pnpm|cargo)\b[^\n;&|]*\bpublish(?:\s|$)/u;

const RELEASE_WORKFLOW_SCHEMA = z.object({
  permissions: z.record(z.string(), z.unknown()),
  on: z.object({
    workflow_dispatch: z.object({
      inputs: z.object({
        release_commit: z.object({
          required: z.literal(true),
          type: z.literal("string"),
        }),
        release_tag: z.object({
          required: z.literal(true),
          type: z.literal("string"),
        }),
        npm_channel: z.object({
          default: z.literal("latest"),
          options: z.tuple([
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
      }),
    }),
  }).strict(),
  jobs: z.object({
    "release-metadata": z.object({
      outputs: z.object({
        github_latest: z.string().min(1),
        github_prerelease: z.string().min(1),
      }),
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "artifact-build": z.object({
      steps: z.array(WORKFLOW_STEP_SCHEMA),
      strategy: z.object({
        matrix: z.object({
          include: z.array(
            z.object({
              package_target: z.string().min(1),
              rust_target: z.string().min(1),
            }),
          ),
        }),
      }),
    }),
    "package-assembly": z.object({
      steps: z.array(WORKFLOW_STEP_SCHEMA),
    }),
    "verify-npm-candidate-native": z.object({
      strategy: z.object({
        matrix: z.object({
          include: z.array(
            z.object({
              package_name: z.string().min(1),
              package_slug: z.string().min(1),
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
    "draft-release": z.object({
      needs: z.array(z.string().min(1)),
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
  }),
});

/** Parses a workflow through its required target-matrix schema. */
function parseWorkflow<TSchema extends z.ZodType>(
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

/** Sorts target triples without mutating workflow or package metadata. */
function sortedTargets(targets: readonly string[]): readonly string[] {
  return targets.toSorted();
}

interface PackageTargetPair {
  readonly packageTarget: string;
  readonly rustTarget: string;
}

/** Sorts package-to-Rust-target pairs without mutating their source metadata. */
function sortedPackageTargetPairs(
  pairs: readonly PackageTargetPair[],
): readonly PackageTargetPair[] {
  return pairs.toSorted((left, right) =>
    left.packageTarget.localeCompare(right.packageTarget),
  );
}

/** Returns the command from the workflow's single step with the requested name. */
function commandForStep(
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
function stepWithName<TStep extends { readonly name: string }>(
  steps: readonly TStep[],
  stepName: string,
): TStep {
  const matchingSteps = steps.filter(({ name }) => name === stepName);
  assert.equal(matchingSteps.length, 1, `expected exactly one ${stepName} step`);
  const step = matchingSteps[0];
  assert.ok(step !== undefined);
  return step;
}

/** Collects scalar workflow values after YAML parsing. */
function workflowScalarValues(value: unknown): readonly string[] {
  if (typeof value === "string") {
    return [value];
  }
  if (Array.isArray(value)) {
    return value.flatMap(workflowScalarValues);
  }
  if (typeof value === "object" && value !== null) {
    return Object.entries(value).flatMap(([key, entryValue]) => [
      key,
      ...workflowScalarValues(entryValue),
    ]);
  }
  return [];
}

describe("native runtime workflow coverage", () => {
  describe("when comparing CI with the release artifact matrix", () => {
    it("smoke-tests every native release target", () => {
      const ciWorkflow = parseWorkflow(CI_WORKFLOW_URL, CI_WORKFLOW_SCHEMA);
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );

      const ciTargets = ciWorkflow.jobs["runtime-smoke"].strategy.matrix.include.map(
        ({ rust_target: rustTarget }) => rustTarget,
      );
      const releaseNativeTargets = releaseWorkflow.jobs[
        "artifact-build"
      ].strategy.matrix.include
        .filter(({ package_target: packageTarget }) => packageTarget !== "wasm32-wasip1")
        .map(({ rust_target: rustTarget }) => rustTarget);

      assert.deepEqual(sortedTargets(ciTargets), sortedTargets(releaseNativeTargets));
    });
  });

  describe("when comparing the release workflow with package metadata", () => {
    it("builds every runtime package target", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );

      const releaseTargets = releaseWorkflow.jobs[
        "artifact-build"
      ].strategy.matrix.include.map(
        ({ package_target: packageTarget, rust_target: rustTarget }) => ({
          packageTarget,
          rustTarget,
        }),
      );
      const packageTargets = RUNTIME_PACKAGE_DEFINITIONS.map(
        ({ slug: packageTarget, rustTarget }) => ({ packageTarget, rustTarget }),
      );

      assert.deepEqual(
        sortedPackageTargetPairs(releaseTargets),
        sortedPackageTargetPairs(packageTargets),
      );
    });
  });

  describe("when forwarding arguments through pnpm scripts", () => {
    it("passes release arguments without injecting an argument terminator", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );
      const packageAssemblySteps = releaseWorkflow.jobs["package-assembly"].steps;
      const draftReleaseSteps = releaseWorkflow.jobs["draft-release"].steps;

      const commands = [
        commandForStep(packageAssemblySteps, "Stage package payloads"),
        commandForStep(draftReleaseSteps, "Validate release assets and generate checksums"),
        commandForStep(draftReleaseSteps, "Replace and verify draft release assets"),
      ];
      assert.match(commands[0] ?? "", /stage:publish-payloads \.\.\/release-artifacts/u);
      assert.match(commands[1] ?? "", /release:prepare-assets .*\$\{RELEASE_VERSION\}/u);
      assert.match(commands[2] ?? "", /release:replace-assets .*\$\{RELEASE_TAG\}/u);
      for (const command of commands) {
        assert.doesNotMatch(command, /pnpm --dir npm run [^\n]* -- /u);
      }
    });
  });

  describe("when registry verification is not explicitly requested", () => {
    it("keeps both registry-install verification modes disabled", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );

      assert.equal(
        releaseWorkflow.on.workflow_dispatch.inputs.verify_crates.default,
        false,
      );
      assert.equal(
        releaseWorkflow.on.workflow_dispatch.inputs.verify_npm.default,
        false,
      );
    });
  });

  describe("when starting release assembly", () => {
    it("requires a manual immutable source and explicit npm channel selection", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );

      assert.deepEqual(Object.keys(releaseWorkflow.on), ["workflow_dispatch"]);
      assert.deepEqual(
        releaseWorkflow.on.workflow_dispatch.inputs.npm_channel.options,
        ["latest", "next", "beta", "rc", "canary", "custom"],
      );
    });
  });

  describe("when preparing the local npm bootstrap publication", () => {
    it("uploads the complete packed candidate without compiling another binary", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );
      const steps = releaseWorkflow.jobs["package-assembly"].steps;
      const verificationIndex = steps.findIndex(
        ({ name }) => name === "Verify staged package packs",
      );
      const prepareIndex = steps.findIndex(
        ({ name }) => name === "Prepare assembled npm publication candidate",
      );
      const uploadIndex = steps.findIndex(
        ({ name }) => name === "Upload assembled npm publication candidate",
      );
      const upload = stepWithName(
        steps,
        "Upload assembled npm publication candidate",
      );
      const digest = stepWithName(
        steps,
        "Record npm publication candidate digest",
      );

      const prepareCommand = commandForStep(
        steps,
        "Prepare assembled npm publication candidate",
      );
      assert.match(prepareCommand, /release:prepare-npm \.\.\/npm-publication-candidate/u);
      assert.match(
        prepareCommand,
        /\$\{RELEASE_COMMIT\}.*\$\{RELEASE_TAG\}.*\$\{NPM_DIST_TAG\}/u,
      );
      assert.ok(verificationIndex < prepareIndex);
      assert.ok(prepareIndex < uploadIndex);
      assert.match(upload.uses ?? "", /^actions\/upload-artifact@/u);
      assert.equal(upload.with?.name, "npm-publication-candidate");
      assert.equal(upload.with?.path, "npm-publication-candidate");
      assert.equal(upload.id, "npm-publication-candidate");
      assert.equal(
        digest.env?.ARTIFACT_DIGEST,
        "${{ steps.npm-publication-candidate.outputs.artifact-digest }}",
      );
    });

    it("restores exactly one regular payload for every runtime package", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );
      const artifactSteps = releaseWorkflow.jobs["artifact-build"].steps;
      const assemblySteps = releaseWorkflow.jobs["package-assembly"].steps;
      const archiveCommand = commandForStep(
        artifactSteps,
        "Archive release artifact payload",
      );
      assert.match(archiveCommand, /tar -C "release-artifacts\/\$\{PACKAGE_TARGET\}"/u);
      assert.match(archiveCommand, /"\$\{ARTIFACT_FILE_NAME\}"/u);

      const restoreCommand = commandForStep(
        assemblySteps,
        "Restore release artifact payloads",
      );
      for (const definition of RUNTIME_PACKAGE_DEFINITIONS) {
        assert.ok(
          restoreCommand.includes(
            `[${definition.slug}]=${definition.artifactRelativePath}`,
          ),
        );
      }
      assert.doesNotMatch(restoreCommand, /tar -xf/u);
      assert.match(restoreCommand, /tar -xOf/u);
      assert.match(restoreCommand, /tar -tvf/u);
      assert.match(restoreCommand, /-links 1/u);
    });
  });

  describe("when inspecting workflow registry authority", () => {
    it("contains no registry publication jobs, credentials, or OIDC permission", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        WORKFLOW_AUTHORITY_SCHEMA,
      );
      const scalars = workflowScalarValues(releaseWorkflow);

      assert.equal(scalars.includes("id-token"), false);
      assert.equal(scalars.some((value) => value.includes("secrets.")), false);
      assert.equal(
        scalars.some((value) =>
          /(?:NPM|NODE_AUTH|CARGO(?:_REGISTRY|_REGISTRIES_CRATES_IO)?)_TOKEN/u.test(value),
        ),
        false,
      );
      assert.equal(
        scalars.some((value) => /release:publish-(?:cargo|npm)/u.test(value)),
        false,
      );
      assert.equal(
        scalars.some((value) => REGISTRY_PUBLISH_COMMAND_PATTERN.test(value)),
        false,
      );
    });

    it("recognizes registry publish commands with global options", () => {
      for (const command of [
        "npm --prefix package publish",
        "pnpm --dir npm publish",
        "cargo +stable publish",
      ]) {
        assert.match(command, REGISTRY_PUBLISH_COMMAND_PATTERN);
      }
    });
  });

  describe("when downstream release jobs check out the frozen source", () => {
    it("pins source jobs without persisting the checkout token", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );
      for (const jobName of [
        "release-metadata",
        "artifact-build",
        "package-assembly",
        "draft-release",
      ] as const) {
        const checkoutSteps = releaseWorkflow.jobs[jobName].steps.filter(
          (step) => step.uses?.startsWith("actions/checkout@") === true,
        );
        assert.equal(checkoutSteps.length, 1, `${jobName} must have one source checkout`);
        assert.equal(checkoutSteps[0]?.with?.["persist-credentials"], false);
        assert.equal(
          checkoutSteps[0]?.with?.ref,
          jobName === "release-metadata"
            ? "${{ inputs.release_commit }}"
            : "${{ needs.release-metadata.outputs.release_commit }}",
        );
      }
      const assemblyCheckout = stepWithName(
        releaseWorkflow.jobs["package-assembly"].steps,
        "Check out repository",
      );
      assert.equal(assemblyCheckout.with?.["fetch-depth"], 0);
    });

    it("revalidates the remote tag before mutating the draft", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );
      const draftSteps = releaseWorkflow.jobs["draft-release"].steps;
      const validationIndex = draftSteps.findIndex(
        ({ name }) => name === "Validate remote release tag",
      );
      const mutationIndex = draftSteps.findIndex(
        ({ name }) => name === "Create or update draft GitHub Release",
      );
      const command = commandForStep(draftSteps, "Validate remote release tag");
      assert.ok(validationIndex < mutationIndex);
      assert.match(command, /gh api "repos\/\$\{GITHUB_REPOSITORY\}\/git\/ref\/tags/u);
      assert.match(command, /while \[\[ "\$\{object_type\}" == "tag" \]\]/u);
      assert.match(command, /test "\$\(remote_tag_commit\)" = "\$\{RELEASE_COMMIT\}"/u);
    });
  });

  describe("when verifying the assembled npm candidate", () => {
    it("runs every carrier from local tarballs before creating the draft release", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );
      const candidateJobNames = [
        "verify-npm-candidate-native",
        "verify-npm-candidate-musl",
        "verify-npm-candidate-wasi",
      ] as const;
      const verifiedPackageNames = [
        ...releaseWorkflow.jobs[
          "verify-npm-candidate-native"
        ].strategy.matrix.include.map(({ package_name: packageName }) => packageName),
        releaseWorkflow.jobs["verify-npm-candidate-musl"].env
          .NPM_RUNTIME_PACKAGE_NAME,
        releaseWorkflow.jobs["verify-npm-candidate-wasi"].env
          .NPM_RUNTIME_PACKAGE_NAME,
      ];

      assert.deepEqual(
        verifiedPackageNames.toSorted(),
        RUNTIME_PACKAGE_DEFINITIONS.map(({ packageName }) => packageName).toSorted(),
      );
      for (const jobName of candidateJobNames) {
        const commands = releaseWorkflow.jobs[jobName].steps
          .flatMap(({ run }) => run ?? [])
          .join("\n");
        assert.match(commands, /\.tgz/u);
        assert.match(commands, /--omit=optional/u);
        assert.match(commands, /unset SPRAWL_GUARD_BINARY/u);
        assert.doesNotMatch(commands, /@kpozin\/sprawl-guard@/u);
        assert.ok(releaseWorkflow.jobs["draft-release"].needs.includes(jobName));
      }

      const wasiComparison = commandForStep(
        releaseWorkflow.jobs["verify-npm-candidate-wasi"].steps,
        "Compare exact candidate native and WASI scans",
      );
      assert.match(wasiComparison, /native-languages\.json/u);
      assert.match(wasiComparison, /wasi-languages\.json/u);
      assert.match(wasiComparison, /config_key === "typescript"/u);
      assert.match(wasiComparison, /cmp native-languages\.json wasi-languages\.json/u);
    });
  });

  describe("when verifying a Cargo registry installation", () => {
    it("uses a fresh Cargo home and scans the pinned Rust repository", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );
      const job = releaseWorkflow.jobs["verify-published-cargo"];
      const fixtureCheckout = stepWithName(job.steps, "Check out the pinned Rust scan fixture");
      const runStep = stepWithName(job.steps, "Run the Cargo-installed CLI");

      assert.equal(job.env.CARGO_HOME, "${{ github.workspace }}/cargo-home");
      assert.equal(fixtureCheckout.with?.repository, "example/scan-fixture");
      assert.equal(
        fixtureCheckout.with?.ref,
        "dc042d2f267bef4b410ef0f71cf9962c0e95a9da",
      );
      assert.match(runStep.run ?? "", /languages --detected --format json/u);
    });
  });

  describe("when verifying the registry-installed WASI fallback", () => {
    it("compares a real scan with the native wrapper", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );
      const job = releaseWorkflow.jobs["verify-published-npm-wasi"];
      const fixture = stepWithName(job.steps, "Check out the pinned npm scan fixture");
      const comparison = stepWithName(
        job.steps,
        "Compare registry-installed native and WASI scans",
      );
      assert.equal(fixture.with?.repository, undefined);
      assert.equal(
        fixture.with?.ref,
        "${{ needs.release-metadata.outputs.release_commit }}",
      );
      assert.equal(fixture.with?.path, "scan-fixture");
      assert.match(comparison.run ?? "", /native-languages\.json/u);
      assert.match(comparison.run ?? "", /wasi-languages\.json/u);
      assert.match(comparison.run ?? "", /config_key === "typescript"/u);
      assert.match(comparison.run ?? "", /cmp native-languages\.json wasi-languages\.json/u);
    });
  });

  describe("when creating the draft GitHub Release", () => {
    it("sets prerelease and latest status from SemVer and the npm channel", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );
      const metadataJob = releaseWorkflow.jobs["release-metadata"];
      const step = stepWithName(
        releaseWorkflow.jobs["draft-release"].steps,
        "Create or update draft GitHub Release",
      );

      assert.equal(
        metadataJob.outputs.github_prerelease,
        "${{ steps.resolve.outputs.github_prerelease }}",
      );
      assert.equal(
        metadataJob.outputs.github_latest,
        "${{ steps.resolve.outputs.github_latest }}",
      );
      assert.doesNotMatch(step.run ?? "", /\[\[.*RELEASE_VERSION.*\*-\*/u);
      assert.match(step.run ?? "", /--prerelease="\$\{GITHUB_PRERELEASE\}"/u);
      assert.match(step.run ?? "", /--latest="\$\{GITHUB_LATEST\}"/u);
      assert.equal(
        step.env?.GITHUB_PRERELEASE,
        "${{ needs.release-metadata.outputs.github_prerelease }}",
      );
      assert.equal(
        step.env?.GITHUB_LATEST,
        "${{ needs.release-metadata.outputs.github_latest }}",
      );
    });
  });

  describe("when verifying packages installed from the npm registry", () => {
    it("runs the wrapper with every native carrier and the WASI fallback", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );
      const verifiedPackageNames = [
        ...releaseWorkflow.jobs["verify-published-npm-native"].strategy.matrix.include.map(
          ({ package_name: packageName }) => packageName,
        ),
        releaseWorkflow.jobs["verify-published-npm-musl"].env.NPM_RUNTIME_PACKAGE_NAME,
        releaseWorkflow.jobs["verify-published-npm-wasi"].env.NPM_RUNTIME_PACKAGE_NAME,
      ];

      assert.deepEqual(
        verifiedPackageNames.toSorted(),
        RUNTIME_PACKAGE_DEFINITIONS.map(({ packageName }) => packageName).toSorted(),
      );
      for (const jobName of [
        "verify-published-npm-native",
        "verify-published-npm-musl",
        "verify-published-npm-wasi",
      ] as const) {
        const job = releaseWorkflow.jobs[jobName];
        const commands = job.steps.flatMap(({ run }) => run ?? []).join("\n");
        assert.match(commands, /npm --prefix/u);
        assert.match(commands, /createRequire\([^\n]*package\.json/u);
        assert.match(commands, /unset SPRAWL_GUARD_BINARY/u);
        assert.doesNotMatch(commands, /npm --prefix [^\n]+ exec -- node/u);
        assert.doesNotMatch(commands, /pnpm --dir/u);
      }
    });
  });
});
