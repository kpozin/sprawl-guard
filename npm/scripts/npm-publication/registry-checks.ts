import { gt } from "semver";
import { PACKAGE_VERSION } from "../package-manifests.ts";
import type { NpmDistTag } from "../npm-dist-tag.ts";
import type { PublicPackageVersion } from "../package-version.ts";
import {
  reconcileNpmDistTag,
  type NpmPackageName,
  type NpmPublicationOperations,
  type NpmPublicationRequest,
  type PublishArchiveResult,
  type RemoteNpmDistTagInspection,
  type RemotePackageIntegrityInspection,
} from "./model.ts";
import { NPM_REGISTRY_URL, type NpmPackageIntegrity } from "./values.ts";

/** How many times a registry read that fails transiently is tried. */
const TRANSIENT_READ_ATTEMPTS = 60;
/**
 * How many times the registry is polled for a version just published. npm can take a few
 * minutes to serve one; at the publisher's two-second interval this allows about ten.
 */
const REGISTRY_VISIBILITY_POLLS = 300;

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
    REGISTRY_VISIBILITY_POLLS,
  );
  return visible ?? {
    kind: "failed",
    detail: `npm did not serve ${args.packageName}@${args.npmDistTag} at ${PACKAGE_VERSION} before the visibility timeout`,
  };
}

/** Waits until npm serves the uploaded package version, failing if its contents differ. */
export async function waitForPublishedPackage(
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
export async function requireNpmDistTagOwnership(args: {
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
export async function refuseBackwardNpmDistTag(args: {
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
export function backwardDistTagFailure(
  npmDistTag: NpmDistTag,
  currentVersion: PublicPackageVersion,
): Extract<PublishArchiveResult, { readonly kind: "failed" }> {
  return {
    kind: "failed",
    detail: `npm dist-tag ${npmDistTag} already targets newer version ${currentVersion}`,
  };
}

/** Explains how to point the channel at a matching version that a resumed release found. */
export function distTagRepairFailure(
  packageName: NpmPackageName,
  npmDistTag: NpmDistTag,
): Extract<PublishArchiveResult, { readonly kind: "failed" }> {
  return {
    kind: "failed",
    detail: `matching package version does not own npm dist-tag ${npmDistTag}; repair with npm dist-tag add ${packageName}@${PACKAGE_VERSION} ${npmDistTag} --registry ${NPM_REGISTRY_URL} from an interactive npm login`,
  };
}
