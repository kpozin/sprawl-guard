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
  const workspaceRoot = fs.realpathSync(
    fs.mkdtempSync(path.join(os.tmpdir(), "sprawl-guard-public-version-")),
  );
  temporaryDirectories.push(workspaceRoot);
  const cargoVersion = args.cargoVersion ?? "0.1.0";
  const npmVersion = args.npmVersion ?? cargoVersion;
  fs.mkdirSync(path.join(workspaceRoot, "rust/cli/src"), {
    recursive: true,
  });
  fs.mkdirSync(path.join(workspaceRoot, "rust/library/src"), {
    recursive: true,
  });
  fs.mkdirSync(path.join(workspaceRoot, "js/npm"), {
    recursive: true,
  });
  fs.writeFileSync(
    path.join(workspaceRoot, "Cargo.toml"),
    `[workspace]\nmembers = ["rust/cli", "rust/library"]\nresolver = "2"\n`,
  );
  fs.writeFileSync(
    path.join(workspaceRoot, "rust/cli/Cargo.toml"),
    `[package]\nname = "sprawl-guard"\nversion = "${cargoVersion}"\nedition = "2024"\n\n[dependencies]\nsprawl-guard-lib = { version = "=${cargoVersion}", path = "../library" }\n`,
  );
  fs.writeFileSync(
    path.join(workspaceRoot, "rust/cli/src/main.rs"),
    "fn main() {}\n",
  );
  fs.writeFileSync(
    path.join(workspaceRoot, "rust/library/Cargo.toml"),
    `[package]\nname = "sprawl-guard-lib"\nversion = "${cargoVersion}"\nedition = "2024"\n`,
  );
  fs.writeFileSync(
    path.join(workspaceRoot, "rust/library/src/lib.rs"),
    "pub fn marker() {}\n",
  );
  fs.writeFileSync(
    path.join(
      workspaceRoot,
      "js/npm/package-version.json",
    ),
    `${JSON.stringify({ version: npmVersion }, null, 2)}\n`,
  );
  return workspaceRoot;
}

function createPublicWorkspace(): string {
  const workspaceRoot = fs.realpathSync(
    fs.mkdtempSync(path.join(os.tmpdir(), "sprawl-guard-public-layout-version-")),
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
      "js/npm/scripts/set-public-version.ts",
    ),
  );
  assert.equal(resolved.kind, "ok");
  return resolved.layout;
}

describe("resolvePublicPackageVersionLayout", () => {
  describe("when the npm workspace is nested below the Cargo workspace", () => {
    it("skips nearer Cargo workspaces that lack the packages", () => {
      const workspaceRoot = createWorkspace();
      fs.writeFileSync(
        path.join(workspaceRoot, "js/Cargo.toml"),
        `[workspace]\nmembers = ["missing"]\n`,
      );

      const resolved = resolvePublicPackageVersionLayout(
        path.join(workspaceRoot, "js/npm/scripts/set-public-version.ts"),
      );

      assert.equal(resolved.kind, "ok");
      assert.deepEqual(resolved.layout, {
        workspaceRoot,
        cliManifest: path.join(workspaceRoot, "rust/cli/Cargo.toml"),
        libraryManifest: path.join(workspaceRoot, "rust/library/Cargo.toml"),
        npmVersion: path.join(workspaceRoot, "js/npm/package-version.json"),
      });
    });
  });

  describe("when the npm workspace sits at the Cargo workspace root", () => {
    it("checks and updates the Cargo and npm versions", () => {
      const workspaceRoot = createPublicWorkspace();
      const resolved = resolvePublicPackageVersionLayout(
        path.join(workspaceRoot, "npm/scripts/set-public-version.ts"),
      );

      assert.equal(resolved.kind, "ok");
      assert.equal(resolved.layout.workspaceRoot, workspaceRoot);
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

  describe("when no Cargo workspace contains both packages", () => {
    it("reports why each Cargo workspace was skipped", () => {
      const workspaceRoot = createWorkspace();
      fs.writeFileSync(
        path.join(workspaceRoot, "Cargo.toml"),
        `[workspace]\nmembers = ["rust/library"]\nresolver = "2"\n`,
      );

      const resolved = resolvePublicPackageVersionLayout(
        path.join(workspaceRoot, "js/npm/scripts/set-public-version.ts"),
      );

      assert.equal(resolved.kind, "invalid_layout");
      assert.deepEqual(resolved.rejectedWorkspaces[0], {
        workspaceRoot,
        detail:
          "cargo metadata must contain exactly one public CLI and library package",
      });
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
          path.join(workspaceRoot, "rust/cli/Cargo.toml"),
          "utf8",
        ),
        /sprawl-guard-lib = \{ version = "=0\.2\.0-beta\.1"/u,
      );
      assert.deepEqual(
        JSON.parse(
          fs.readFileSync(
            path.join(
              workspaceRoot,
              "js/npm/package-version.json",
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
        "rust/cli/Cargo.toml",
      );
      const npmVersionPath = path.join(
        workspaceRoot,
        "js/npm/package-version.json",
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
        "rust/cli/Cargo.toml",
      );
      const libraryManifestPath = path.join(
        workspaceRoot,
        "rust/library/Cargo.toml",
      );
      fs.writeFileSync(
        cliManifestPath,
        fs
          .readFileSync(cliManifestPath, "utf8")
          .replace('version = "0.1.0"', 'version="0.1.0"')
          .replace(
            'sprawl-guard-lib = { version = "=0.1.0", path = "../library" }',
            'sprawl-guard-lib={version="=0.1.0",path="../library"}',
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
        /sprawl-guard-lib=\{version="=0\.2\.0",path="\.\.\/library"\}/u,
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
        path.join(workspaceRoot, "rust/cli/Cargo.toml"),
        path.join(workspaceRoot, "rust/library/Cargo.toml"),
        path.join(
          workspaceRoot,
          "js/npm/package-version.json",
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
