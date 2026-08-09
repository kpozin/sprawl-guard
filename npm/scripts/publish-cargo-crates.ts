#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { PACKAGE_VERSION } from "./package-manifests.ts";
import {
  inspectReleaseSource,
  parseReleaseSourceExpectation,
  type ReleaseSourceExpectation,
} from "./release-source.ts";

const CRATES_IO_BASE_URL = "https://crates.io/api/v1/crates";
const CRATES_IO_REGISTRY_NAME = "crates-io";
const CRATES_IO_USER_AGENT = `sprawl-guard-release/${PACKAGE_VERSION} (https://github.com/kpozin/sprawl-guard)`;
const CARGO_PUBLICATION_MANIFEST_FILE = "sprawl-guard-publication.json";
const CARGO_COMMAND_MAXIMUM_OUTPUT_BYTES = 1_048_576;
const CARGO_COMMAND_TIMEOUT_MILLISECONDS = 600_000;
const CARGO_PUBLICATION_MANIFEST_SCHEMA = z
  .object({
    schemaVersion: z.literal(1),
    releaseCommit: z.string(),
    releaseTag: z.string(),
    releaseVersion: z.string(),
    packages: z.array(
      z
        .object({
          checksum: z.string().regex(/^[0-9a-f]{64}$/u),
          packageName: z.enum(["sprawl-guard-lib", "sprawl-guard"]),
        })
        .strict(),
    ),
  })
  .strict();
const REGISTRY_VISIBILITY_ATTEMPTS = 60;
const REGISTRY_VISIBILITY_INTERVAL_MILLISECONDS = 2_000;
const REGISTRY_ARCHIVE_MAXIMUM_BYTES = 16_777_216;
const REGISTRY_REQUEST_TIMEOUT_MILLISECONDS = 30_000;
const WORKSPACE_ROOT = path.resolve(
  fileURLToPath(new URL("../..", import.meta.url)),
);

export type CargoPackageName = "sprawl-guard-lib" | "sprawl-guard";

type CargoRegistryReadiness =
  { readonly kind: "archive" } | { readonly kind: "archive_and_index" };

/** One Cargo workspace package and the registry state required before continuing. */
export interface CargoPublicationRequest {
  readonly archivePath: string;
  readonly packageName: CargoPackageName;
  readonly readiness: CargoRegistryReadiness;
}

type RemoteArchiveChecksum =
  | { readonly kind: "absent" }
  | { readonly kind: "present"; readonly checksum: string };

/** Result of inspecting one immutable crate archive on crates.io. */
export type RemoteCargoArchiveInspection =
  | { readonly kind: "ok"; readonly checksum: RemoteArchiveChecksum }
  | { readonly kind: "transient_failure"; readonly detail: string }
  | { readonly kind: "permanent_failure"; readonly detail: string };

type CargoIndexInspection =
  | { readonly kind: "ready" }
  | { readonly kind: "transient_failure"; readonly detail: string }
  | { readonly kind: "permanent_failure"; readonly detail: string };

/** Decision for reconciling one immutable crates.io version. */
export type CargoArchiveChecksumDecision =
  | { readonly kind: "publish" }
  | { readonly kind: "skip" }
  | {
      readonly kind: "checksum_mismatch";
      readonly localChecksum: string;
      readonly remoteChecksum: string;
    };

type PackageCrateResult =
  | { readonly kind: "ok"; readonly checksum: string }
  | { readonly kind: "failed"; readonly detail: string };
type SuccessfulPackageCrateResult = Extract<
  PackageCrateResult,
  { readonly kind: "ok" }
>;

type PublishCrateResult =
  | { readonly kind: "ok" }
  | { readonly kind: "failed"; readonly detail: string };

export type PublishCargoCratesResult =
  | { readonly kind: "ok" }
  | {
      readonly kind: "failed";
      readonly packageName: CargoPackageName;
      readonly detail: string;
    };

/** Registry and Cargo operations used by the Cargo publication state machine. */
export interface CargoPublicationOperations {
  readonly inspectCargoIndex: (
    packageName: CargoPackageName,
  ) => CargoIndexInspection;
  readonly inspectRemoteArchive: (
    packageName: CargoPackageName,
  ) => Promise<RemoteCargoArchiveInspection>;
  readonly packageCrate: (
    request: CargoPublicationRequest,
  ) => PackageCrateResult;
  readonly publishCrate: (
    request: CargoPublicationRequest,
  ) => PublishCrateResult;
  readonly sleepBeforeRetry: () => Promise<void>;
}

/** Returns the implementation library followed by its dependent CLI crate. */
export function cargoPublicationRequests(args: {
  readonly workspaceRoot: string;
}): readonly CargoPublicationRequest[] {
  return (["sprawl-guard-lib", "sprawl-guard"] as const).map((packageName) => ({
    archivePath: path.join(
      args.workspaceRoot,
      "target",
      "package",
      `${packageName}-${PACKAGE_VERSION}.crate`,
    ),
    packageName,
    readiness:
      packageName === "sprawl-guard-lib"
        ? { kind: "archive_and_index" as const }
        : { kind: "archive" as const },
  }));
}

/** Decides whether a crates.io version is new, safely resumed, or conflicting. */
export function reconcileCargoArchiveChecksum(args: {
  readonly localChecksum: string;
  readonly remoteChecksum: RemoteArchiveChecksum;
}): CargoArchiveChecksumDecision {
  if (args.remoteChecksum.kind === "absent") {
    return { kind: "publish" };
  }
  if (args.remoteChecksum.checksum === args.localChecksum) {
    return { kind: "skip" };
  }
  return {
    kind: "checksum_mismatch",
    localChecksum: args.localChecksum,
    remoteChecksum: args.remoteChecksum.checksum,
  };
}

/** Runs the ordered, resumable Cargo publication state machine through supplied operations. */
export async function publishCargoPublicationRequests(args: {
  readonly operations: CargoPublicationOperations;
  readonly requests: readonly CargoPublicationRequest[];
}): Promise<PublishCargoCratesResult> {
  const [dependency, ...dependents] = args.requests;
  if (dependency === undefined) {
    return { kind: "ok" };
  }
  const packagedDependency = args.operations.packageCrate(dependency);
  if (packagedDependency.kind === "failed") {
    return failedPublication(dependency, packagedDependency.detail);
  }
  const dependencyInspection = await inspectBeforePublication(
    dependency.packageName,
    args.operations,
  );
  if (dependencyInspection.kind !== "ok") {
    return failedPublication(dependency, dependencyInspection.detail);
  }
  const dependencyDecision = reconcileCargoArchiveChecksum({
    localChecksum: packagedDependency.checksum,
    remoteChecksum: dependencyInspection.checksum,
  });
  if (dependencyDecision.kind === "checksum_mismatch") {
    return checksumMismatch(dependency, dependencyDecision);
  }

  const dependentInspections = new Map<
    CargoPackageName,
    RemoteArchiveChecksum
  >();
  for (const dependent of dependents) {
    const inspected = await inspectBeforePublication(
      dependent.packageName,
      args.operations,
    );
    if (inspected.kind !== "ok") {
      return failedPublication(dependent, inspected.detail);
    }
    if (
      dependencyDecision.kind === "publish" &&
      inspected.checksum.kind === "present"
    ) {
      return failedPublication(
        dependent,
        "dependent crate already exists while its frozen library candidate is absent",
      );
    }
    dependentInspections.set(dependent.packageName, inspected.checksum);
  }

  let currentDependencyDecision = dependencyDecision;
  if (currentDependencyDecision.kind === "publish") {
    const currentInspection = await reinspectChecksum(
      dependency,
      args.operations,
    );
    if (currentInspection.kind === "failed") {
      return currentInspection;
    }
    const recheckedDecision = reconcileCargoArchiveChecksum({
      localChecksum: packagedDependency.checksum,
      remoteChecksum: currentInspection.checksum,
    });
    if (recheckedDecision.kind === "checksum_mismatch") {
      return checksumMismatch(dependency, recheckedDecision);
    }
    currentDependencyDecision = recheckedDecision;
  }
  const dependencyPublication = await publishPreparedCargoCrate({
    decision: currentDependencyDecision,
    operations: args.operations,
    packaged: packagedDependency,
    request: dependency,
  });
  if (dependencyPublication !== null) {
    return dependencyPublication;
  }

  for (const dependent of dependents) {
    const packaged = args.operations.packageCrate(dependent);
    if (packaged.kind === "failed") {
      return failedPublication(dependent, packaged.detail);
    }
    const initialInspection = dependentInspections.get(dependent.packageName);
    if (initialInspection === undefined) {
      return failedPublication(
        dependent,
        "missing dependent registry preflight",
      );
    }
    const currentInspection =
      initialInspection.kind === "present"
        ? { kind: "ok" as const, checksum: initialInspection }
        : await reinspectChecksum(dependent, args.operations);
    if (currentInspection.kind === "failed") {
      return currentInspection;
    }
    const decision = reconcileCargoArchiveChecksum({
      localChecksum: packaged.checksum,
      remoteChecksum: currentInspection.checksum,
    });
    if (decision.kind === "checksum_mismatch") {
      return checksumMismatch(dependent, decision);
    }
    const publication = await publishPreparedCargoCrate({
      decision,
      operations: args.operations,
      packaged,
      request: dependent,
    });
    if (publication !== null) {
      return publication;
    }
  }
  return { kind: "ok" };
}

async function reinspectChecksum(
  request: CargoPublicationRequest,
  operations: CargoPublicationOperations,
): Promise<
  | { readonly kind: "ok"; readonly checksum: RemoteArchiveChecksum }
  | Exclude<PublishCargoCratesResult, { readonly kind: "ok" }>
> {
  const inspected = await inspectBeforePublication(
    request.packageName,
    operations,
  );
  return inspected.kind === "ok"
    ? { kind: "ok", checksum: inspected.checksum }
    : failedPublication(request, inspected.detail);
}

function checksumMismatch(
  request: CargoPublicationRequest,
  decision: Extract<
    CargoArchiveChecksumDecision,
    { readonly kind: "checksum_mismatch" }
  >,
): Exclude<PublishCargoCratesResult, { readonly kind: "ok" }> {
  return failedPublication(
    request,
    `registry checksum ${decision.remoteChecksum} differs from packaged checksum ${decision.localChecksum}`,
  );
}

async function publishPreparedCargoCrate(args: {
  readonly decision: Exclude<
    CargoArchiveChecksumDecision,
    { readonly kind: "checksum_mismatch" }
  >;
  readonly operations: CargoPublicationOperations;
  readonly packaged: SuccessfulPackageCrateResult;
  readonly request: CargoPublicationRequest;
}): Promise<Exclude<PublishCargoCratesResult, { readonly kind: "ok" }> | null> {
  if (args.decision.kind === "publish") {
    const published = args.operations.publishCrate(args.request);
    if (published.kind === "failed") {
      return failedPublication(args.request, published.detail);
    }
    const visible = await waitForPublishedArchive(
      args.request.packageName,
      args.packaged.checksum,
      args.operations,
    );
    if (visible !== null) {
      return failedPublication(args.request, visible);
    }
    console.log(`published ${args.request.packageName}@${PACKAGE_VERSION}`);
  } else {
    console.log(
      `already published with matching checksum: ${args.request.packageName}@${PACKAGE_VERSION}`,
    );
  }
  if (args.request.readiness.kind === "archive_and_index") {
    const indexReady = await waitForCargoIndex(
      args.request.packageName,
      args.operations,
    );
    if (indexReady !== null) {
      return failedPublication(args.request, indexReady);
    }
  }
  return null;
}

/** Publishes absent crates and safely skips byte-identical versions from a resumed release. */
export async function publishCargoCrates(args: {
  readonly expectation: ReleaseSourceExpectation;
  readonly workspaceRoot: string;
}): Promise<PublishCargoCratesResult> {
  const candidate = validateCargoPublicationCandidate(args);
  if (candidate.kind === "failed") {
    return candidate;
  }
  const frozenCheckout = createFrozenReleaseCheckout(args);
  if (frozenCheckout.kind === "failed") {
    return frozenCheckout;
  }
  const publication = await publishCargoPublicationRequests({
    operations: productionOperations(
      frozenCheckout.workspaceRoot,
      candidate.checksums,
      args.expectation,
    ),
    requests: cargoPublicationRequests({
      workspaceRoot: frozenCheckout.workspaceRoot,
    }),
  });
  try {
    fs.rmSync(frozenCheckout.temporaryRoot, { force: true, recursive: true });
  } catch (cause) {
    const detail = `failed to remove frozen release checkout ${frozenCheckout.temporaryRoot}: ${String(cause)}`;
    return publication.kind === "ok"
      ? { kind: "failed", packageName: "sprawl-guard-lib", detail }
      : { ...publication, detail: `${publication.detail}; ${detail}` };
  }
  return publication;
}

/** Clones the reviewed commit into an isolated checkout used only for Cargo publication. */
export function createFrozenReleaseCheckout(args: {
  readonly expectation: ReleaseSourceExpectation;
  readonly workspaceRoot: string;
}):
  | {
      readonly kind: "ok";
      readonly temporaryRoot: string;
      readonly workspaceRoot: string;
    }
  | Exclude<PublishCargoCratesResult, { readonly kind: "ok" }> {
  let temporaryRoot: string;
  try {
    temporaryRoot = fs.mkdtempSync(
      path.join(os.tmpdir(), "sprawl-guard-cargo-release-"),
    );
  } catch (cause) {
    return {
      kind: "failed",
      packageName: "sprawl-guard-lib",
      detail: `failed to create frozen release checkout: ${String(cause)}`,
    };
  }
  const workspaceRoot = path.join(temporaryRoot, "checkout");
  const cloned = runGit(
    [
      "clone",
      "--quiet",
      "--no-checkout",
      "--no-local",
      args.workspaceRoot,
      workspaceRoot,
    ],
    temporaryRoot,
  );
  const checkedOut =
    cloned.kind === "ok"
      ? runGit(
          ["checkout", "--quiet", "--detach", args.expectation.commit],
          workspaceRoot,
        )
      : cloned;
  const inspected =
    checkedOut.kind === "ok"
      ? inspectReleaseSource({ expectation: args.expectation, workspaceRoot })
      : checkedOut;
  if (inspected.kind === "ok") {
    return { kind: "ok", temporaryRoot, workspaceRoot };
  }
  let cleanupFailure: unknown;
  try {
    fs.rmSync(temporaryRoot, { force: true, recursive: true });
  } catch (cause) {
    cleanupFailure = cause;
  }
  return {
    kind: "failed",
    packageName: "sprawl-guard-lib",
    detail: [
      `failed to create frozen release checkout: ${JSON.stringify(inspected)}`,
      ...(cleanupFailure === undefined
        ? []
        : [
            `failed to remove temporary checkout ${temporaryRoot}: ${String(cleanupFailure)}`,
          ]),
    ].join("; "),
  };
}

function failedPublication(
  request: CargoPublicationRequest,
  detail: string,
): Extract<PublishCargoCratesResult, { readonly kind: "failed" }> {
  return {
    kind: "failed",
    packageName: request.packageName,
    detail,
  };
}

async function inspectBeforePublication(
  packageName: CargoPackageName,
  operations: CargoPublicationOperations,
): Promise<RemoteCargoArchiveInspection> {
  for (let attempt = 0; attempt < REGISTRY_VISIBILITY_ATTEMPTS; attempt += 1) {
    const inspected = await operations.inspectRemoteArchive(packageName);
    if (inspected.kind !== "transient_failure") {
      return inspected;
    }
    if (attempt + 1 < REGISTRY_VISIBILITY_ATTEMPTS) {
      await operations.sleepBeforeRetry();
    }
  }
  return {
    kind: "permanent_failure",
    detail: `crates.io inspection for ${packageName}@${PACKAGE_VERSION} exceeded the retry budget`,
  };
}

async function waitForPublishedArchive(
  packageName: CargoPackageName,
  expectedChecksum: string,
  operations: CargoPublicationOperations,
): Promise<string | null> {
  for (let attempt = 0; attempt < REGISTRY_VISIBILITY_ATTEMPTS; attempt += 1) {
    const inspected = await operations.inspectRemoteArchive(packageName);
    if (inspected.kind === "permanent_failure") {
      return inspected.detail;
    }
    if (inspected.kind === "ok" && inspected.checksum.kind === "present") {
      return inspected.checksum.checksum === expectedChecksum
        ? null
        : `published checksum ${inspected.checksum.checksum} differs from uploaded checksum ${expectedChecksum}`;
    }
    if (attempt + 1 < REGISTRY_VISIBILITY_ATTEMPTS) {
      await operations.sleepBeforeRetry();
    }
  }
  return `crates.io did not serve ${packageName}@${PACKAGE_VERSION} before the visibility timeout`;
}

async function waitForCargoIndex(
  packageName: CargoPackageName,
  operations: CargoPublicationOperations,
): Promise<string | null> {
  for (let attempt = 0; attempt < REGISTRY_VISIBILITY_ATTEMPTS; attempt += 1) {
    const inspected = operations.inspectCargoIndex(packageName);
    if (inspected.kind === "ready") {
      return null;
    }
    if (inspected.kind === "permanent_failure") {
      return inspected.detail;
    }
    if (attempt + 1 < REGISTRY_VISIBILITY_ATTEMPTS) {
      await operations.sleepBeforeRetry();
    }
  }
  return `Cargo could not resolve ${packageName}@${PACKAGE_VERSION} before the visibility timeout`;
}

function packageCrate(request: CargoPublicationRequest): PackageCrateResult {
  try {
    const status = fs.lstatSync(request.archivePath);
    if (!status.isFile() || status.nlink !== 1) {
      return {
        kind: "failed",
        detail: `crate candidate is not an independent regular file: ${request.archivePath}`,
      };
    }
    return {
      kind: "ok",
      checksum: createHash("sha256")
        .update(fs.readFileSync(request.archivePath))
        .digest("hex"),
    };
  } catch (cause) {
    return {
      kind: "failed",
      detail: `failed to hash ${request.archivePath}: ${String(cause)}`,
    };
  }
}

/** Downloads and incrementally hashes one bounded crates.io archive response. */
export async function inspectRemoteCargoArchive(
  packageName: CargoPackageName,
): Promise<RemoteCargoArchiveInspection> {
  let response: Response;
  try {
    response = await fetch(
      `${CRATES_IO_BASE_URL}/${packageName}/${PACKAGE_VERSION}/download`,
      {
        headers: { "User-Agent": CRATES_IO_USER_AGENT },
        redirect: "follow",
        signal: AbortSignal.timeout(REGISTRY_REQUEST_TIMEOUT_MILLISECONDS),
      },
    );
  } catch (cause) {
    return {
      kind: "transient_failure",
      detail: `failed to query crates.io: ${String(cause)}`,
    };
  }
  if (response.status === 404) {
    return { kind: "ok", checksum: { kind: "absent" } };
  }
  if (!response.ok) {
    const kind =
      response.status === 408 ||
      response.status === 429 ||
      response.status >= 500
        ? "transient_failure"
        : "permanent_failure";
    return {
      kind,
      detail: `crates.io returned HTTP ${response.status}`,
    };
  }
  return hashRemoteArchive(response);
}

async function hashRemoteArchive(
  response: Response,
): Promise<RemoteCargoArchiveInspection> {
  const declaredLength = response.headers.get("content-length");
  if (
    declaredLength !== null &&
    (!/^\d+$/u.test(declaredLength) ||
      Number(declaredLength) > REGISTRY_ARCHIVE_MAXIMUM_BYTES)
  ) {
    return {
      kind: "permanent_failure",
      detail: `crates.io archive length is invalid or exceeds ${REGISTRY_ARCHIVE_MAXIMUM_BYTES} bytes`,
    };
  }
  if (response.body === null) {
    return {
      kind: "transient_failure",
      detail: "crates.io returned an archive response without a body",
    };
  }
  const checksum = createHash("sha256");
  let receivedBytes = 0;
  try {
    for await (const chunk of response.body) {
      receivedBytes += chunk.byteLength;
      if (receivedBytes > REGISTRY_ARCHIVE_MAXIMUM_BYTES) {
        return {
          kind: "permanent_failure",
          detail: `crates.io archive exceeds ${REGISTRY_ARCHIVE_MAXIMUM_BYTES} bytes`,
        };
      }
      checksum.update(chunk);
    }
  } catch (cause) {
    return {
      kind: "transient_failure",
      detail: `failed to read crates.io response: ${String(cause)}`,
    };
  }
  return {
    kind: "ok",
    checksum: { kind: "present", checksum: checksum.digest("hex") },
  };
}

function inspectCargoIndex(
  packageName: CargoPackageName,
  workspaceRoot: string,
): CargoIndexInspection {
  const inspected = runCargo(
    [
      "info",
      "--registry",
      CRATES_IO_REGISTRY_NAME,
      `${packageName}@${PACKAGE_VERSION}`,
    ],
    workspaceRoot,
  );
  if (inspected.kind === "ok") {
    return { kind: "ready" };
  }
  return inspected.spawnFailed
    ? inspected.timedOut
      ? { kind: "transient_failure", detail: inspected.detail }
      : { kind: "permanent_failure", detail: inspected.detail }
    : { kind: "transient_failure", detail: inspected.detail };
}

function publishCrate(
  request: CargoPublicationRequest,
  workspaceRoot: string,
): PublishCrateResult {
  const published = runCargo(
    cargoPublishArguments(request.packageName),
    workspaceRoot,
  );
  return published.kind === "ok" ? { kind: "ok" } : published;
}

/** Returns the dependency-execution-free Cargo upload arguments. */
export function cargoPublishArguments(
  packageName: CargoPackageName,
): readonly string[] {
  return [
    "publish",
    "--locked",
    "--no-verify",
    "--registry",
    CRATES_IO_REGISTRY_NAME,
    "--package",
    packageName,
  ];
}

/** Returns the credential-free Cargo packaging arguments for a first release. */
export function cargoPackageArguments(
  packageName: CargoPackageName,
): readonly string[] {
  return [
    "package",
    "--locked",
    "--no-verify",
    "--registry",
    CRATES_IO_REGISTRY_NAME,
    "--package",
    packageName,
  ];
}

/** Packages both Cargo candidates before the first registry upload. */
export function prepareCargoCrates(args: {
  readonly expectation: ReleaseSourceExpectation;
  readonly workspaceRoot: string;
}): PublishCargoCratesResult {
  const requests = cargoPublicationRequests(args);
  const packaged = runCargo(
    [
      "package",
      "--locked",
      "--no-verify",
      "--registry",
      CRATES_IO_REGISTRY_NAME,
      "--workspace",
    ],
    args.workspaceRoot,
  );
  if (packaged.kind === "failed") {
    return {
      kind: "failed",
      packageName: "sprawl-guard-lib",
      detail: packaged.detail,
    };
  }
  const packages: Array<{ checksum: string; packageName: CargoPackageName }> =
    [];
  for (const request of requests) {
    const packaged = packageCrate(request);
    if (packaged.kind === "failed") {
      return failedPublication(request, packaged.detail);
    }
    packages.push({
      checksum: packaged.checksum,
      packageName: request.packageName,
    });
  }
  const manifestPath = cargoPublicationManifestPath(args.workspaceRoot);
  const temporaryManifestPath = `${manifestPath}.tmp-${process.pid}`;
  try {
    fs.writeFileSync(
      temporaryManifestPath,
      `${JSON.stringify(
        {
          schemaVersion: 1,
          releaseCommit: args.expectation.commit,
          releaseTag: args.expectation.tag,
          releaseVersion: PACKAGE_VERSION,
          packages,
        },
        null,
        2,
      )}\n`,
      { flag: "wx" },
    );
    if (fs.existsSync(manifestPath)) {
      const existingManifest = fs.lstatSync(manifestPath);
      if (!existingManifest.isFile() || existingManifest.nlink !== 1) {
        throw new Error(
          "existing Cargo publication candidate is not an independent regular file",
        );
      }
      fs.rmSync(manifestPath);
    }
    fs.renameSync(temporaryManifestPath, manifestPath);
  } catch (cause) {
    let cleanupFailure: unknown;
    try {
      fs.rmSync(temporaryManifestPath, { force: true });
    } catch (cleanupCause) {
      cleanupFailure = cleanupCause;
    }
    return {
      kind: "failed",
      packageName: "sprawl-guard-lib",
      detail: [
        `failed to write Cargo publication candidate: ${String(cause)}`,
        ...(cleanupFailure === undefined
          ? []
          : [
              `failed to remove ${temporaryManifestPath}: ${String(cleanupFailure)}`,
            ]),
      ].join("; "),
    };
  }
  return { kind: "ok" };
}

function cargoPublicationManifestPath(workspaceRoot: string): string {
  return path.join(
    workspaceRoot,
    "target",
    "package",
    CARGO_PUBLICATION_MANIFEST_FILE,
  );
}

/** Validates the frozen Cargo manifest and archive before any registry operation. */
export function validateCargoPublicationCandidate(args: {
  readonly expectation: ReleaseSourceExpectation;
  readonly workspaceRoot: string;
}):
  | {
      readonly kind: "ok";
      readonly checksums: ReadonlyMap<CargoPackageName, string>;
    }
  | Exclude<PublishCargoCratesResult, { readonly kind: "ok" }> {
  let json: unknown;
  try {
    const manifestPath = cargoPublicationManifestPath(args.workspaceRoot);
    const status = fs.lstatSync(manifestPath);
    if (!status.isFile() || status.nlink !== 1) {
      throw new Error(
        "Cargo publication candidate is not an independent regular file",
      );
    }
    json = JSON.parse(fs.readFileSync(manifestPath, "utf8")) as unknown;
  } catch (cause) {
    return {
      kind: "failed",
      packageName: "sprawl-guard-lib",
      detail: `failed to read Cargo publication candidate: ${String(cause)}`,
    };
  }
  const parsed = CARGO_PUBLICATION_MANIFEST_SCHEMA.safeParse(json);
  if (!parsed.success) {
    return {
      kind: "failed",
      packageName: "sprawl-guard-lib",
      detail: `invalid Cargo publication candidate: ${z.prettifyError(parsed.error)}`,
    };
  }
  if (
    parsed.data.releaseCommit !== args.expectation.commit ||
    parsed.data.releaseTag !== args.expectation.tag ||
    parsed.data.releaseVersion !== PACKAGE_VERSION
  ) {
    return {
      kind: "failed",
      packageName: "sprawl-guard-lib",
      detail:
        "Cargo publication candidate does not match the frozen release source",
    };
  }
  const expectedPackageNames = cargoPublicationRequests(args).map(
    ({ packageName }) => packageName,
  );
  if (
    parsed.data.packages.length !== expectedPackageNames.length ||
    parsed.data.packages.some(
      (candidate, index) =>
        candidate.packageName !== expectedPackageNames[index],
    )
  ) {
    return {
      kind: "failed",
      packageName: "sprawl-guard-lib",
      detail: "Cargo publication candidate package order is invalid",
    };
  }
  const checksums = new Map(
    parsed.data.packages.map(({ checksum, packageName }) => [
      packageName,
      checksum,
    ]),
  );
  for (const request of cargoPublicationRequests(args)) {
    const packaged = packageCrate(request);
    if (packaged.kind === "failed") {
      return failedPublication(request, packaged.detail);
    }
    if (packaged.checksum !== checksums.get(request.packageName)) {
      return failedPublication(
        request,
        "Cargo publication archive differs from its frozen candidate manifest",
      );
    }
  }
  return { kind: "ok", checksums };
}

function runGit(
  args: readonly string[],
  workspaceRoot: string,
):
  | { readonly kind: "ok" }
  | {
      readonly kind: "failed";
      readonly command: string;
      readonly detail: string;
    } {
  const result = spawnSync("git", args, {
    cwd: workspaceRoot,
    encoding: "utf8",
    maxBuffer: CARGO_COMMAND_MAXIMUM_OUTPUT_BYTES,
    stdio: ["ignore", "pipe", "pipe"],
    timeout: CARGO_COMMAND_TIMEOUT_MILLISECONDS,
  });
  if (result.status === 0) {
    return { kind: "ok" };
  }
  return {
    kind: "failed",
    command: ["git", ...args].join(" "),
    detail: [result.stdout, result.stderr, result.error?.message]
      .filter(
        (value): value is string => value !== undefined && value.length > 0,
      )
      .join("\n"),
  };
}

function runCargo(
  args: readonly string[],
  workspaceRoot: string,
):
  | { readonly kind: "ok" }
  | {
      readonly kind: "failed";
      readonly detail: string;
      readonly spawnFailed: boolean;
      readonly timedOut: boolean;
    } {
  const result = spawnSync("cargo", args, {
    cwd: workspaceRoot,
    encoding: "utf8",
    maxBuffer: CARGO_COMMAND_MAXIMUM_OUTPUT_BYTES,
    stdio: ["ignore", "pipe", "pipe"],
    timeout: CARGO_COMMAND_TIMEOUT_MILLISECONDS,
  });
  if (result.status === 0) {
    process.stdout.write(result.stdout);
    process.stderr.write(result.stderr);
    return { kind: "ok" };
  }
  return {
    kind: "failed",
    detail: [result.stdout, result.stderr, result.error?.message]
      .filter(
        (value): value is string => value !== undefined && value.length > 0,
      )
      .join("\n"),
    spawnFailed: result.error !== undefined,
    timedOut:
      result.error !== undefined &&
      "code" in result.error &&
      result.error.code === "ETIMEDOUT",
  };
}

function productionOperations(
  workspaceRoot: string,
  checksums: ReadonlyMap<CargoPackageName, string>,
  expectation: ReleaseSourceExpectation,
): CargoPublicationOperations {
  return {
    inspectCargoIndex: (packageName) =>
      inspectCargoIndex(packageName, workspaceRoot),
    inspectRemoteArchive: inspectRemoteCargoArchive,
    packageCrate: (request) => {
      const packagedCrate = runCargo(
        cargoPackageArguments(request.packageName),
        workspaceRoot,
      );
      if (packagedCrate.kind === "failed") {
        return packagedCrate;
      }
      const packaged = packageCrate(request);
      if (packaged.kind === "failed") {
        return packaged;
      }
      if (packaged.checksum !== checksums.get(request.packageName)) {
        return {
          kind: "failed",
          detail: "packaged crate differs from the frozen Cargo candidate",
        };
      }
      return packaged;
    },
    publishCrate: (request) => {
      const source = inspectReleaseSource({ expectation, workspaceRoot });
      return source.kind === "ok"
        ? publishCrate(request, workspaceRoot)
        : {
            kind: "failed",
            detail: `frozen release checkout changed before publication: ${JSON.stringify(source)}`,
          };
    },
    sleepBeforeRetry: () =>
      new Promise<void>((resolve) =>
        setTimeout(resolve, REGISTRY_VISIBILITY_INTERVAL_MILLISECONDS),
      ),
  };
}

async function main(): Promise<number> {
  const mode = process.argv[2];
  if (mode !== "prepare" && mode !== "publish") {
    console.error(
      "usage: publish-cargo-crates.ts <prepare|publish> <full-commit> <v-tag>",
    );
    return 1;
  }
  const parsedSource = parseReleaseSourceExpectation(
    process.argv.slice(3),
    PACKAGE_VERSION,
  );
  if (parsedSource.kind !== "ok") {
    console.error(
      "release source requires a full commit and matching v-prefixed tag",
    );
    return 1;
  }
  const source = inspectReleaseSource({
    expectation: parsedSource.expectation,
    workspaceRoot: WORKSPACE_ROOT,
  });
  if (source.kind !== "ok") {
    console.error(
      `release source validation failed: ${JSON.stringify(source)}`,
    );
    return 1;
  }
  const result =
    mode === "prepare"
      ? prepareCargoCrates({
          expectation: parsedSource.expectation,
          workspaceRoot: WORKSPACE_ROOT,
        })
      : await publishCargoCrates({
          expectation: parsedSource.expectation,
          workspaceRoot: WORKSPACE_ROOT,
        });
  if (result.kind === "ok") {
    return 0;
  }
  console.error(
    `failed to publish ${result.packageName}@${PACKAGE_VERSION}: ${result.detail}`,
  );
  return 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(await main());
}
