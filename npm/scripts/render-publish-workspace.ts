#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  PUBLISH_WORKSPACE_DIRECTORY,
  PUBLISH_LICENSE_FILE_NAMES,
  PUBLISH_WRAPPER_SCHEMAS,
  RUNTIME_PACKAGE_DEFINITIONS,
  publishWorkspaceTextFiles,
} from "./package-manifests.ts";

type RenderPublishWorkspaceResult =
  | {
      readonly kind: "ok";
      readonly writtenPaths: readonly string[];
    }
  | {
      readonly kind: "missing_source_entry";
      readonly relativePath: string;
    }
  | {
      readonly kind: "filesystem_error";
      readonly operation: "create" | "remove" | "write" | "copy" | "rename" | "restore";
      readonly relativePath: string;
      readonly cause: unknown;
    };

type RenderPublishWorkspaceFailure = Exclude<
  RenderPublishWorkspaceResult,
  { readonly kind: "ok" }
>;

const WORKSPACE_ROOT_DIRECTORY = path.resolve(
  fileURLToPath(new URL("..", import.meta.url)),
);
const SOURCE_WRAPPER_DIRECTORY = "sprawl-guard" as const;
const SOURCE_SCHEMA_DIRECTORY = "../site/public/schemas/v1" as const;

/** Renders the script-free staged publish workspace under `.stage/`. */
export function renderPublishWorkspace(args: {
  readonly workspaceRootDirectory: string;
}): RenderPublishWorkspaceResult {
  const writtenPaths: string[] = [];
  const stageRootDirectory = path.join(
    args.workspaceRootDirectory,
    PUBLISH_WORKSPACE_DIRECTORY,
  );
  const temporaryStageRootDirectory = createTemporaryStageRootDirectory(
    args.workspaceRootDirectory,
  );
  if (temporaryStageRootDirectory.kind !== "ok") {
    return temporaryStageRootDirectory;
  }

  const sourceEntries = [
    `${SOURCE_WRAPPER_DIRECTORY}/README.md`,
    `${SOURCE_WRAPPER_DIRECTORY}/dist`,
    ...PUBLISH_WRAPPER_SCHEMAS.map(
      (schema) => `${SOURCE_SCHEMA_DIRECTORY}/${schema.fileName}`,
    ),
    ...PUBLISH_LICENSE_FILE_NAMES.map((fileName) => `../${fileName}`),
  ];

  for (const sourceRelativePath of sourceEntries) {
    if (!fs.existsSync(path.join(args.workspaceRootDirectory, sourceRelativePath))) {
      safeRemoveDirectory(temporaryStageRootDirectory.path);
      return {
        kind: "missing_source_entry",
        relativePath: sourceRelativePath,
      };
    }
  }

  for (const textFile of publishWorkspaceTextFiles()) {
    const writeResult = writeTextFile(temporaryStageRootDirectory.path, textFile);
    if (writeResult.kind !== "ok") {
      safeRemoveDirectory(temporaryStageRootDirectory.path);
      return writeResult;
    }
    writtenPaths.push(textFile.relativePath);
  }

  const wrapperReadmeCopy = copySourceEntry({
    workspaceRootDirectory: args.workspaceRootDirectory,
    sourceRelativePath: `${SOURCE_WRAPPER_DIRECTORY}/README.md`,
    destinationRelativePath: `${PUBLISH_WORKSPACE_DIRECTORY}/${SOURCE_WRAPPER_DIRECTORY}/README.md`,
    temporaryStageRootDirectory: temporaryStageRootDirectory.path,
  });
  if (wrapperReadmeCopy.kind !== "ok") {
    safeRemoveDirectory(temporaryStageRootDirectory.path);
    return wrapperReadmeCopy;
  }
  writtenPaths.push(wrapperReadmeCopy.destinationRelativePath);

  const wrapperDistCopy = copySourceEntry({
    workspaceRootDirectory: args.workspaceRootDirectory,
    sourceRelativePath: `${SOURCE_WRAPPER_DIRECTORY}/dist`,
    destinationRelativePath: `${PUBLISH_WORKSPACE_DIRECTORY}/${SOURCE_WRAPPER_DIRECTORY}/dist`,
    temporaryStageRootDirectory: temporaryStageRootDirectory.path,
  });
  if (wrapperDistCopy.kind !== "ok") {
    safeRemoveDirectory(temporaryStageRootDirectory.path);
    return wrapperDistCopy;
  }
  writtenPaths.push(wrapperDistCopy.destinationRelativePath);

  for (const schema of PUBLISH_WRAPPER_SCHEMAS) {
    const schemaCopy = copySourceEntry({
      workspaceRootDirectory: args.workspaceRootDirectory,
      sourceRelativePath: `${SOURCE_SCHEMA_DIRECTORY}/${schema.fileName}`,
      destinationRelativePath: `${PUBLISH_WORKSPACE_DIRECTORY}/${SOURCE_WRAPPER_DIRECTORY}/${schema.packagePath}`,
      temporaryStageRootDirectory: temporaryStageRootDirectory.path,
    });
    if (schemaCopy.kind !== "ok") {
      safeRemoveDirectory(temporaryStageRootDirectory.path);
      return schemaCopy;
    }
    writtenPaths.push(schemaCopy.destinationRelativePath);
  }

  for (const fileName of PUBLISH_LICENSE_FILE_NAMES) {
    for (const destinationDirectory of [
      SOURCE_WRAPPER_DIRECTORY,
      ...runtimePackageDirectories(),
    ]) {
      const licenseCopy = copySourceEntry({
        workspaceRootDirectory: args.workspaceRootDirectory,
        sourceRelativePath: `../${fileName}`,
        destinationRelativePath: `${PUBLISH_WORKSPACE_DIRECTORY}/${destinationDirectory}/${fileName}`,
        temporaryStageRootDirectory: temporaryStageRootDirectory.path,
      });
      if (licenseCopy.kind !== "ok") {
        safeRemoveDirectory(temporaryStageRootDirectory.path);
        return licenseCopy;
      }
      writtenPaths.push(licenseCopy.destinationRelativePath);
    }
  }

  const replaceResult = replaceStageRoot({
    stageRootDirectory,
    temporaryStageRootDirectory: temporaryStageRootDirectory.path,
  });
  if (replaceResult.kind !== "ok") {
    safeRemoveDirectory(temporaryStageRootDirectory.path);
    return replaceResult;
  }

  return {
    kind: "ok",
    writtenPaths,
  };
}

function createTemporaryStageRootDirectory(workspaceRootDirectory: string):
  | {
      readonly kind: "ok";
      readonly path: string;
    }
  | RenderPublishWorkspaceFailure {
  try {
    return {
      kind: "ok",
      path: fs.mkdtempSync(path.join(workspaceRootDirectory, ".stage.tmp-")),
    };
  } catch (cause) {
    return {
      kind: "filesystem_error",
      operation: "create",
      relativePath: ".stage.tmp-*",
      cause,
    };
  }
}

function writeTextFile(
  temporaryStageRootDirectory: string,
  textFile: {
    readonly relativePath: string;
    readonly text: string;
  },
):
  | {
      readonly kind: "ok";
    }
  | RenderPublishWorkspaceFailure {
  const destinationPath = path.join(
    temporaryStageRootDirectory,
    relativePathInsidePublishWorkspace(textFile.relativePath),
  );

  try {
    fs.mkdirSync(path.dirname(destinationPath), { recursive: true });
    fs.writeFileSync(destinationPath, textFile.text);
  } catch (cause) {
    return {
      kind: "filesystem_error",
      operation: "write",
      relativePath: textFile.relativePath,
      cause,
    };
  }

  return {
    kind: "ok",
  };
}

function copySourceEntry(args: {
  readonly workspaceRootDirectory: string;
  readonly sourceRelativePath: string;
  readonly destinationRelativePath: string;
  readonly temporaryStageRootDirectory: string;
}):
  | {
      readonly kind: "ok";
      readonly destinationRelativePath: string;
    }
  | RenderPublishWorkspaceFailure {
  const sourcePath = path.join(args.workspaceRootDirectory, args.sourceRelativePath);

  const destinationPath = path.join(
    args.temporaryStageRootDirectory,
    relativePathInsidePublishWorkspace(args.destinationRelativePath),
  );

  try {
    fs.mkdirSync(path.dirname(destinationPath), { recursive: true });
    fs.cpSync(sourcePath, destinationPath, { recursive: true });
  } catch (cause) {
    return {
      kind: "filesystem_error",
      operation: "copy",
      relativePath: args.destinationRelativePath,
      cause,
    };
  }

  return {
    kind: "ok",
    destinationRelativePath: args.destinationRelativePath,
  };
}

function relativePathInsidePublishWorkspace(relativePath: string): string {
  return relativePath.slice(`${PUBLISH_WORKSPACE_DIRECTORY}/`.length);
}

function runtimePackageDirectories(): string[] {
  return RUNTIME_PACKAGE_DEFINITIONS.map(
    (definition) => `packages/${definition.slug}`,
  );
}

function replaceStageRoot(args: {
  readonly stageRootDirectory: string;
  readonly temporaryStageRootDirectory: string;
}):
  | {
      readonly kind: "ok";
    }
  | RenderPublishWorkspaceFailure {
  const backupStageRootDirectory = `${args.stageRootDirectory}.previous-${process.pid}-${Date.now()}`;
  let hasBackup = false;

  if (fs.existsSync(args.stageRootDirectory)) {
    try {
      fs.renameSync(args.stageRootDirectory, backupStageRootDirectory);
      hasBackup = true;
    } catch (cause) {
      return {
        kind: "filesystem_error",
        operation: "rename",
        relativePath: PUBLISH_WORKSPACE_DIRECTORY,
        cause,
      };
    }
  }

  try {
    fs.renameSync(args.temporaryStageRootDirectory, args.stageRootDirectory);
  } catch (cause) {
    if (hasBackup) {
      try {
        fs.renameSync(backupStageRootDirectory, args.stageRootDirectory);
      } catch (restoreCause) {
        return {
          kind: "filesystem_error",
          operation: "restore",
          relativePath: PUBLISH_WORKSPACE_DIRECTORY,
          cause: restoreCause,
        };
      }
    }

    return {
      kind: "filesystem_error",
      operation: "rename",
      relativePath: PUBLISH_WORKSPACE_DIRECTORY,
      cause,
    };
  }

  if (hasBackup) {
    safeRemoveDirectory(backupStageRootDirectory);
  }

  return {
    kind: "ok",
  };
}

function safeRemoveDirectory(directoryPath: string): void {
  try {
    fs.rmSync(directoryPath, { recursive: true, force: true });
  } catch {
    // Best-effort cleanup only.
  }
}

function main(): number {
  const result = renderPublishWorkspace({
    workspaceRootDirectory: WORKSPACE_ROOT_DIRECTORY,
  });

  if (result.kind === "ok") {
    return 0;
  }

  printFailure(result);
  return 1;
}

function printFailure(failure: RenderPublishWorkspaceFailure): void {
  switch (failure.kind) {
    case "missing_source_entry":
      console.error(
        [
          `missing source workspace input: ${failure.relativePath}`,
          "",
          "Build the wrapper before rendering `.stage/`.",
        ].join("\n"),
      );
      return;

    case "filesystem_error":
      console.error(
        `failed to ${failure.operation} ${failure.relativePath}: ${String(failure.cause)}`,
      );
      return;
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
