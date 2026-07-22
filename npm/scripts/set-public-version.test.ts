import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, it, mock } from "node:test";
import {
  checkPublicPackageVersionConsistency,
  resolvePublicPackageVersionLayout,
  setPublicPackageVersion,
  type PublicPackageVersionLayout,
} from "./set-public-version.ts";

const temporaryDirectories: string[] = [];

afterEach(() => {
  for (const directory of temporaryDirectories.splice(0)) {
    fs.rmSync(directory, { force: true, recursive: true });
  }
});

function createWorkspace(
  args: {
    readonly cargoVersion?: string;
    readonly npmVersion?: string;
  } = {},
): string {
  const workspaceRoot = fs.mkdtempSync(
    path.join(os.tmpdir(), "sprawl-guard-public-version-"),
  );
  temporaryDirectories.push(workspaceRoot);
  const cargoVersion = args.cargoVersion ?? "0.1.0";
  const npmVersion = args.npmVersion ?? cargoVersion;
  fs.mkdirSync(path.join(workspaceRoot, "tools/sprawl-guard/src"), {
    recursive: true,
  });
  fs.mkdirSync(path.join(workspaceRoot, "tools/sprawl-guard-lib/src"), {
    recursive: true,
  });
  fs.mkdirSync(path.join(workspaceRoot, "tools/sprawl-guard-public/npm"), {
    recursive: true,
  });
  fs.writeFileSync(
    path.join(workspaceRoot, "Cargo.toml"),
    `[workspace]\nmembers = ["tools/sprawl-guard", "tools/sprawl-guard-lib"]\nresolver = "2"\n`,
  );
  fs.writeFileSync(
    path.join(workspaceRoot, "tools/sprawl-guard/Cargo.toml"),
    `[package]\nname = "sprawl-guard"\nversion = "${cargoVersion}"\nedition = "2024"\n\n[dependencies]\nsprawl-guard-lib = { version = "=${cargoVersion}", path = "../sprawl-guard-lib" }\n`,
  );
  fs.writeFileSync(
    path.join(workspaceRoot, "tools/sprawl-guard/src/main.rs"),
    "fn main() {}\n",
  );
  fs.writeFileSync(
    path.join(workspaceRoot, "tools/sprawl-guard-lib/Cargo.toml"),
    `[package]\nname = "sprawl-guard-lib"\nversion = "${cargoVersion}"\nedition = "2024"\n`,
  );
  fs.writeFileSync(
    path.join(workspaceRoot, "tools/sprawl-guard-lib/src/lib.rs"),
    "pub fn marker() {}\n",
  );
  fs.writeFileSync(
    path.join(
      workspaceRoot,
      "tools/sprawl-guard-public/npm/package-version.json",
    ),
    `${JSON.stringify({ version: npmVersion }, null, 2)}\n`,
  );
  return workspaceRoot;
}

function createPublicWorkspace(): string {
  const workspaceRoot = fs.mkdtempSync(
    path.join(os.tmpdir(), "sprawl-guard-public-layout-version-"),
  );
  temporaryDirectories.push(workspaceRoot);
  fs.mkdirSync(path.join(workspaceRoot, "crates/sprawl-guard/src"), {
    recursive: true,
  });
  fs.mkdirSync(path.join(workspaceRoot, "crates/sprawl-guard-lib/src"), {
    recursive: true,
  });
  fs.mkdirSync(path.join(workspaceRoot, "npm/scripts"), { recursive: true });
  fs.writeFileSync(
    path.join(workspaceRoot, "Cargo.toml"),
    `[workspace]\nmembers = ["crates/sprawl-guard", "crates/sprawl-guard-lib"]\nresolver = "2"\n`,
  );
  fs.writeFileSync(
    path.join(workspaceRoot, "crates/sprawl-guard/Cargo.toml"),
    `[package]\nname = "sprawl-guard"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\nsprawl-guard-lib = { version = "=0.1.0", path = "../sprawl-guard-lib" }\n`,
  );
  fs.writeFileSync(
    path.join(workspaceRoot, "crates/sprawl-guard/src/main.rs"),
    "fn main() {}\n",
  );
  fs.writeFileSync(
    path.join(workspaceRoot, "crates/sprawl-guard-lib/Cargo.toml"),
    `[package]\nname = "sprawl-guard-lib"\nversion = "0.1.0"\nedition = "2024"\n`,
  );
  fs.writeFileSync(
    path.join(workspaceRoot, "crates/sprawl-guard-lib/src/lib.rs"),
    "pub fn marker() {}\n",
  );
  fs.writeFileSync(
    path.join(workspaceRoot, "npm/package-version.json"),
    '{"version":"0.1.0"}\n',
  );
  return workspaceRoot;
}

function cargoPackageVersions(
  workspaceRoot: string,
): ReadonlyMap<string, string> {
  const result = spawnSync(
    "cargo",
    ["metadata", "--format-version", "1", "--no-deps"],
    { cwd: workspaceRoot, encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  const metadata = JSON.parse(result.stdout) as {
    readonly packages: readonly {
      readonly name: string;
      readonly version: string;
    }[];
  };
  return new Map(metadata.packages.map(({ name, version }) => [name, version]));
}

function resolveFixtureLayout(
  workspaceRoot: string,
): PublicPackageVersionLayout {
  const resolved = resolvePublicPackageVersionLayout(
    path.join(
      workspaceRoot,
      "tools/sprawl-guard-public/npm/scripts/set-public-version.ts",
    ),
  );
  assert.equal(resolved.kind, "ok");
  return resolved.layout;
}

describe("resolvePublicPackageVersionLayout", () => {
  describe("when the script is in the nested npm workspace", () => {
    it("resolves nested Cargo and npm paths", () => {
      const workspaceRoot = createWorkspace();

      const resolved = resolvePublicPackageVersionLayout(
        path.join(
          workspaceRoot,
          "tools/sprawl-guard-public/npm/scripts/set-public-version.ts",
        ),
      );

      assert.equal(resolved.kind, "ok");
      assert.equal(resolved.layout.kind, "nested");
      assert.equal(resolved.layout.workspaceRoot, workspaceRoot);
    });
  });

  describe("when the script is in the exported public npm workspace", () => {
    it("checks and updates the exported Cargo and npm paths", () => {
      const workspaceRoot = createPublicWorkspace();
      const resolved = resolvePublicPackageVersionLayout(
        path.join(workspaceRoot, "npm/scripts/set-public-version.ts"),
      );

      assert.equal(resolved.kind, "ok");
      assert.equal(resolved.layout.kind, "public");
      assert.deepEqual(checkPublicPackageVersionConsistency(resolved.layout), {
        kind: "ok",
        version: "0.1.0",
      });
      assert.deepEqual(
        setPublicPackageVersion({
          layout: resolved.layout,
          newVersion: "0.2.0",
        }),
        { kind: "ok" },
      );
      assert.deepEqual(
        cargoPackageVersions(workspaceRoot),
        new Map([
          ["sprawl-guard", "0.2.0"],
          ["sprawl-guard-lib", "0.2.0"],
        ]),
      );
      assert.deepEqual(
        JSON.parse(
          fs.readFileSync(
            path.join(workspaceRoot, "npm/package-version.json"),
            "utf8",
          ),
        ),
        { version: "0.2.0" },
      );
    });
  });

  describe("when no supported repository layout is complete", () => {
    it("reports the missing paths for every candidate", () => {
      const workspaceRoot = createWorkspace();
      const npmVersionPath = path.join(
        workspaceRoot,
        "tools/sprawl-guard-public/npm/package-version.json",
      );
      fs.rmSync(npmVersionPath);

      const resolved = resolvePublicPackageVersionLayout(
        path.join(
          workspaceRoot,
          "tools/sprawl-guard-public/npm/scripts/set-public-version.ts",
        ),
      );

      assert.equal(resolved.kind, "invalid_layout");
      assert.equal(resolved.reason, "no_match");
      assert.equal(
        resolved.candidates
          .find(({ kind }) => kind === "nested")
          ?.missingPaths.includes(npmVersionPath),
        true,
      );
    });
  });

  describe("when both supported repository layouts are complete", () => {
    it("reports the ambiguous candidates instead of choosing by order", () => {
      const workspaceRoot = createWorkspace();
      const nestedPublicRoot = path.join(
        workspaceRoot,
        "tools/sprawl-guard-public",
      );
      for (const relativePath of [
        "Cargo.toml",
        "crates/sprawl-guard/Cargo.toml",
        "crates/sprawl-guard-lib/Cargo.toml",
      ]) {
        const candidatePath = path.join(nestedPublicRoot, relativePath);
        fs.mkdirSync(path.dirname(candidatePath), { recursive: true });
        fs.writeFileSync(candidatePath, "");
      }

      const resolved = resolvePublicPackageVersionLayout(
        path.join(
          workspaceRoot,
          "tools/sprawl-guard-public/npm/scripts/set-public-version.ts",
        ),
      );

      assert.equal(resolved.kind, "invalid_layout");
      assert.equal(resolved.reason, "ambiguous_match");
      assert.equal(resolved.candidates.length, 2);
      assert.equal(
        resolved.candidates.every(
          ({ missingPaths }) => missingPaths.length === 0,
        ),
        true,
      );
    });
  });
});

describe("setPublicPackageVersion", () => {
  describe("when every current version agrees", () => {
    it("updates both Cargo packages, their exact dependency, and npm metadata", () => {
      const workspaceRoot = createWorkspace();

      const result = setPublicPackageVersion({
        layout: resolveFixtureLayout(workspaceRoot),
        newVersion: "0.2.0-beta.1",
      });

      assert.deepEqual(result, { kind: "ok" });
      assert.deepEqual(
        cargoPackageVersions(workspaceRoot),
        new Map([
          ["sprawl-guard", "0.2.0-beta.1"],
          ["sprawl-guard-lib", "0.2.0-beta.1"],
        ]),
      );
      assert.match(
        fs.readFileSync(
          path.join(workspaceRoot, "tools/sprawl-guard/Cargo.toml"),
          "utf8",
        ),
        /sprawl-guard-lib = \{ version = "=0\.2\.0-beta\.1"/u,
      );
      assert.deepEqual(
        JSON.parse(
          fs.readFileSync(
            path.join(
              workspaceRoot,
              "tools/sprawl-guard-public/npm/package-version.json",
            ),
            "utf8",
          ),
        ),
        { version: "0.2.0-beta.1" },
      );
    });

    it("can be rerun with the already-selected version", () => {
      const workspaceRoot = createWorkspace();

      assert.deepEqual(
        setPublicPackageVersion({
          layout: resolveFixtureLayout(workspaceRoot),
          newVersion: "0.2.0",
        }),
        { kind: "ok" },
      );
      assert.deepEqual(
        setPublicPackageVersion({
          layout: resolveFixtureLayout(workspaceRoot),
          newVersion: "0.2.0",
        }),
        { kind: "ok" },
      );
    });
  });

  describe("when checked-in versions disagree", () => {
    it("reports the mismatch without modifying any version source", () => {
      const workspaceRoot = createWorkspace({ npmVersion: "0.1.1" });
      const cargoManifestPath = path.join(
        workspaceRoot,
        "tools/sprawl-guard/Cargo.toml",
      );
      const npmVersionPath = path.join(
        workspaceRoot,
        "tools/sprawl-guard-public/npm/package-version.json",
      );
      const originalCargoManifest = fs.readFileSync(cargoManifestPath, "utf8");
      const originalNpmVersion = fs.readFileSync(npmVersionPath, "utf8");

      const result = setPublicPackageVersion({
        layout: resolveFixtureLayout(workspaceRoot),
        newVersion: "0.2.0",
      });

      assert.equal(result.kind, "version_mismatch");
      assert.equal(
        fs.readFileSync(cargoManifestPath, "utf8"),
        originalCargoManifest,
      );
      assert.equal(fs.readFileSync(npmVersionPath, "utf8"), originalNpmVersion);
    });
  });

  describe("when Cargo manifests use alternate valid formatting", () => {
    it("updates the semantic TOML fields without reformatting unrelated content", () => {
      const workspaceRoot = createWorkspace();
      const cliManifestPath = path.join(
        workspaceRoot,
        "tools/sprawl-guard/Cargo.toml",
      );
      const libraryManifestPath = path.join(
        workspaceRoot,
        "tools/sprawl-guard-lib/Cargo.toml",
      );
      fs.writeFileSync(
        cliManifestPath,
        fs
          .readFileSync(cliManifestPath, "utf8")
          .replace('version = "0.1.0"', 'version="0.1.0"')
          .replace(
            'sprawl-guard-lib = { version = "=0.1.0", path = "../sprawl-guard-lib" }',
            'sprawl-guard-lib={version="=0.1.0",path="../sprawl-guard-lib"}',
          ),
      );
      fs.writeFileSync(
        libraryManifestPath,
        fs
          .readFileSync(libraryManifestPath, "utf8")
          .replace('version = "0.1.0"', 'version="0.1.0"'),
      );

      assert.deepEqual(
        setPublicPackageVersion({
          layout: resolveFixtureLayout(workspaceRoot),
          newVersion: "0.2.0",
        }),
        { kind: "ok" },
      );
      assert.match(
        fs.readFileSync(cliManifestPath, "utf8"),
        /version="0\.2\.0"/u,
      );
      assert.match(
        fs.readFileSync(cliManifestPath, "utf8"),
        /sprawl-guard-lib=\{version="=0\.2\.0",path="\.\.\/sprawl-guard-lib"\}/u,
      );
      assert.match(
        fs.readFileSync(libraryManifestPath, "utf8"),
        /version="0\.2\.0"/u,
      );
    });
  });

  describe("when replacing the second version source fails", () => {
    it("restores every source to its original contents", () => {
      const workspaceRoot = createWorkspace();
      const versionPaths = [
        path.join(workspaceRoot, "tools/sprawl-guard/Cargo.toml"),
        path.join(workspaceRoot, "tools/sprawl-guard-lib/Cargo.toml"),
        path.join(
          workspaceRoot,
          "tools/sprawl-guard-public/npm/package-version.json",
        ),
      ];
      const originalContents = versionPaths.map((versionPath) =>
        fs.readFileSync(versionPath, "utf8"),
      );
      const originalCopy = fs.copyFileSync;
      let replacementCount = 0;
      const copyMock = mock.method(
        fs,
        "copyFileSync",
        (source: fs.PathLike, destination: fs.PathLike, mode?: number) => {
          if (String(source).includes(".tmp-") && ++replacementCount === 2) {
            throw new Error("injected replacement failure");
          }
          return mode === undefined
            ? originalCopy(source, destination)
            : originalCopy(source, destination, mode);
        },
      );
      try {
        assert.equal(
          setPublicPackageVersion({
            layout: resolveFixtureLayout(workspaceRoot),
            newVersion: "0.2.0",
          }).kind,
          "write_failed",
        );
      } finally {
        copyMock.mock.restore();
      }

      assert.deepEqual(
        versionPaths.map((versionPath) => fs.readFileSync(versionPath, "utf8")),
        originalContents,
      );
    });
  });

  describe("when restoring a partially replaced workspace fails", () => {
    it("retains and reports every recovery backup", () => {
      const workspaceRoot = createWorkspace();
      const originalCopy = fs.copyFileSync;
      let replacementCount = 0;
      let rollbackFailed = false;
      const copyMock = mock.method(
        fs,
        "copyFileSync",
        (source: fs.PathLike, destination: fs.PathLike, mode?: number) => {
          if (String(source).includes(".tmp-") && ++replacementCount === 2) {
            throw new Error("injected replacement failure");
          }
          if (String(source).includes(".backup-") && !rollbackFailed) {
            rollbackFailed = true;
            throw new Error("injected rollback failure");
          }
          return mode === undefined
            ? originalCopy(source, destination)
            : originalCopy(source, destination, mode);
        },
      );
      let result;
      try {
        result = setPublicPackageVersion({
          layout: resolveFixtureLayout(workspaceRoot),
          newVersion: "0.2.0",
        });
      } finally {
        copyMock.mock.restore();
      }

      assert.equal(result.kind, "write_failed");
      assert.match(result.detail, /rollback failed/u);
      assert.equal(result.retainedPaths.length, 3);
      for (const backupPath of result.retainedPaths) {
        assert.equal(fs.existsSync(backupPath), true);
      }
    });
  });

  describe("when auxiliary-file cleanup fails after a successful replacement", () => {
    it("returns a structured warning and leaves the updated versions valid", () => {
      const workspaceRoot = createWorkspace();
      const originalRemove = fs.rmSync;
      let cleanupFailed = false;
      const removeMock = mock.method(
        fs,
        "rmSync",
        (target: fs.PathLike, options?: fs.RmDirOptions) => {
          if (String(target).includes(".tmp-") && !cleanupFailed) {
            cleanupFailed = true;
            throw new Error("injected cleanup failure");
          }
          return originalRemove(target, options);
        },
      );
      let result;
      try {
        result = setPublicPackageVersion({
          layout: resolveFixtureLayout(workspaceRoot),
          newVersion: "0.2.0",
        });
      } finally {
        removeMock.mock.restore();
      }

      assert.equal(result.kind, "cleanup_failed");
      assert.equal(result.retainedPaths.length, 1);
      assert.deepEqual(
        checkPublicPackageVersionConsistency(
          resolveFixtureLayout(workspaceRoot),
        ),
        { kind: "ok", version: "0.2.0" },
      );
    });
  });

  describe("when verification fails after the initial cleanup also fails", () => {
    it("restores the sources and preserves the initial retained path", () => {
      const workspaceRoot = createWorkspace();
      const layout = resolveFixtureLayout(workspaceRoot);
      const originalCopy = fs.copyFileSync;
      const originalRemove = fs.rmSync;
      let replacementCount = 0;
      let cleanupFailed = false;
      const copyMock = mock.method(
        fs,
        "copyFileSync",
        (source: fs.PathLike, destination: fs.PathLike, mode?: number) => {
          const result =
            mode === undefined
              ? originalCopy(source, destination)
              : originalCopy(source, destination, mode);
          if (String(source).includes(".tmp-") && ++replacementCount === 3) {
            fs.writeFileSync(destination, '{"version":"0.3.0"}\n');
          }
          return result;
        },
      );
      const removeMock = mock.method(
        fs,
        "rmSync",
        (target: fs.PathLike, options?: fs.RmDirOptions) => {
          if (!cleanupFailed) {
            cleanupFailed = true;
            throw new Error("injected initial cleanup failure");
          }
          return originalRemove(target, options);
        },
      );
      let result;
      try {
        result = setPublicPackageVersion({ layout, newVersion: "0.2.0" });
      } finally {
        copyMock.mock.restore();
        removeMock.mock.restore();
      }

      assert.equal(result.kind, "cleanup_failed");
      assert.match(result.detail, /version verification failed/u);
      assert.match(result.detail, /initial cleanup failed/u);
      assert.equal(result.retainedPaths.length, 1);
      assert.equal(fs.existsSync(result.retainedPaths[0] ?? ""), true);
      assert.deepEqual(checkPublicPackageVersionConsistency(layout), {
        kind: "ok",
        version: "0.1.0",
      });
    });
  });

  describe("when verification fails before restoration cleanup fails", () => {
    it("reports cleanup rather than claiming that restoration failed", () => {
      const workspaceRoot = createWorkspace();
      const layout = resolveFixtureLayout(workspaceRoot);
      const originalCopy = fs.copyFileSync;
      const originalRemove = fs.rmSync;
      let replacementCount = 0;
      let cleanupCount = 0;
      const copyMock = mock.method(
        fs,
        "copyFileSync",
        (source: fs.PathLike, destination: fs.PathLike, mode?: number) => {
          const result =
            mode === undefined
              ? originalCopy(source, destination)
              : originalCopy(source, destination, mode);
          if (String(source).includes(".tmp-") && ++replacementCount === 3) {
            fs.writeFileSync(destination, '{"version":"0.3.0"}\n');
          }
          return result;
        },
      );
      const removeMock = mock.method(
        fs,
        "rmSync",
        (target: fs.PathLike, options?: fs.RmDirOptions) => {
          if (++cleanupCount === 7) {
            throw new Error("injected restoration cleanup failure");
          }
          return originalRemove(target, options);
        },
      );
      let result;
      try {
        result = setPublicPackageVersion({ layout, newVersion: "0.2.0" });
      } finally {
        copyMock.mock.restore();
        removeMock.mock.restore();
      }

      assert.equal(result.kind, "cleanup_failed");
      assert.match(result.detail, /restoration cleanup failed/u);
      assert.doesNotMatch(result.detail, /restoration failed:/u);
      assert.equal(result.retainedPaths.length, 1);
      assert.deepEqual(checkPublicPackageVersionConsistency(layout), {
        kind: "ok",
        version: "0.1.0",
      });
    });
  });

  describe("when cleanup fails after a successful rollback", () => {
    it("reports the retained auxiliary file without losing the restored sources", () => {
      const workspaceRoot = createWorkspace();
      const originalCopy = fs.copyFileSync;
      const originalRemove = fs.rmSync;
      let replacementCount = 0;
      let cleanupFailed = false;
      const copyMock = mock.method(
        fs,
        "copyFileSync",
        (source: fs.PathLike, destination: fs.PathLike, mode?: number) => {
          if (String(source).includes(".tmp-") && ++replacementCount === 2) {
            throw new Error("injected replacement failure");
          }
          return mode === undefined
            ? originalCopy(source, destination)
            : originalCopy(source, destination, mode);
        },
      );
      const removeMock = mock.method(
        fs,
        "rmSync",
        (target: fs.PathLike, options?: fs.RmDirOptions) => {
          if (String(target).includes(".tmp-") && !cleanupFailed) {
            cleanupFailed = true;
            throw new Error("injected cleanup failure");
          }
          return originalRemove(target, options);
        },
      );
      let result;
      try {
        result = setPublicPackageVersion({
          layout: resolveFixtureLayout(workspaceRoot),
          newVersion: "0.2.0",
        });
      } finally {
        copyMock.mock.restore();
        removeMock.mock.restore();
      }

      assert.equal(result.kind, "write_failed");
      assert.equal(result.retainedPaths.length, 1);
      assert.deepEqual(
        checkPublicPackageVersionConsistency(
          resolveFixtureLayout(workspaceRoot),
        ),
        { kind: "ok", version: "0.1.0" },
      );
    });
  });

  describe("when the requested version cannot be published consistently", () => {
    it("rejects it without modifying the workspace", () => {
      const workspaceRoot = createWorkspace();

      const result = setPublicPackageVersion({
        layout: resolveFixtureLayout(workspaceRoot),
        newVersion: "version-two",
      });

      assert.deepEqual(result, {
        kind: "invalid_version",
        detail: "version must be canonical npm SemVer without build metadata",
        value: "version-two",
      });
      assert.deepEqual(
        cargoPackageVersions(workspaceRoot).get("sprawl-guard"),
        "0.1.0",
      );
    });

    it("rejects registry-incompatible SemVer without modifying the workspace", () => {
      const workspaceRoot = createWorkspace();
      const layout = resolveFixtureLayout(workspaceRoot);

      for (const newVersion of [
        "9007199254740992.0.0",
        `1.0.0-${"a".repeat(251)}`,
        "1.0.0+build.1",
      ]) {
        assert.deepEqual(setPublicPackageVersion({ layout, newVersion }), {
          kind: "invalid_version",
          detail: "version must be canonical npm SemVer without build metadata",
          value: newVersion,
        });
      }
      assert.equal(
        cargoPackageVersions(workspaceRoot).get("sprawl-guard"),
        "0.1.0",
      );
    });
  });
});

describe("checkPublicPackageVersionConsistency", () => {
  describe("when Cargo and npm versions agree", () => {
    it("returns the shared version without modifying the workspace", () => {
      const workspaceRoot = createWorkspace();

      assert.deepEqual(
        checkPublicPackageVersionConsistency(
          resolveFixtureLayout(workspaceRoot),
        ),
        {
          kind: "ok",
          version: "0.1.0",
        },
      );
    });
  });

  describe("when Cargo and npm versions disagree", () => {
    it("returns the mismatch for the normal check path", () => {
      const workspaceRoot = createWorkspace({ npmVersion: "0.1.1" });

      assert.deepEqual(
        checkPublicPackageVersionConsistency(
          resolveFixtureLayout(workspaceRoot),
        ),
        {
          kind: "version_mismatch",
          cargoVersion: "0.1.0",
          npmVersion: "0.1.1",
        },
      );
    });
  });
});
