import { gt } from "semver";
import { PACKAGE_VERSION } from "../package-manifests.ts";
import type { NpmDistTag, SelectedNpmDistTag } from "../npm-dist-tag.ts";
import {
  reconcileNpmDistTag,
  reconcileNpmPackageIntegrity,
  type NpmPackageName,
  type NpmPublicationOperations,
  type NpmPublicationRequest,
  type NpmPublicationRoute,
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

const REGISTRY_VISIBILITY_ATTEMPTS = 60;

/** Packs and preflights every package before the first registry mutation. */
export async function prepareNpmPublications(args: {
  readonly npmDistTag: SelectedNpmDistTag;
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
        `npm dist-tag ${args.npmDistTag} already targets newer version ${distTagDecision.currentVersion}`,
      );
    }
    preparedPublications.push({ ...publication, decision, distTagDecision });
  }
  return { kind: "ok", publications: preparedPublications };
}

/** Publishes a prepared set through either local staging or trusted publishing. */
export async function publishPreparedNpmPublications(args: {
  readonly operations: NpmPublicationOperations;
  readonly publications: readonly PreparedNpmPublication[];
  readonly route: NpmPublicationRoute;
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
      if (args.route.kind === "trusted_selected") {
        const repairFailure = await inspectTrustedNpmDistTag({
          npmDistTag: args.route.publicationDistTag,
          operations: args.operations,
          request,
        });
        if (repairFailure.kind === "failed") {
          return failedPublication(request, repairFailure.detail);
        }
      }
      console.log(
        `already published with matching integrity: ${request.packageName}@${PACKAGE_VERSION}`,
      );
      continue;
    }

    if (args.route.kind === "trusted_selected") {
      const channelFailure = await inspectTrustedNpmDistTagBeforeMutation({
        npmDistTag: args.route.publicationDistTag,
        operations: args.operations,
        request,
      });
      if (channelFailure.kind === "failed") {
        return failedPublication(request, channelFailure.detail);
      }
    }

    const published = args.operations.publishArchive(
      packed.archivePath,
      packed.integrity,
      args.route.publicationDistTag,
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
    if (args.route.kind === "trusted_selected") {
      const distTagVisible = await waitForPublishedDistTag({
        npmDistTag: args.route.publicationDistTag,
        operations: args.operations,
        packageName: request.packageName,
      });
      if (distTagVisible.kind === "failed") {
        return failedPublication(request, distTagVisible.detail);
      }
    }
    console.log(`published ${request.packageName}@${PACKAGE_VERSION}`);
  }
  return { kind: "ok" };
}

/** Publishes with npm trusted publishing without separate dist-tag mutations. */
export async function publishTrustedNpmPublicationRequests(args: {
  readonly npmDistTag: SelectedNpmDistTag;
  readonly operations: NpmPublicationOperations;
  readonly requests: readonly NpmPublicationRequest[];
  readonly temporaryDirectory: AbsoluteDirectoryPath;
}): Promise<PublishNpmPackagesResult> {
  const prepared = await prepareNpmPublications(args);
  if (prepared.kind === "failed") {
    return prepared;
  }
  for (const publication of prepared.publications) {
    if (
      publication.decision.kind === "skip"
      && publication.distTagDecision.kind !== "keep"
    ) {
      const repairFailure = trustedDistTagRepairFailure(
        publication.request.packageName,
        args.npmDistTag,
      );
      return failedPublication(publication.request, repairFailure.detail);
    }
  }
  return publishPreparedNpmPublications({
    operations: args.operations,
    publications: prepared.publications,
    route: {
      kind: "trusted_selected",
      publicationDistTag: args.npmDistTag,
    },
  });
}

/** Reads one immutable package version with bounded transient retries. */
export async function inspectBeforePublication(
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
export async function inspectDistTagBeforePublication(
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
export async function waitForPublishedDistTag(args: {
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
  );
  return visible ?? {
    kind: "failed",
    detail: `npm did not serve ${packageName}@${PACKAGE_VERSION} before the visibility timeout`,
  };
}

async function pollRegistry<TResult>(
  inspect: () => TResult | null,
  sleepBeforeRetry: () => Promise<void>,
): Promise<TResult | null> {
  for (let attempt = 0; attempt < REGISTRY_VISIBILITY_ATTEMPTS; attempt += 1) {
    const result = inspect();
    if (result !== null) {
      return result;
    }
    if (attempt + 1 < REGISTRY_VISIBILITY_ATTEMPTS) {
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
  );
  return result ?? timeoutResult;
}

async function inspectTrustedNpmDistTag(args: {
  readonly npmDistTag: SelectedNpmDistTag;
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
  return distTag.remote.kind === "present"
    && distTag.remote.version === PACKAGE_VERSION
    ? { kind: "ok" }
    : trustedDistTagRepairFailure(args.request.packageName, args.npmDistTag);
}

async function inspectTrustedNpmDistTagBeforeMutation(args: {
  readonly npmDistTag: SelectedNpmDistTag;
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
    ? {
        kind: "failed",
        detail: `npm dist-tag ${args.npmDistTag} already targets newer version ${decision.currentVersion}`,
      }
    : { kind: "ok" };
}

function trustedDistTagRepairFailure(
  packageName: NpmPackageName,
  npmDistTag: SelectedNpmDistTag,
): Extract<PublishArchiveResult, { readonly kind: "failed" }> {
  return {
    kind: "failed",
    detail: `matching package version does not own npm dist-tag ${npmDistTag}; repair with npm dist-tag add ${packageName}@${PACKAGE_VERSION} ${npmDistTag} --registry ${NPM_REGISTRY_URL} using a short-lived local credential`,
  };
}

function failedPublication(
  request: NpmPublicationRequest,
  detail: string,
): Extract<PublishNpmPackagesResult, { readonly kind: "failed" }> {
  return { kind: "failed", packageName: request.packageName, detail };
}
