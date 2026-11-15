import { gt } from "semver";
import { PACKAGE_VERSION } from "../package-manifests.ts";
import type { NpmDistTag } from "../npm-dist-tag.ts";
import type { PublicPackageVersion } from "../package-version.ts";
import {
  reconcileNpmDistTag,
  reconcileNpmPackageIntegrity,
  type NpmPackageName,
  type NpmPublicationOperations,
  type NpmPublicationRequest,
  type NpmPackageIntegrityDecision,
  type PackagedNpmPublication,
  type PrepareNpmPublicationsResult,
  type PreparedNpmPublication,
  type PublishArchiveResult,
  type PublishNpmPackagesResult,
  type RemoteNpmDistTagInspection,
  type RemotePackageIntegrityInspection,
} from "./model.ts";
import {
  NPM_REGISTRY_URL,
  type AbsoluteDirectoryPath,
  type NpmPackageIntegrity,
} from "./values.ts";

/** How many times a registry read that fails transiently is tried. */
const TRANSIENT_READ_ATTEMPTS = 60;
/**
 * How many times the registry is polled for a version just published. npm can take a few
 * minutes to serve one; at the publisher's two-second interval this allows about ten.
 */
const REGISTRY_VISIBILITY_POLLS = 300;

/**
 * Publishes every absent package version directly on the selected channel, runtime carriers
 * before the wrapper.
 *
 * Each absent package costs exactly one registry write and no dist-tag changes, so the same flow
 * works with npm trusted publishing, which cannot change dist-tags, and with an interactive local
 * login, where npm asks for one browser approval per write. A resumed release skips identical
 * versions that already own the channel and stops with a repair command for any that do not.
 */
export async function publishNpmPublicationRequests(args: {
  readonly npmDistTag: NpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly requests: readonly NpmPublicationRequest[];
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
  readonly requests: readonly NpmPublicationRequest[];
  readonly temporaryDirectory: AbsoluteDirectoryPath;
}): Promise<PrepareNpmPublicationsResult> {
  const packagedPublications: PackagedNpmPublication[] = [];
  for (const request of args.requests) {
    const packed = args.operations.packPackage(request, args.temporaryDirectory);
    if (packed.kind === "failed") {
      return failedPublication(request, packed.detail);
    }
    if (packed.packageName !== request.packageName || packed.version !== PACKAGE_VERSION) {
      return failedPublication(
        request,
        `packed identity ${packed.packageName}@${packed.version} does not match ${request.packageName}@${PACKAGE_VERSION}`,
      );
    }
    packagedPublications.push({ packed, request });
  }

  const preparedPublications: PreparedNpmPublication[] = [];
  for (const publication of packagedPublications) {
    const { packed, request } = publication;
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
    preparedPublications.push({ ...publication, decision });
  }
  return { kind: "ok", publications: preparedPublications };
}

/** Publishes a preflighted set in order, rechecking each package and channel before its write. */
async function publishPreparedNpmPublications(args: {
  readonly npmDistTag: NpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly publications: readonly PreparedNpmPublication[];
}): Promise<PublishNpmPackagesResult> {
  for (const { decision, packed, request } of args.publications) {
    let currentDecision: NpmPackageIntegrityDecision = decision;
    if (currentDecision.kind === "publish") {
      const reinspection = await inspectBeforePublication(
        request.packageName,
        args.operations,
      );
      if (reinspection.kind !== "ok") {
        return failedPublication(request, reinspection.detail);
      }
      currentDecision = reconcileNpmPackageIntegrity({
        localIntegrity: packed.integrity,
        remoteIntegrity: reinspection.integrity,
      });
    }
    if (currentDecision.kind === "integrity_mismatch") {
      return failedPublication(
        request,
        `registry integrity ${currentDecision.remoteIntegrity} differs from staged integrity ${currentDecision.localIntegrity}`,
      );
    }
    if (currentDecision.kind === "skip") {
      const ownership = await requireNpmDistTagOwnership({
        npmDistTag: args.npmDistTag,
        operations: args.operations,
        request,
      });
      if (ownership.kind === "failed") {
        return failedPublication(request, ownership.detail);
      }
      console.log(
        `already published with matching integrity: ${request.packageName}@${PACKAGE_VERSION}`,
      );
      continue;
    }

    const channel = await refuseBackwardNpmDistTag({
      npmDistTag: args.npmDistTag,
      operations: args.operations,
      request,
    });
    if (channel.kind === "failed") {
      return failedPublication(request, channel.detail);
    }

    const published = args.operations.publishArchive(
      packed.archivePath,
      packed.integrity,
      args.npmDistTag,
    );
    if (published.kind === "failed") {
      return failedPublication(request, published.detail);
    }
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
  }
  return { kind: "ok" };
}

/** Reads one immutable package version with bounded transient retries. */
async function inspectBeforePublication(
  packageName: NpmPackageName,
  operations: NpmPublicationOperations,
): Promise<RemotePackageIntegrityInspection> {
  return retryTransientRegistryRead(
    () => operations.inspectRemoteIntegrity(packageName),
    operations.sleepBeforeRetry,
    {
      kind: "permanent_failure",
      detail: `npm inspection for ${packageName}@${PACKAGE_VERSION} exceeded the retry budget`,
    },
  );
}

/** Reads one npm channel with bounded transient retries. */
async function inspectDistTagBeforePublication(
  packageName: NpmPackageName,
  npmDistTag: NpmDistTag,
  operations: NpmPublicationOperations,
): Promise<RemoteNpmDistTagInspection> {
  return retryTransientRegistryRead(
    () => operations.inspectRemoteDistTag(packageName, npmDistTag),
    operations.sleepBeforeRetry,
    {
      kind: "permanent_failure",
      detail: `npm dist-tag inspection for ${packageName}@${npmDistTag} exceeded the retry budget`,
    },
  );
}

/** Waits until npm serves the release through the selected channel. */
async function waitForPublishedDistTag(args: {
  readonly npmDistTag: NpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly packageName: NpmPackageName;
}): Promise<PublishArchiveResult> {
  const visible = await pollRegistry<PublishArchiveResult>(
    () => {
      const inspected = args.operations.inspectRemoteDistTag(
        args.packageName,
        args.npmDistTag,
      );
      if (inspected.kind === "permanent_failure") {
        return { kind: "failed", detail: inspected.detail };
      }
      if (inspected.kind !== "ok" || inspected.remote.kind === "absent") {
        return null;
      }
      if (inspected.remote.version === PACKAGE_VERSION) {
        return { kind: "ok" };
      }
      return gt(inspected.remote.version, PACKAGE_VERSION)
        ? {
            kind: "failed",
            detail: `npm dist-tag ${args.npmDistTag} advanced to ${inspected.remote.version}`,
          }
        : null;
    },
    args.operations.sleepBeforeRetry,
    REGISTRY_VISIBILITY_POLLS,
  );
  return visible ?? {
    kind: "failed",
    detail: `npm did not serve ${args.packageName}@${args.npmDistTag} at ${PACKAGE_VERSION} before the visibility timeout`,
  };
}

async function waitForPublishedPackage(
  packageName: NpmPackageName,
  expectedIntegrity: NpmPackageIntegrity,
  operations: NpmPublicationOperations,
): Promise<PublishArchiveResult> {
  const visible = await pollRegistry<PublishArchiveResult>(
    () => {
      const inspected = operations.inspectRemoteIntegrity(packageName);
      if (inspected.kind === "permanent_failure") {
        return { kind: "failed", detail: inspected.detail };
      }
      if (inspected.kind !== "ok" || inspected.integrity.kind === "absent") {
        return null;
      }
      return inspected.integrity.integrity === expectedIntegrity
        ? { kind: "ok" }
        : {
            kind: "failed",
            detail: `published integrity ${inspected.integrity.integrity} differs from uploaded integrity ${expectedIntegrity}`,
          };
    },
    operations.sleepBeforeRetry,
    REGISTRY_VISIBILITY_POLLS,
  );
  return visible ?? {
    kind: "failed",
    detail: `npm did not serve ${packageName}@${PACKAGE_VERSION} before the visibility timeout`,
  };
}

async function pollRegistry<TResult>(
  inspect: () => TResult | null,
  sleepBeforeRetry: () => Promise<void>,
  attempts: number,
): Promise<TResult | null> {
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    const result = inspect();
    if (result !== null) {
      return result;
    }
    if (attempt + 1 < attempts) {
      await sleepBeforeRetry();
    }
  }
  return null;
}

async function retryTransientRegistryRead<
  TResult extends { readonly kind: string },
>(
  inspect: () => TResult,
  sleepBeforeRetry: () => Promise<void>,
  timeoutResult: TResult,
): Promise<TResult> {
  const result = await pollRegistry(
    () => {
      const inspected = inspect();
      return inspected.kind === "transient_failure" ? null : inspected;
    },
    sleepBeforeRetry,
    TRANSIENT_READ_ATTEMPTS,
  );
  return result ?? timeoutResult;
}

/** Confirms that an already-published identical version still owns the selected channel. */
async function requireNpmDistTagOwnership(args: {
  readonly npmDistTag: NpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly request: NpmPublicationRequest;
}): Promise<PublishArchiveResult> {
  const distTag = await inspectDistTagBeforePublication(
    args.request.packageName,
    args.npmDistTag,
    args.operations,
  );
  if (distTag.kind !== "ok") {
    return { kind: "failed", detail: distTag.detail };
  }
  const decision = reconcileNpmDistTag({
    releaseVersion: PACKAGE_VERSION,
    remote: distTag.remote,
  });
  switch (decision.kind) {
    case "keep":
      return { kind: "ok" };
    case "would_move_backward":
      return backwardDistTagFailure(args.npmDistTag, decision.currentVersion);
    case "update":
      return distTagRepairFailure(args.request.packageName, args.npmDistTag);
  }
}

/** Rereads the selected channel so a publish never moves it to an older version. */
async function refuseBackwardNpmDistTag(args: {
  readonly npmDistTag: NpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly request: NpmPublicationRequest;
}): Promise<PublishArchiveResult> {
  const distTag = await inspectDistTagBeforePublication(
    args.request.packageName,
    args.npmDistTag,
    args.operations,
  );
  if (distTag.kind !== "ok") {
    return { kind: "failed", detail: distTag.detail };
  }
  const decision = reconcileNpmDistTag({
    releaseVersion: PACKAGE_VERSION,
    remote: distTag.remote,
  });
  return decision.kind === "would_move_backward"
    ? backwardDistTagFailure(args.npmDistTag, decision.currentVersion)
    : { kind: "ok" };
}

/** Explains why a publish would move the selected channel to an older version. */
function backwardDistTagFailure(
  npmDistTag: NpmDistTag,
  currentVersion: PublicPackageVersion,
): Extract<PublishArchiveResult, { readonly kind: "failed" }> {
  return {
    kind: "failed",
    detail: `npm dist-tag ${npmDistTag} already targets newer version ${currentVersion}`,
  };
}

/** Explains how to point the channel at a matching version that a resumed release found. */
function distTagRepairFailure(
  packageName: NpmPackageName,
  npmDistTag: NpmDistTag,
): Extract<PublishArchiveResult, { readonly kind: "failed" }> {
  return {
    kind: "failed",
    detail: `matching package version does not own npm dist-tag ${npmDistTag}; repair with npm dist-tag add ${packageName}@${PACKAGE_VERSION} ${npmDistTag} --registry ${NPM_REGISTRY_URL} from an interactive npm login`,
  };
}

function failedPublication(
  request: NpmPublicationRequest,
  detail: string,
): Extract<PublishNpmPackagesResult, { readonly kind: "failed" }> {
  return { kind: "failed", packageName: request.packageName, detail };
}
