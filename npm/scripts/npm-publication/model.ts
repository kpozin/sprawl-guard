import { gt } from "semver";
import type {
  NpmDistTag,
  SelectedNpmDistTag,
  StagingNpmDistTag,
} from "../npm-dist-tag.ts";
import type { PublicPackageVersion } from "../package-version.ts";
import type {
  AbsoluteDirectoryPath,
  NpmArchivePath,
  NpmPackageIntegrity,
  NpmPackageName,
  PackageRelativePath,
} from "./values.ts";

export type { NpmPackageName } from "./values.ts";

/** Expected contents of one staged npm package archive. */
export type ArchiveExpectation =
  | {
      readonly kind: "files";
      readonly requiredPaths: readonly PackageRelativePath[];
    }
  | {
      readonly kind: "posix_executable";
      readonly executablePath: PackageRelativePath;
      readonly requiredPaths: readonly PackageRelativePath[];
    };

/** One staged npm package that must be reconciled with the registry. */
export interface NpmPublicationRequest {
  readonly archiveExpectation: ArchiveExpectation;
  readonly expectedManifestText: string;
  readonly expectedPackedManifestText: string;
  readonly expectedPackedPaths: readonly PackageRelativePath[];
  readonly packageDirectory: AbsoluteDirectoryPath;
  readonly packageName: NpmPackageName;
}

/** Current immutable version state returned by npm. */
export type RemotePackageIntegrity =
  | { readonly kind: "absent" }
  | { readonly kind: "present"; readonly integrity: NpmPackageIntegrity };

/** Current registry target for one npm distribution tag. */
export type RemoteNpmDistTag =
  | { readonly kind: "absent" }
  | { readonly kind: "present"; readonly version: PublicPackageVersion };

/** Result of reading one npm channel from the registry. */
export type RemoteNpmDistTagInspection =
  | { readonly kind: "ok"; readonly remote: RemoteNpmDistTag }
  | { readonly kind: "transient_failure"; readonly detail: string }
  | { readonly kind: "permanent_failure"; readonly detail: string };

/** Decision for reconciling an npm channel with the release version. */
export type NpmDistTagDecision =
  | { readonly kind: "keep" }
  | { readonly kind: "update" }
  | {
      readonly kind: "would_move_backward";
      readonly currentVersion: PublicPackageVersion;
    };

/** Result of inspecting one immutable package version on npm. */
export type RemotePackageIntegrityInspection =
  | { readonly kind: "ok"; readonly integrity: RemotePackageIntegrity }
  | { readonly kind: "transient_failure"; readonly detail: string }
  | { readonly kind: "permanent_failure"; readonly detail: string };

/** Decision for reconciling one immutable package version. */
export type NpmPackageIntegrityDecision =
  | { readonly kind: "publish" }
  | { readonly kind: "skip" }
  | {
      readonly kind: "integrity_mismatch";
      readonly localIntegrity: NpmPackageIntegrity;
      readonly remoteIntegrity: NpmPackageIntegrity;
    };

/** Result of packing one npm package. */
export type PackResult =
  | {
      readonly kind: "ok";
      readonly archivePath: NpmArchivePath;
      readonly integrity: NpmPackageIntegrity;
      readonly packageName: NpmPackageName;
      readonly version: PublicPackageVersion;
    }
  | { readonly kind: "failed"; readonly detail: string };

/** Successful package archive and identity. */
export type SuccessfulPackResult = Extract<PackResult, { readonly kind: "ok" }>;

/** Result of one npm registry mutation. */
export type PublishArchiveResult =
  | { readonly kind: "ok" }
  | { readonly kind: "failed"; readonly detail: string };

/** Result of validating one untrusted external value. */
export type ValidationResult =
  | { readonly kind: "valid" }
  | { readonly kind: "invalid"; readonly detail: string };

/** Result of publishing the complete npm release set. */
export type PublishNpmPackagesResult =
  | { readonly kind: "ok" }
  | {
      readonly kind: "failed";
      readonly packageName: NpmPackageName;
      readonly detail: string;
    };

/** Registry and package-manager operations used by the npm publication state machine. */
export interface NpmPublicationOperations {
  readonly inspectRemoteDistTag: (
    packageName: NpmPackageName,
    npmDistTag: NpmDistTag,
  ) => RemoteNpmDistTagInspection;
  readonly inspectRemoteIntegrity: (
    packageName: NpmPackageName,
  ) => RemotePackageIntegrityInspection;
  readonly packPackage: (
    request: NpmPublicationRequest,
    temporaryDirectory: AbsoluteDirectoryPath,
  ) => PackResult;
  readonly publishArchive: (
    archivePath: NpmArchivePath,
    expectedIntegrity: NpmPackageIntegrity,
    npmDistTag: NpmDistTag,
  ) => PublishArchiveResult;
  readonly removeRemoteDistTag: (
    packageName: NpmPackageName,
    npmDistTag: NpmDistTag,
  ) => PublishArchiveResult;
  readonly sleepBeforeRetry: () => Promise<void>;
  readonly updateRemoteDistTag: (
    packageName: NpmPackageName,
    npmDistTag: NpmDistTag,
  ) => PublishArchiveResult;
}

/** Packed package and its release request. */
export interface PackagedNpmPublication {
  readonly packed: SuccessfulPackResult;
  readonly request: NpmPublicationRequest;
}

/** Package whose immutable version and selected channel passed preflight. */
export interface PreparedNpmPublication extends PackagedNpmPublication {
  readonly decision: Exclude<
    NpmPackageIntegrityDecision,
    { readonly kind: "integrity_mismatch" }
  >;
  readonly distTagDecision: NpmDistTagDecision;
}

/** Result of packing and preflighting the complete npm release set. */
export type PrepareNpmPublicationsResult =
  | {
      readonly kind: "ok";
      readonly publications: readonly PreparedNpmPublication[];
    }
  | Extract<PublishNpmPackagesResult, { readonly kind: "failed" }>;

/** Registry channel used for the initial publish mutation. */
export type NpmPublicationRoute =
  | {
      readonly kind: "local_staging";
      readonly publicationDistTag: StagingNpmDistTag;
    }
  | {
      readonly kind: "trusted_selected";
      readonly publicationDistTag: SelectedNpmDistTag;
    };

/** Decides whether an immutable npm version is new, identical, or conflicting. */
export function reconcileNpmPackageIntegrity(args: {
  readonly localIntegrity: NpmPackageIntegrity;
  readonly remoteIntegrity: RemotePackageIntegrity;
}): NpmPackageIntegrityDecision {
  if (args.remoteIntegrity.kind === "absent") {
    return { kind: "publish" };
  }
  if (args.remoteIntegrity.integrity === args.localIntegrity) {
    return { kind: "skip" };
  }
  return {
    kind: "integrity_mismatch",
    localIntegrity: args.localIntegrity,
    remoteIntegrity: args.remoteIntegrity.integrity,
  };
}

/** Decides whether a channel is current, may advance, or would move backward. */
export function reconcileNpmDistTag(args: {
  readonly releaseVersion: PublicPackageVersion;
  readonly remote: RemoteNpmDistTag;
}): NpmDistTagDecision {
  if (args.remote.kind === "absent") {
    return { kind: "update" };
  }
  if (args.remote.version === args.releaseVersion) {
    return { kind: "keep" };
  }
  return gt(args.remote.version, args.releaseVersion)
    ? {
        kind: "would_move_backward",
        currentVersion: args.remote.version,
      }
    : { kind: "update" };
}
