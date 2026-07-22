#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  PUBLISH_LICENSE_FILE_NAMES,
  PUBLISH_WORKSPACE_DIRECTORY,
  RUNTIME_PACKAGE_DEFINITIONS,
} from "./package-manifests.ts";

interface StagedPayloadPackVerificationRequest {
  readonly packageDirectory: string;
  readonly requiredPaths: readonly string[];
}

type VerifyStagedPayloadPacksResult =
  | {
      readonly kind: "ok";
    }
  | {
      readonly kind: "verification_failed";
      readonly packageDirectory: string;
      readonly status: number;
    };

const WORKSPACE_ROOT_DIRECTORY = path.resolve(
  fileURLToPath(new URL("..", import.meta.url)),
);
const VERIFY_PACKED_FILES_SCRIPT = fileURLToPath(
  new URL("verify-packed-files.ts", import.meta.url),
);

/** Builds verifier invocations for every rendered runtime package. */
export function stagedPayloadPackVerificationRequests(args: {
  readonly workspaceRootDirectory: string;
}): readonly StagedPayloadPackVerificationRequest[] {
  return RUNTIME_PACKAGE_DEFINITIONS.map((definition) => ({
    packageDirectory: path.join(
      args.workspaceRootDirectory,
      PUBLISH_WORKSPACE_DIRECTORY,
      "packages",
      definition.slug,
    ),
    requiredPaths: ["README.md", definition.payloadPath, ...PUBLISH_LICENSE_FILE_NAMES],
  }));
}

/** Verifies that every staged runtime package packs its payload and license files. */
export function verifyStagedPayloadPacks(args: {
  readonly workspaceRootDirectory: string;
}): VerifyStagedPayloadPacksResult {
  for (const request of stagedPayloadPackVerificationRequests(args)) {
    const verification = spawnSync(
      process.execPath,
      [
        VERIFY_PACKED_FILES_SCRIPT,
        request.packageDirectory,
        ...request.requiredPaths.flatMap((requiredPath) => ["--require", requiredPath]),
      ],
      {
        stdio: "inherit",
      },
    );

    if (verification.status !== 0) {
      return {
        kind: "verification_failed",
        packageDirectory: request.packageDirectory,
        status: verification.status ?? 1,
      };
    }
  }

  return {
    kind: "ok",
  };
}

function main(): number {
  const result = verifyStagedPayloadPacks({
    workspaceRootDirectory: WORKSPACE_ROOT_DIRECTORY,
  });

  if (result.kind === "ok") {
    return 0;
  }

  console.error(
    `failed staged payload package verification for ${result.packageDirectory}`,
  );
  return result.status;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
