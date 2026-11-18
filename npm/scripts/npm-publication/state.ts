import { PACKAGE_VERSION } from "../package-manifests.ts";
import type { NpmDistTag } from "../npm-dist-tag.ts";
import {
  reconcileNpmDistTag,
  reconcileNpmPackageIntegrity,
  type NpmPublicationOperations,
  type NpmPublicationRequest,
  type NpmReleasePackages,
  type PackagedNpmPublication,
  type PrepareNpmPublicationsResult,
  type PreparedNpmPublication,
  type PublishNpmPackagesResult,
} from "./model.ts";
import {
  backwardDistTagFailure,
  distTagRepairFailure,
  inspectBeforePublication,
  inspectDistTagBeforePublication,
  refuseBackwardNpmDistTag,
  requireNpmDistTagOwnership,
  waitForPublishedDistTag,
  waitForPublishedPackage,
} from "./registry-checks.ts";
import type { AbsoluteDirectoryPath } from "./values.ts";

type FailedPublication = Extract<PublishNpmPackagesResult, { readonly kind: "failed" }>;

/** Result of one step for one package, which names the package when it fails. */
type PublicationStepResult<TValue> =
  | { readonly kind: "ok"; readonly value: TValue }
  | FailedPublication;

/** What the registry write for one prepared package did, before npm serves the result. */
type SubmittedPublication =
  | { readonly kind: "skipped" }
  | { readonly kind: "uploaded"; readonly publication: PreparedNpmPublication }
  | FailedPublication;

/**
 * Publishes every absent package version directly on the selected channel: the platform packages
 * first, then the wrapper once npm serves all of them.
 *
 * Each absent package costs exactly one registry write and no dist-tag changes, so the same flow
 * works with npm trusted publishing, which cannot change dist-tags, and with an interactive local
 * login, where npm asks for one browser approval per write. A resumed release skips identical
 * versions that already own the channel and stops with a repair command for any that do not.
 */
export async function publishNpmPublicationRequests(args: {
  readonly npmDistTag: NpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly requests: NpmReleasePackages<NpmPublicationRequest>;
  readonly temporaryDirectory: AbsoluteDirectoryPath;
}): Promise<PublishNpmPackagesResult> {
  const prepared = await prepareNpmPublications(args);
  if (prepared.kind === "failed") {
    return prepared;
  }
  return publishPreparedNpmPublications({
    npmDistTag: args.npmDistTag,
    operations: args.operations,
    publications: prepared.publications,
  });
}

/** Packs and preflights every package before the first registry mutation. */
async function prepareNpmPublications(args: {
  readonly npmDistTag: NpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly requests: NpmReleasePackages<NpmPublicationRequest>;
  readonly temporaryDirectory: AbsoluteDirectoryPath;
}): Promise<PrepareNpmPublicationsResult> {
  const packaged = await mapReleasePackages(args.requests, (request) =>
    packPublication({
      packPackage: args.operations.packPackage,
      request,
      temporaryDirectory: args.temporaryDirectory,
    }),
  );
  if (packaged.kind === "failed") {
    return packaged;
  }
  const prepared = await mapReleasePackages(packaged.value, (publication) =>
    preflightPublication({ ...args, publication }),
  );
  return prepared.kind === "failed"
    ? prepared
    : { kind: "ok", publications: prepared.value };
}

/** Runs one step on each platform package and then the wrapper, stopping at the first failure. */
async function mapReleasePackages<TInput, TOutput>(
  packages: NpmReleasePackages<TInput>,
  step: (item: TInput) => PublicationStepResult<TOutput> | Promise<PublicationStepResult<TOutput>>,
): Promise<PublicationStepResult<NpmReleasePackages<TOutput>>> {
  const platformPackages: TOutput[] = [];
  for (const item of packages.platformPackages) {
    const result = await step(item);
    if (result.kind === "failed") {
      return result;
    }
    platformPackages.push(result.value);
  }
  const wrapper = await step(packages.wrapper);
  return wrapper.kind === "failed"
    ? wrapper
    : { kind: "ok", value: { platformPackages, wrapper: wrapper.value } };
}

/** Packs one package and checks that the archive has the requested name and release version. */
export function packPublication(args: {
  readonly packPackage: NpmPublicationOperations["packPackage"];
  readonly request: NpmPublicationRequest;
  readonly temporaryDirectory: AbsoluteDirectoryPath;
}): PublicationStepResult<PackagedNpmPublication> {
  const { request } = args;
  const packed = args.packPackage(request, args.temporaryDirectory);
  if (packed.kind === "failed") {
    return failedPublication(request, packed.detail);
  }
  if (packed.packageName !== request.packageName || packed.version !== PACKAGE_VERSION) {
    return failedPublication(
      request,
      `packed identity ${packed.packageName}@${packed.version} does not match ${request.packageName}@${PACKAGE_VERSION}`,
    );
  }
  return { kind: "ok", value: { packed, request } };
}

/** Decides whether one packed package is published or skipped, refusing registry conflicts. */
async function preflightPublication(args: {
  readonly npmDistTag: NpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly publication: PackagedNpmPublication;
}): Promise<PublicationStepResult<PreparedNpmPublication>> {
  const { packed, request } = args.publication;
  const inspected = await inspectBeforePublication(request.packageName, args.operations);
  if (inspected.kind !== "ok") {
    return failedPublication(request, inspected.detail);
  }
  const decision = reconcileNpmPackageIntegrity({
    localIntegrity: packed.integrity,
    remoteIntegrity: inspected.integrity,
  });
  if (decision.kind === "integrity_mismatch") {
    return failedPublication(
      request,
      `registry integrity ${decision.remoteIntegrity} differs from staged integrity ${decision.localIntegrity}`,
    );
  }
  const inspectedDistTag = await inspectDistTagBeforePublication(
    request.packageName,
    args.npmDistTag,
    args.operations,
  );
  if (inspectedDistTag.kind !== "ok") {
    return failedPublication(request, inspectedDistTag.detail);
  }
  const distTagDecision = reconcileNpmDistTag({
    releaseVersion: PACKAGE_VERSION,
    remote: inspectedDistTag.remote,
  });
  if (distTagDecision.kind === "would_move_backward") {
    return failedPublication(
      request,
      backwardDistTagFailure(args.npmDistTag, distTagDecision.currentVersion).detail,
    );
  }
  if (decision.kind === "skip" && distTagDecision.kind !== "keep") {
    return failedPublication(
      request,
      distTagRepairFailure(request.packageName, args.npmDistTag).detail,
    );
  }
  return { kind: "ok", value: { ...args.publication, decision } };
}

/**
 * Uploads the platform packages one at a time, then waits until npm serves all of them, and only
 * then publishes the wrapper, so npm never serves a wrapper whose optional dependencies are
 * missing. Waiting once for every platform package, rather than after each upload, is what keeps
 * the release short: npm takes minutes to serve a new version but seconds to accept it.
 */
async function publishPreparedNpmPublications(args: {
  readonly npmDistTag: NpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly publications: NpmReleasePackages<PreparedNpmPublication>;
}): Promise<PublishNpmPackagesResult> {
  const submit = (publication: PreparedNpmPublication) =>
    submitPreparedPublication({ ...args, publication });
  const uploaded: PreparedNpmPublication[] = [];
  for (const publication of args.publications.platformPackages) {
    const submitted = await submit(publication);
    if (submitted.kind === "failed") {
      return submitted;
    }
    if (submitted.kind === "uploaded") {
      uploaded.push(submitted.publication);
    }
  }
  for (const publication of uploaded) {
    const served = await waitUntilServed({ ...args, publication });
    if (served.kind === "failed") {
      return served;
    }
  }
  const wrapper = await submit(args.publications.wrapper);
  switch (wrapper.kind) {
    case "failed":
      return wrapper;
    case "skipped":
      return { kind: "ok" };
    case "uploaded":
      return waitUntilServed({ ...args, publication: wrapper.publication });
  }
}

/**
 * Rechecks one package and its channel against the registry, then uploads it unless an
 * identical version already owns the channel.
 */
async function submitPreparedPublication(args: {
  readonly npmDistTag: NpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly publication: PreparedNpmPublication;
}): Promise<SubmittedPublication> {
  const { decision, packed, request } = args.publication;
  const reinspection = decision.kind === "publish"
    ? await inspectBeforePublication(request.packageName, args.operations)
    : null;
  if (reinspection !== null && reinspection.kind !== "ok") {
    return failedPublication(request, reinspection.detail);
  }
  const currentDecision = reinspection === null
    ? decision
    : reconcileNpmPackageIntegrity({
        localIntegrity: packed.integrity,
        remoteIntegrity: reinspection.integrity,
      });
  switch (currentDecision.kind) {
    case "integrity_mismatch":
      return failedPublication(
        request,
        `registry integrity ${currentDecision.remoteIntegrity} differs from staged integrity ${currentDecision.localIntegrity}`,
      );
    case "skip": {
      const ownership = await requireNpmDistTagOwnership({ ...args, request });
      if (ownership.kind === "failed") {
        return failedPublication(request, ownership.detail);
      }
      console.log(
        `already published with matching integrity: ${request.packageName}@${PACKAGE_VERSION}`,
      );
      return { kind: "skipped" };
    }
    case "publish": {
      const channel = await refuseBackwardNpmDistTag({ ...args, request });
      if (channel.kind === "failed") {
        return failedPublication(request, channel.detail);
      }
      const published = args.operations.publishArchive(
        packed.archivePath,
        packed.integrity,
        args.npmDistTag,
      );
      return published.kind === "failed"
        ? failedPublication(request, published.detail)
        : { kind: "uploaded", publication: args.publication };
    }
  }
}

/** Waits until npm serves an uploaded package version and points the channel at it. */
async function waitUntilServed(args: {
  readonly npmDistTag: NpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly publication: PreparedNpmPublication;
}): Promise<PublishNpmPackagesResult> {
  const { packed, request } = args.publication;
  const visible = await waitForPublishedPackage(
    request.packageName,
    packed.integrity,
    args.operations,
  );
  if (visible.kind === "failed") {
    return failedPublication(request, visible.detail);
  }
  const distTagVisible = await waitForPublishedDistTag({
    npmDistTag: args.npmDistTag,
    operations: args.operations,
    packageName: request.packageName,
  });
  if (distTagVisible.kind === "failed") {
    return failedPublication(request, distTagVisible.detail);
  }
  console.log(`published ${request.packageName}@${PACKAGE_VERSION}`);
  return { kind: "ok" };
}

function failedPublication(
  request: NpmPublicationRequest,
  detail: string,
): FailedPublication {
  return { kind: "failed", packageName: request.packageName, detail };
}
