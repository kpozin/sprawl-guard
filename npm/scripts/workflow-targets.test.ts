import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { describe, it } from "node:test";
import { z } from "zod";
import { RUNTIME_PACKAGE_DEFINITIONS, type RuntimePackageDefinition } from "./package-manifests.ts";
import { WHEEL_PLATFORMS, wheelPlatformTag } from "./pypi/wheel-platforms.ts";
import { ciRuntimeTargets } from "./runtime-target-matrix.ts";
import {
  CI_WORKFLOW_SCHEMA,
  CI_WORKFLOW_URL,
  commandForStep,
  parseWorkflow,
  RELEASE_WORKFLOW_SCHEMA,
  RELEASE_WORKFLOW_URL,
  stepWithName,
} from "./workflow-checks/release-workflow.ts";

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


/** Returns the container image that builds and tests a runtime package, if it has one. */
function packageContainer(definition: RuntimePackageDefinition): string | undefined {
  return "container" in definition ? definition.container : undefined;
}

/** Release jobs that install the npm packages and check that the wrapper runs. */
const NPM_INSTALL_CHECK_JOB_NAMES = [
  "verify-npm-candidate-native",
  "verify-npm-candidate-musl",
  "verify-npm-candidate-wasi",
  "verify-published-npm-native",
  "verify-published-npm-musl",
  "verify-published-npm-wasi",
] as const;
/** Native runtime targets other than Linux musl, which the wheel jobs check in an Alpine container. */
const NON_MUSL_NATIVE_SLUGS = RUNTIME_PACKAGE_DEFINITIONS.flatMap((definition) =>
  definition.kind === "native" && definition.slug !== "linux-x64-musl" ? [definition.slug] : [],
);
/** Matches a check that a languages report found TypeScript sources. */
const LANGUAGES_REPORT_CHECK_PATTERN = /check-languages-report\.ts (?:[^\n]|\\\n)*--language typescript/u;

/** Asserts that a script writes two scan reports and compares them byte for byte. */
function assertComparesScanReports(script: string): void {
  const reports = [...script.matchAll(/> (\S+)/gu)].map(([, report = ""]) => report);
  assert.equal(reports.length, 2, "expected one native and one WASI scan report");
  assert.ok(script.includes(`cmp ${reports.join(" ")}`), "expected the two reports to be compared");
}

describe("native runtime workflow coverage", () => {
  describe("when comparing CI with the release artifact matrix", () => {
    const releaseNativeTargets = (): readonly string[] =>
      parseWorkflow(RELEASE_WORKFLOW_URL, RELEASE_WORKFLOW_SCHEMA)
        .jobs["artifact-build"].strategy.matrix.include.filter(
          ({ package_target: packageTarget }) => packageTarget !== "wasm32-wasip1",
        )
        .map(({ rust_target: rustTarget }) => rustTarget);

    describe("with all platforms", () => {
      it("smoke-tests every native release target", () => {
        parseWorkflow(CI_WORKFLOW_URL, CI_WORKFLOW_SCHEMA);

        const ciTargets = ciRuntimeTargets("all").map(({ rust_target: rustTarget }) => rustTarget);

        assert.deepEqual(sortedTargets(ciTargets), sortedTargets(releaseNativeTargets()));
      });
    });

    describe("without darwin", () => {
      it("smoke-tests every native release target except the macOS ones", () => {
        const ciTargets = ciRuntimeTargets("except-darwin").map(
          ({ rust_target: rustTarget }) => rustTarget,
        );

        assert.deepEqual(
          sortedTargets(ciTargets),
          sortedTargets(releaseNativeTargets().filter((target) => !target.endsWith("-apple-darwin"))),
        );
      });
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

  describe("when choosing each job's container", () => {
    const releaseWorkflow = (): z.infer<typeof RELEASE_WORKFLOW_SCHEMA> =>
      parseWorkflow(RELEASE_WORKFLOW_URL, RELEASE_WORKFLOW_SCHEMA);

    it("builds each runtime package in its container", () => {
      const workflowContainers = releaseWorkflow().jobs["artifact-build"].strategy.matrix.include.map(
        ({ container, package_target: packageTarget }) => [packageTarget, container] as const,
      );

      assert.deepEqual(
        new Map(workflowContainers),
        new Map(RUNTIME_PACKAGE_DEFINITIONS.map((definition) => [definition.slug, packageContainer(definition)])),
      );
    });

    for (const jobName of ["verify-npm-candidate-native", "verify-published-npm-native"] as const) {
      it(`runs ${jobName} for each package in its build container`, () => {
        const containersByPackageName = new Map<string, string | undefined>(
          RUNTIME_PACKAGE_DEFINITIONS.map((definition) => [definition.packageName, packageContainer(definition)]),
        );

        for (const { container, package_name: packageName } of releaseWorkflow().jobs[jobName].strategy.matrix.include) {
          assert.equal(container, containersByPackageName.get(packageName), packageName);
        }
      });
    }

    for (const jobName of ["verify-wheel-candidate-native", "verify-published-pypi-native"] as const) {
      it(`runs ${jobName} for each target in its build container`, () => {
        const containersBySlug = new Map<string, string | undefined>(
          RUNTIME_PACKAGE_DEFINITIONS.map((definition) => [definition.slug, packageContainer(definition)]),
        );
        const include = releaseWorkflow().jobs[jobName].strategy.matrix.include;

        assert.deepEqual(include.map(({ package_target: slug }) => slug).toSorted(), NON_MUSL_NATIVE_SLUGS.toSorted());
        for (const { container, package_target: slug } of include) {
          assert.equal(container, containersBySlug.get(slug), slug);
        }
      });
    }
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
        commandForStep(packageAssemblySteps, "Validate release assets and generate checksums"),
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
      assert.equal(releaseWorkflow.on.workflow_dispatch.inputs.verify_pypi.default, false);
    });
  });

  describe("when dispatching a release", () => {
    it("checks the release mode before any other step", () => {
      const steps = parseWorkflow(RELEASE_WORKFLOW_URL, RELEASE_WORKFLOW_SCHEMA).jobs[
        "release-metadata"
      ].steps;

      assert.equal(steps[0]?.name, "Check release mode");
    });

    describe("with each release mode", () => {
      const runModeCheck = (
        releaseMode: string,
        repositoryVisibility: string,
      ): { readonly status: number | null; readonly stdout: string } => {
        const command = commandForStep(
          parseWorkflow(RELEASE_WORKFLOW_URL, RELEASE_WORKFLOW_SCHEMA).jobs["release-metadata"]
            .steps,
          "Check release mode",
        );
        return spawnSync("bash", ["-euo", "pipefail", "-c", command], {
          encoding: "utf8",
          env: {
            PATH: process.env["PATH"],
            RELEASE_MODE: releaseMode,
            REPOSITORY_VISIBILITY: repositoryVisibility,
          },
        });
      };

      it("rejects the form's initial choice", () => {
        const result = runModeCheck("choose", "public");

        assert.equal(result.status, 1);
        assert.match(result.stdout, /::error::Set release_mode to draft or publish/u);
      });

      it("accepts a draft in any repository", () => {
        assert.equal(runModeCheck("draft", "private").status, 0);
      });

      it("accepts publishing from the public repository", () => {
        assert.equal(runModeCheck("publish", "public").status, 0);
      });

      it("rejects publishing from a repository that is not public", () => {
        const result = runModeCheck("publish", "private");

        assert.equal(result.status, 1);
        assert.match(result.stdout, /::error::release_mode publish needs a public repository/u);
      });
    });
  });

  describe("when starting release assembly", () => {
    it("uses the selected tag ref and derives the npm channel by default", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );

      assert.deepEqual(Object.keys(releaseWorkflow.on), ["workflow_dispatch"]);
      assert.deepEqual(
        releaseWorkflow.on.workflow_dispatch.inputs.npm_channel.options,
        ["auto", "latest", "next", "beta", "rc", "canary", "custom"],
      );
      assert.equal(
        releaseWorkflow.on.workflow_dispatch.inputs.npm_channel.default,
        "auto",
      );
      assert.equal(releaseWorkflow.concurrency.group, "sprawl-guard-release");
    });
  });

  describe("when publishing the GitHub Release", () => {
    it("waits for every published-package check to succeed in publish mode", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );
      const finalize = releaseWorkflow.jobs["finalize-release"];
      const publishedChecks = Object.keys(releaseWorkflow.jobs).filter((job) =>
        job.startsWith("verify-published-"),
      );

      assert.ok(publishedChecks.length > 0);
      assert.match(finalize.if, /inputs\.release_mode == 'publish'/u);
      for (const job of publishedChecks) {
        assert.ok(finalize.needs.includes(job), `finalize-release must need ${job}`);
        assert.ok(
          finalize.if.includes(`needs.${job}.result == 'success'`),
          `finalize-release must require ${job} to succeed`,
        );
      }
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
    it("isolates attestation authority from artifact builders", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );
      const artifactJob = releaseWorkflow.jobs["artifact-build"];
      assert.deepEqual(artifactJob.permissions, { contents: "read" });
      const attestationJob = releaseWorkflow.jobs["artifact-attestation"];
      assert.deepEqual(attestationJob.needs.toSorted(), [
        "cargo-candidate",
        "package-assembly",
        "verify-npm-candidate-musl",
        "verify-npm-candidate-native",
        "verify-npm-candidate-wasi",
        "verify-wheel-candidate-musl",
        "verify-wheel-candidate-native",
      ]);
      assert.ok(
        releaseWorkflow.jobs["draft-release"].needs.includes("artifact-attestation"),
      );
      assert.match(
        releaseWorkflow.jobs["draft-release"].if,
        /artifact-attestation\.result == 'success'.*artifact-attestation\.result == 'skipped'/u,
      );
      assert.deepEqual(attestationJob.permissions, {
        "artifact-metadata": "write",
        attestations: "write",
        contents: "read",
        "id-token": "write",
      });
      const attestation = stepWithName(
        attestationJob.steps,
        "Attest release artifact provenance",
      );
      const subjects = String(attestation.with?.["subject-path"]);
      assert.match(subjects, /attestation-subjects\/github-release-assets\/\*/u);
      assert.match(subjects, /attestation-subjects\/cargo\/\*\.crate/u);
      assert.match(subjects, /attestation-subjects\/npm\/\*\.tgz/u);
      assert.match(subjects, /attestation-subjects\/pypi\/\*\.whl/u);
      assert.doesNotMatch(String(attestation.with?.["subject-path"]), /release-.*\.tar/u);
      assert.match(attestationJob.if, /repository\.visibility == 'public'/u);
    });

    it("keeps candidate assembly free of publication authority", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );
      assert.deepEqual(releaseWorkflow.jobs["cargo-candidate"].permissions, {
        contents: "read",
      });
      const cargoUpload = stepWithName(
        releaseWorkflow.jobs["cargo-candidate"].steps,
        "Upload Cargo publication candidate",
      );
      assert.match(String(cargoUpload.with?.path), /sprawl-guard-lib-/u);
      assert.ok(
        String(cargoUpload.with?.path).includes(
          "target/package/sprawl-guard-${{ needs.release-metadata.outputs.release_version }}.crate",
        ),
      );
      assert.ok(releaseWorkflow.jobs["draft-release"].needs.includes("cargo-candidate"));
      assert.match(
        releaseWorkflow.jobs["draft-release"].if,
        /cargo-candidate\.result == 'success'/u,
      );
      assert.deepEqual(releaseWorkflow.jobs["package-assembly"].permissions, {
        contents: "read",
      });
      const releaseAssetUpload = stepWithName(
        releaseWorkflow.jobs["package-assembly"].steps,
        "Upload GitHub Release assets",
      );
      assert.equal(releaseAssetUpload.with?.name, "github-release-assets");
      assert.equal(
        releaseAssetUpload.with?.path,
        "release-artifact-archives/github-release-assets",
      );
      assert.equal(
        stepWithName(
          releaseWorkflow.jobs["draft-release"].steps,
          "Download validated GitHub Release assets",
        ).with?.name,
        "github-release-assets",
      );
      assert.equal(
        releaseWorkflow.jobs["draft-release"].steps.some(
          ({ name }) => name === "Validate release assets and generate checksums",
        ),
        false,
      );
    });

    it("grants short-lived registry authority only after the draft is ready", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );
      const cargoJob = releaseWorkflow.jobs["publish-cargo"];
      const npmJob = releaseWorkflow.jobs["publish-npm"];
      const pypiJob = releaseWorkflow.jobs["publish-pypi"];
      for (const job of [cargoJob, npmJob, pypiJob]) {
        assert.deepEqual(job.permissions, {
          contents: "read",
          "id-token": "write",
        });
        assert.equal(job.if, "${{ inputs.release_mode == 'publish' }}");
        assert.ok(job.needs.includes("draft-release"));
      }
      // The two registries publish at the same time, so their approvals can be given together.
      assert.equal(npmJob.needs.includes("publish-cargo"), false);
      assert.deepEqual(pypiJob.needs.toSorted(), ["draft-release", "package-assembly", "release-metadata"]);
      const pypiPublish = stepWithName(pypiJob.steps, "Publish PyPI wheels");
      assert.match(pypiPublish.uses ?? "", /^pypa\/gh-action-pypi-publish@[0-9a-f]{40}$/u);
      assert.deepEqual(pypiPublish.with, { "packages-dir": "pypi-publication-candidate" });

      const cargoPublish = stepWithName(cargoJob.steps, "Publish Cargo crates");
      assert.equal(
        cargoPublish.env?.CARGO_REGISTRY_TOKEN,
        "${{ steps.crates-io-auth.outputs.token }}",
      );

      const npmPublish = stepWithName(npmJob.steps, "Publish npm packages");
      assert.match(npmPublish.run ?? "", /release:publish-npm-tag/u);
      assert.equal(npmPublish.env?.NODE_AUTH_TOKEN, undefined);
      assert.doesNotMatch(npmPublish.run ?? "", /dist-tag/u);
      for (const job of [cargoJob, npmJob]) {
        assert.match(
          commandForStep(job.steps, "Install npm workspace dependencies"),
          /pnpm install --frozen-lockfile --ignore-scripts/u,
        );
      }
      assert.doesNotMatch(JSON.stringify([cargoJob, npmJob, pypiJob]), /secrets\./u);
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
        "cargo-candidate",
        "package-assembly",
        "draft-release",
        "publish-cargo",
        "publish-npm",
        ...NPM_INSTALL_CHECK_JOB_NAMES,
      ] as const) {
        const checkoutSteps = releaseWorkflow.jobs[jobName].steps.filter(
          (step) => step.uses?.startsWith("actions/checkout@") === true,
        );
        assert.equal(checkoutSteps.length, 1, `${jobName} must have one source checkout`);
        assert.equal(checkoutSteps[0]?.with?.["persist-credentials"], false);
        assert.equal(
          checkoutSteps[0]?.with?.ref,
          jobName === "release-metadata"
            ? "${{ github.sha }}"
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
        assert.match(commands, /--candidate-directory /u);
        assert.doesNotMatch(commands, /--dist-tag /u);
        assert.ok(releaseWorkflow.jobs["draft-release"].needs.includes(jobName));
      }

      const wasiComparison = commandForStep(
        releaseWorkflow.jobs["verify-npm-candidate-wasi"].steps,
        "Compare exact candidate native and WASI scans",
      );
      assert.match(wasiComparison, LANGUAGES_REPORT_CHECK_PATTERN);
      assertComparesScanReports(wasiComparison);
    });
  });

  describe("when verifying the PyPI wheel candidate", () => {
    it("installs every wheel from the local candidate before creating the draft release", () => {
      const releaseWorkflow = parseWorkflow(RELEASE_WORKFLOW_URL, RELEASE_WORKFLOW_SCHEMA);
      const nativeJob = releaseWorkflow.jobs["verify-wheel-candidate-native"];
      const muslJob = releaseWorkflow.jobs["verify-wheel-candidate-musl"];

      assert.deepEqual(
        [...nativeJob.strategy.matrix.include.map(({ wheel_tag: tag }) => tag), muslJob.env.WHEEL_TAG].toSorted(),
        Object.values(WHEEL_PLATFORMS).map(wheelPlatformTag).toSorted(),
      );
      for (const { package_target: slug, wheel_tag: tag } of nativeJob.strategy.matrix.include) {
        assert.ok(Object.hasOwn(WHEEL_PLATFORMS, slug), slug);
        assert.equal(tag, wheelPlatformTag(WHEEL_PLATFORMS[slug as keyof typeof WHEEL_PLATFORMS]), slug);
      }
      for (const jobName of ["verify-wheel-candidate-native", "verify-wheel-candidate-musl"] as const) {
        const commands = releaseWorkflow.jobs[jobName].steps.flatMap(({ run }) => run ?? []).join("\n");
        assert.match(commands, /--no-index --find-links /u);
        assert.ok(releaseWorkflow.jobs["draft-release"].needs.includes(jobName));
        assert.match(releaseWorkflow.jobs["draft-release"].if, new RegExp(`${jobName}\\.result == 'success'`, "u"));
      }
    });

    it("builds the wheels from the restored release binaries", () => {
      const releaseWorkflow = parseWorkflow(RELEASE_WORKFLOW_URL, RELEASE_WORKFLOW_SCHEMA);
      const steps = releaseWorkflow.jobs["package-assembly"].steps;

      assert.match(
        commandForStep(steps, "Build PyPI wheels"),
        /^pnpm --dir npm run release:build-wheels \.\.\/release-artifacts \.\.\/pypi-publication-candidate "\$\{RELEASE_VERSION\}"$/u,
      );
      assert.match(commandForStep(steps, "Install Python release tools"), /--require-hashes/u);
      assert.equal(stepWithName(steps, "Upload PyPI publication candidate").with?.name, "pypi-publication-candidate");
    });
  });

  describe("when verifying wheels installed from PyPI", () => {
    it("installs the release's PyPI version on every native target", () => {
      const releaseWorkflow = parseWorkflow(RELEASE_WORKFLOW_URL, RELEASE_WORKFLOW_SCHEMA);

      assert.equal(
        releaseWorkflow.jobs["release-metadata"].outputs.python_version,
        "${{ steps.resolve.outputs.python_version }}",
      );
      for (const jobName of ["verify-published-pypi-native", "verify-published-pypi-musl"] as const) {
        const job = releaseWorkflow.jobs[jobName];
        const commands = job.steps.flatMap(({ run }) => run ?? []).join("\n");
        assert.match(commands, /"sprawl-guard==\$\{PYTHON_VERSION\}"/u);
        assert.doesNotMatch(commands, /--find-links /u);
        assert.match(job.if, /needs\.publish-pypi\.result == 'success'/u);
      }
    });
  });

  describe("when verifying a Cargo registry installation", () => {
    it("uses a fresh Cargo home and scans the release commit's Rust sources", () => {
      const releaseWorkflow = parseWorkflow(
        RELEASE_WORKFLOW_URL,
        RELEASE_WORKFLOW_SCHEMA,
      );
      const job = releaseWorkflow.jobs["verify-published-cargo"];
      const fixtureCheckout = stepWithName(job.steps, "Check out the release commit to scan");
      const runStep = stepWithName(job.steps, "Run the Cargo-installed CLI");

      assert.equal(job.env.CARGO_HOME, "${{ github.workspace }}/cargo-home");
      assert.equal(fixtureCheckout.with?.repository, undefined);
      assert.equal(
        fixtureCheckout.with?.ref,
        "${{ needs.release-metadata.outputs.release_commit }}",
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
      const fixture = job.steps.find((step) => step.uses?.startsWith("actions/checkout@") === true);
      const comparison = stepWithName(
        job.steps,
        "Compare registry-installed native and WASI scans",
      );
      assert.ok(fixture !== undefined);
      assert.equal(fixture.with?.repository, undefined);
      assert.equal(
        fixture.with?.ref,
        "${{ needs.release-metadata.outputs.release_commit }}",
      );
      assert.match(comparison.run ?? "", LANGUAGES_REPORT_CHECK_PATTERN);
      assertComparesScanReports(comparison.run ?? "");
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
        assert.match(commands, /--dist-tag "\$\{NPM_DIST_TAG\}"/u);
        assert.doesNotMatch(commands, /--candidate-directory /u);
      }
    });
  });
});
