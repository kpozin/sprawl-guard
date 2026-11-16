#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { inspectFilesystemEntry } from "./filesystem-entry.ts";
import {
  PUBLISH_WORKSPACE_DIRECTORY,
  PUBLISH_LICENSE_FILE_NAMES,
  PUBLISH_WRAPPER_BUILD_FILES,
  PUBLISH_WRAPPER_DOCUMENTS,
  RUNTIME_PACKAGE_DEFINITIONS,
  publishWorkspaceTextFiles,
} from "./package-manifests.ts";
import {
  absoluteDirectoryPath,
  artifactRootDirectory as artifactRootDirectoryPath,
  type AbsoluteDirectoryPath,
  type ArtifactRootDirectory,
  type PackageRelativePath,
  payloadFileRelativePath,
  type PayloadValidationDetail,
  payloadValidationDetail,
  payloadValidationDetails,
  publishWorkspaceRootDirectory,
  type PublishWorkspaceRootDirectory,
  renderedWorkspaceRelativePath,
  type RenderedWorkspaceRelativePath,
  sourcePayloadRelativePath,
  type SourcePayloadRelativePath,
} from "./path-values.ts";
import {
  validatePackagePayloads,
  type PayloadValidationResult,
} from "./payload-validation.ts";
import { resolveAbsolutePath, type AbsolutePath } from "../sprawl-guard/src/lib/paths.ts";

type StagePublishPayloadsResult =
  | {
      readonly kind: "ok";
      readonly stagedPaths: readonly RenderedWorkspaceRelativePath[];
    }
  | {
      readonly kind: "invalid_arguments";
      readonly args: readonly string[];
    }
  | {
      readonly kind: "missing_source_entry";
      readonly relativePath: SourcePayloadRelativePath;
    }
  | {
      readonly kind: "missing_rendered_entry";
      readonly relativePath: RenderedWorkspaceRelativePath;
    }
  | {
      readonly kind: "invalid_payload";
      readonly relativePath: SourcePayloadRelativePath | RenderedWorkspaceRelativePath;
      readonly detail: PayloadValidationDetail;
    }
  | {
      readonly kind: "inspection_failed";
      readonly relativePath: SourcePayloadRelativePath | RenderedWorkspaceRelativePath;
      readonly cause: unknown;
    }
  | {
      readonly kind: "copy_failed";
      readonly relativePath: RenderedWorkspaceRelativePath;
      readonly cause: unknown;
    };

type StagePublishPayloadsFailure = Exclude<
  StagePublishPayloadsResult,
  { readonly kind: "ok" }
>;
type PayloadStageFailure = Extract<
  StagePublishPayloadsFailure,
  { readonly kind: "invalid_payload" | "inspection_failed" }
>;
type StagedPayloadRelativePath =
  | SourcePayloadRelativePath
  | RenderedWorkspaceRelativePath;
type PayloadPathRole = "source" | "destination";
type PayloadPathValidationResult =
  | {
      readonly kind: "valid";
    }
  | {
      readonly kind: "invalid";
      readonly detail: PayloadValidationDetail;
    }
  | {
      readonly kind: "filesystem_error";
      readonly cause: unknown;
    };
type ParseArtifactRootDirectoryResult =
  | {
      readonly kind: "ok";
      readonly artifactRootDirectory: ArtifactRootDirectory;
    }
  | {
      readonly kind: "invalid_arguments";
      readonly args: readonly string[];
    };

const WORKSPACE_ROOT_DIRECTORY = publishWorkspaceRootDirectory(
  fileURLToPath(new URL("..", import.meta.url)),
);

/**
 * Stages release payload files into an exclusively owned, freshly rendered publish workspace.
 *
 * The caller must discard and render the workspace again after any failure.
 */
export function stagePublishPayloads(args: {
  readonly workspaceRootDirectory: PublishWorkspaceRootDirectory;
  readonly artifactRootDirectory: ArtifactRootDirectory;
}): StagePublishPayloadsResult {
  const stagedPaths: RenderedWorkspaceRelativePath[] = [];
  const wrapperDirectory = `${PUBLISH_WORKSPACE_DIRECTORY}/sprawl-guard`;

  const renderedWorkspaceEntries = [
    ...publishWorkspaceTextFiles().map((textFile) =>
      renderedWorkspaceRelativePath(textFile.relativePath),
    ),
    ...[
      "README.md",
      ...PUBLISH_WRAPPER_BUILD_FILES,
      ...PUBLISH_WRAPPER_DOCUMENTS.map((document) => document.packagePath),
      ...PUBLISH_LICENSE_FILE_NAMES,
    ].map((relativePath) =>
      renderedWorkspaceRelativePath(`${wrapperDirectory}/${relativePath}`),
    ),
    ...RUNTIME_PACKAGE_DEFINITIONS.flatMap((definition) =>
      PUBLISH_LICENSE_FILE_NAMES.map((fileName) =>
        renderedWorkspaceRelativePath(
          `${PUBLISH_WORKSPACE_DIRECTORY}/packages/${definition.slug}/${fileName}`,
        ),
      ),
    ),
  ];

  for (const relativePath of renderedWorkspaceEntries) {
    const pathValidationFailure = payloadPathValidationFailure(
      relativePath,
      validateDestinationPath({
        destinationRelativePath: relativePath,
        workspaceRootDirectory: args.workspaceRootDirectory,
      }),
    );
    if (pathValidationFailure !== undefined) {
      return pathValidationFailure;
    }

    const inspection = inspectFilesystemEntry(
      resolveAbsolutePath(path.join(args.workspaceRootDirectory, relativePath)),
    );
    if (inspection.kind === "missing") {
      return {
        kind: "missing_rendered_entry",
        relativePath,
      };
    }
    if (inspection.kind === "filesystem_error") {
      return {
        kind: "inspection_failed",
        relativePath,
        cause: inspection.cause,
      };
    }
  }

  for (const definition of RUNTIME_PACKAGE_DEFINITIONS) {
    const sourceRelativePath = sourcePayloadRelativePath(
      `${definition.slug}/${definition.artifactRelativePath}`,
    );
    const sourcePath = resolveAbsolutePath(
      path.join(args.artifactRootDirectory, sourceRelativePath),
    );
    const sourceValidation = validateSourcePath({
      artifactRootDirectory: args.artifactRootDirectory,
      sourceRelativePath,
    });
    const sourceValidationFailure = payloadPathValidationFailure(
      sourceRelativePath,
      sourceValidation,
    );
    if (sourceValidationFailure !== undefined) {
      return sourceValidationFailure;
    }

    const sourceInspection = inspectFilesystemEntry(sourcePath);
    if (sourceInspection.kind === "missing") {
      return {
        kind: "missing_source_entry",
        relativePath: sourceRelativePath,
      };
    }
    if (sourceInspection.kind === "filesystem_error") {
      return {
        kind: "inspection_failed",
        relativePath: sourceRelativePath,
        cause: sourceInspection.cause,
      };
    }

    const sourcePayloadValidation = validatePackagePayloads({
      packageDirectory: absoluteDirectoryPath(
        path.join(args.artifactRootDirectory, definition.slug),
      ),
      payloadKind: definition.payloadCheckKind,
      expectedPaths: [payloadFileRelativePath(definition.artifactRelativePath)],
    });
    const sourcePayloadFailure = packagePayloadValidationFailure(
      sourceRelativePath,
      sourcePayloadValidation,
    );
    if (sourcePayloadFailure !== undefined) {
      return sourcePayloadFailure;
    }
  }

  for (const definition of RUNTIME_PACKAGE_DEFINITIONS) {
    const destinationRelativePath = renderedWorkspaceRelativePath(
      `${PUBLISH_WORKSPACE_DIRECTORY}/packages/${definition.slug}/${definition.payloadPath}`,
    );
    const destinationValidation = validateDestinationPath({
      destinationRelativePath,
      workspaceRootDirectory: args.workspaceRootDirectory,
    });
    const destinationValidationFailure = payloadPathValidationFailure(
      destinationRelativePath,
      destinationValidation,
    );
    if (destinationValidationFailure !== undefined) {
      return destinationValidationFailure;
    }
  }

  for (const definition of RUNTIME_PACKAGE_DEFINITIONS) {
    const sourceRelativePath = sourcePayloadRelativePath(
      `${definition.slug}/${definition.artifactRelativePath}`,
    );
    const sourcePath = path.join(args.artifactRootDirectory, sourceRelativePath);

    const destinationRelativePath = renderedWorkspaceRelativePath(
      `${PUBLISH_WORKSPACE_DIRECTORY}/packages/${definition.slug}/${definition.payloadPath}`,
    );
    const destinationPath = path.join(
      args.workspaceRootDirectory,
      destinationRelativePath,
    );

    try {
      fs.mkdirSync(path.dirname(destinationPath), { recursive: true });
      fs.copyFileSync(sourcePath, destinationPath);
    } catch (cause) {
      return {
        kind: "copy_failed",
        relativePath: destinationRelativePath,
        cause,
      };
    }

    const stagedPackageDirectory = path.join(
      args.workspaceRootDirectory,
      PUBLISH_WORKSPACE_DIRECTORY,
      "packages",
      definition.slug,
    );
    const payloadValidation = validatePackagePayloads({
      packageDirectory: absoluteDirectoryPath(stagedPackageDirectory),
      payloadKind: definition.payloadCheckKind,
      expectedPaths: [payloadFileRelativePath(definition.payloadPath)],
    });

    const payloadFailure = packagePayloadValidationFailure(
      destinationRelativePath,
      payloadValidation,
    );
    if (payloadFailure !== undefined) {
      return payloadFailure;
    }

    stagedPaths.push(destinationRelativePath);
  }

  return {
    kind: "ok",
    stagedPaths,
  };
}

function payloadPathValidationFailure(
  relativePath: StagedPayloadRelativePath,
  validation: PayloadPathValidationResult,
): PayloadStageFailure | undefined {
  switch (validation.kind) {
    case "valid":
      return undefined;

    case "invalid":
      return {
        kind: "invalid_payload",
        relativePath,
        detail: validation.detail,
      };

    case "filesystem_error":
      return {
        kind: "inspection_failed",
        relativePath,
        cause: validation.cause,
      };
  }
}

function packagePayloadValidationFailure(
  relativePath: StagedPayloadRelativePath,
  validation: PayloadValidationResult,
): PayloadStageFailure | undefined {
  if (validation.kind === "filesystem_error") {
    return {
      kind: "inspection_failed",
      relativePath,
      cause: validation.cause,
    };
  }

  if (validation.missingPaths.length > 0) {
    return {
      kind: "invalid_payload",
      relativePath,
      detail: payloadValidationDetails(validation.missingPaths),
    };
  }

  if (validation.invalidPaths.length > 0) {
    return {
      kind: "invalid_payload",
      relativePath,
      detail: payloadValidationDetails(validation.invalidPaths),
    };
  }

  return undefined;
}

function validateSourcePath(args: {
  readonly artifactRootDirectory: ArtifactRootDirectory;
  readonly sourceRelativePath: SourcePayloadRelativePath;
}): PayloadPathValidationResult {
  return validatePayloadPath({
    relativePath: args.sourceRelativePath,
    role: "source",
    rootDirectory: args.artifactRootDirectory,
  });
}

function validateDestinationPath(args: {
  readonly workspaceRootDirectory: PublishWorkspaceRootDirectory;
  readonly destinationRelativePath: RenderedWorkspaceRelativePath;
}): PayloadPathValidationResult {
  return validatePayloadPath({
    relativePath: args.destinationRelativePath,
    role: "destination",
    rootDirectory: args.workspaceRootDirectory,
  });
}

function validatePayloadPath(args: {
  readonly rootDirectory: AbsoluteDirectoryPath;
  readonly relativePath: PackageRelativePath;
  readonly role: PayloadPathRole;
}): PayloadPathValidationResult {
  const payloadPath = resolveAbsolutePath(path.join(args.rootDirectory, args.relativePath));
  const relativeParentPath = path.dirname(args.relativePath);
  const normalizedParentPath = path.normalize(relativeParentPath);
  const parentSegments =
    normalizedParentPath === "." ? [] : normalizedParentPath.split(path.sep);
  let currentPath: AbsolutePath = args.rootDirectory;

  const rootValidation = validateExistingPayloadPathEntry({
    entryPath: currentPath,
    expectedKind: "directory",
    role: args.role,
  });
  if (rootValidation.kind !== "valid") {
    return rootValidation;
  }

  for (const parentSegment of parentSegments) {
    currentPath = resolveAbsolutePath(path.join(currentPath, parentSegment));
    const parentValidation = validateExistingPayloadPathEntry({
      entryPath: currentPath,
      expectedKind: "directory",
      role: args.role,
    });
    if (parentValidation.kind !== "valid") {
      return parentValidation;
    }
  }

  return validateExistingPayloadPathEntry({
    entryPath: payloadPath,
    expectedKind: "file",
    role: args.role,
  });
}

function validateExistingPayloadPathEntry(args: {
  readonly entryPath: AbsolutePath;
  readonly expectedKind: "directory" | "file";
  readonly role: PayloadPathRole;
}): PayloadPathValidationResult {
  const inspection = inspectFilesystemEntry(args.entryPath);
  if (inspection.kind === "missing") {
    return { kind: "valid" };
  }
  if (inspection.kind === "filesystem_error") {
    return {
      kind: "filesystem_error",
      cause: inspection.cause,
    };
  }
  const stat = inspection.stats;

  if (stat.isSymbolicLink()) {
    return {
      kind: "invalid",
      detail: payloadValidationDetail(
        `${args.entryPath} (${args.role} symbolic links are not allowed)`,
      ),
    };
  }

  if (args.expectedKind === "directory" && !stat.isDirectory()) {
    return {
      kind: "invalid",
      detail: payloadValidationDetail(
        `${args.entryPath} (expected an existing directory)`,
      ),
    };
  }

  if (args.expectedKind === "file" && !stat.isFile()) {
    return {
      kind: "invalid",
      detail: payloadValidationDetail(
        `${args.entryPath} (expected an existing regular file)`,
      ),
    };
  }

  if (args.role === "destination" && args.expectedKind === "file" && stat.nlink > 1) {
    return {
      kind: "invalid",
      detail: payloadValidationDetail(
        `${args.entryPath} (destination hard links are not allowed)`,
      ),
    };
  }

  return { kind: "valid" };
}

function parseArtifactRootDirectory(args: readonly string[]): ParseArtifactRootDirectoryResult {
  const [artifactRootDirectory] = args;
  if (args.length === 1 && artifactRootDirectory !== undefined) {
    return {
      kind: "ok",
      artifactRootDirectory: artifactRootDirectoryPath(artifactRootDirectory),
    };
  }

  return {
    kind: "invalid_arguments",
    args: [...args],
  };
}

function main(): number {
  const parsedArtifactRootDirectory = parseArtifactRootDirectory(process.argv.slice(2));
  if (parsedArtifactRootDirectory.kind === "invalid_arguments") {
    printFailure(parsedArtifactRootDirectory);
    return 1;
  }

  const result = stagePublishPayloads({
    workspaceRootDirectory: WORKSPACE_ROOT_DIRECTORY,
    artifactRootDirectory: parsedArtifactRootDirectory.artifactRootDirectory,
  });

  if (result.kind === "ok") {
    return 0;
  }

  printFailure(result);
  return 1;
}

function printFailure(failure: StagePublishPayloadsFailure): void {
  switch (failure.kind) {
    case "invalid_arguments":
      console.error(
        [
          "usage: stage-publish-payloads.ts <artifact-root-directory>",
          ...failure.args.map((arg) => `unexpected argument: ${arg}`),
        ].join("\n"),
      );
      return;

    case "missing_source_entry":
      console.error(
        [
          `missing artifact-backed payload input: ${failure.relativePath}`,
          "",
          "Render `.stage/` first, then stage one artifact directory per runtime package slug.",
        ].join("\n"),
      );
      return;

    case "missing_rendered_entry":
      console.error(
        [
          `missing rendered publish workspace input: ${failure.relativePath}`,
          "",
          "Render `.stage/` before staging payloads into it.",
        ].join("\n"),
      );
      return;

    case "invalid_payload":
      console.error(`invalid staged payload ${failure.relativePath}: ${failure.detail}`);
      return;

    case "inspection_failed":
      console.error(
        `failed to inspect ${failure.relativePath}: ${String(failure.cause)}`,
      );
      return;

    case "copy_failed":
      console.error(
        `failed to copy ${failure.relativePath}: ${String(failure.cause)}`,
      );
      return;
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
