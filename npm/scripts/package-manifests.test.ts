import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, it } from "node:test";
import {
  PACKAGE_VERSION,
  RUNTIME_PACKAGE_DEFINITIONS,
  PUBLISH_WORKSPACE_DIRECTORY,
  publishWorkspaceTextFiles,
  renderPublishWrapperPackageManifest,
  renderRuntimePackageManifest,
  renderSourceWrapperPackageManifest,
} from "./package-manifests.ts";

describe("renderSourceWrapperPackageManifest", () => {
  describe("when rendering the checked-in source manifest", () => {
    it("keeps source-only scripts and omits publish-only runtime dependencies", () => {
      const manifest = renderSourceWrapperPackageManifest();

      assert.equal(manifest.private, true);
      assert.deepEqual(manifest.scripts, {
        build:
          "pnpm run clean && pnpm --dir .. exec tsc -p sprawl-guard/tsconfig.json",
        clean:
          "node -e \"require('node:fs').rmSync('dist', { recursive: true, force: true })\"",
        prepack: "pnpm build",
        test: "node --test dist/lib/*.test.js",
      });
      assert.deepEqual(manifest.files, [
        "README.md",
        "dist/",
        "!dist/lib/*.test.js",
      ]);
      assert.ok(!("optionalDependencies" in manifest));
      assert.ok(!("publishConfig" in manifest));
    });
  });
});

describe("renderPublishWrapperPackageManifest", () => {
  describe("when rendering the staged publish manifest", () => {
    it("lists every runtime package and stays script-free", () => {
      const manifest = renderPublishWrapperPackageManifest();

      assert.deepEqual(
        manifest.optionalDependencies,
        Object.fromEntries(
          RUNTIME_PACKAGE_DEFINITIONS.map((definition) => [
            definition.packageName,
            PACKAGE_VERSION,
          ]),
        ),
      );
      assert.deepEqual(manifest.publishConfig, {
        access: "public",
      });
      assert.deepEqual(manifest.files, [
        "README.md",
        "LICENSE-MIT",
        "LICENSE-APACHE",
        "schema/",
        "skills/",
        "dist/",
        "!dist/lib/*.test.js",
      ]);
      assert.ok(!("scripts" in manifest));
      assert.ok(!("private" in manifest));
    });
  });
});

describe("renderRuntimePackageManifest", () => {
  describe("when the target is native linux x64 gnu", () => {
    it("renders platform selectors without a lifecycle script", () => {
      const definition = RUNTIME_PACKAGE_DEFINITIONS.find(
        (candidate) => candidate.slug === "linux-x64-gnu",
      );
      assert.ok(definition);

      const manifest = renderRuntimePackageManifest(definition);

      assert.deepEqual(manifest, {
        name: "@kpozin/sprawl-guard-linux-x64-gnu",
        version: PACKAGE_VERSION,
        description: "Native linux x64 glibc binary package for sprawl-guard.",
        license: "(MIT OR Apache-2.0)",
        repository: {
          type: "git",
          url: "https://github.com/kpozin/sprawl-guard.git",
        },
        publishConfig: {
          access: "public",
          executableFiles: ["bin/sprawl-guard"],
        },
        os: ["linux"],
        cpu: ["x64"],
        libc: ["glibc"],
        files: ["README.md", "LICENSE-MIT", "LICENSE-APACHE", "bin/"],
      });
      assert.ok(!("scripts" in manifest));
    });
  });

  describe("when pnpm packs a native runtime package", () => {
    it(
      "preserves the executable mode declared by the rendered manifest",
      { skip: process.platform === "win32" },
      () => {
        const definition = RUNTIME_PACKAGE_DEFINITIONS.find(
          (candidate) => candidate.slug === "linux-x64-gnu",
        );
        assert.ok(definition);

        const tempRoot = fs.mkdtempSync(
          path.join(os.tmpdir(), "sprawl-guard-native-pack-"),
        );
        try {
          const packageDirectory = path.join(tempRoot, "package");
          const packDirectory = path.join(tempRoot, "packed");
          const extractDirectory = path.join(tempRoot, "extracted");
          const binaryPath = path.join(packageDirectory, "bin", "sprawl-guard");
          fs.mkdirSync(path.dirname(binaryPath), { recursive: true });
          fs.mkdirSync(packDirectory);
          fs.mkdirSync(extractDirectory);
          fs.writeFileSync(
            path.join(packageDirectory, "package.json"),
            `${JSON.stringify(renderRuntimePackageManifest(definition), null, 2)}\n`,
          );
          fs.writeFileSync(
            path.join(packageDirectory, "README.md"),
            "fixture\n",
          );
          fs.writeFileSync(
            path.join(packageDirectory, "LICENSE-MIT"),
            "fixture\n",
          );
          fs.writeFileSync(
            path.join(packageDirectory, "LICENSE-APACHE"),
            "fixture\n",
          );
          fs.writeFileSync(binaryPath, "fixture\n");
          fs.chmodSync(binaryPath, 0o755);

          const packResult = spawnSync(
            "pnpm",
            [
              "--dir",
              packageDirectory,
              "pack",
              "--pack-destination",
              packDirectory,
            ],
            { encoding: "utf8" },
          );
          assert.equal(packResult.status, 0, packResult.stderr);
          const archiveNames = fs.readdirSync(packDirectory);
          assert.equal(archiveNames.length, 1);
          const [archiveName] = archiveNames;
          assert.ok(archiveName);

          const extractResult = spawnSync(
            "tar",
            [
              "-xzf",
              path.join(packDirectory, archiveName),
              "-C",
              extractDirectory,
            ],
            { encoding: "utf8" },
          );
          assert.equal(extractResult.status, 0, extractResult.stderr);

          const packedBinaryMode = fs.statSync(
            path.join(extractDirectory, "package", "bin", "sprawl-guard"),
          ).mode;
          assert.equal(packedBinaryMode & 0o111, 0o111);
        } finally {
          fs.rmSync(tempRoot, { recursive: true, force: true });
        }
      },
    );
  });

  describe("when the target is the WASI fallback", () => {
    it("omits platform selectors and lifecycle scripts", () => {
      const definition = RUNTIME_PACKAGE_DEFINITIONS.find(
        (candidate) => candidate.slug === "wasm32-wasip1",
      );
      assert.ok(definition);

      const manifest = renderRuntimePackageManifest(definition);

      assert.deepEqual(manifest, {
        name: "@kpozin/sprawl-guard-wasm32-wasip1",
        version: PACKAGE_VERSION,
        description: "WASI fallback package for sprawl-guard.",
        license: "(MIT OR Apache-2.0)",
        repository: {
          type: "git",
          url: "https://github.com/kpozin/sprawl-guard.git",
        },
        publishConfig: {
          access: "public",
        },
        files: [
          "README.md",
          "LICENSE-MIT",
          "LICENSE-APACHE",
          "sprawl-guard.wasm",
        ],
      });
      assert.ok(!("scripts" in manifest));
    });
  });
});

describe("publishWorkspaceTextFiles", () => {
  describe("when rendering the staged publish workspace", () => {
    it("includes the root workspace files, every publish manifest, and every generated package README", () => {
      const textFiles = publishWorkspaceTextFiles();
      const relativePaths = new Set(
        textFiles.map((textFile) => textFile.relativePath),
      );

      assert.ok(
        relativePaths.has(`${PUBLISH_WORKSPACE_DIRECTORY}/package.json`),
      );
      assert.ok(
        relativePaths.has(`${PUBLISH_WORKSPACE_DIRECTORY}/pnpm-workspace.yaml`),
      );
      assert.ok(
        relativePaths.has(
          `${PUBLISH_WORKSPACE_DIRECTORY}/sprawl-guard/package.json`,
        ),
      );

      for (const definition of RUNTIME_PACKAGE_DEFINITIONS) {
        assert.ok(
          relativePaths.has(
            `${PUBLISH_WORKSPACE_DIRECTORY}/packages/${definition.slug}/package.json`,
          ),
        );
        assert.ok(
          relativePaths.has(
            `${PUBLISH_WORKSPACE_DIRECTORY}/packages/${definition.slug}/README.md`,
          ),
        );
      }
    });
  });

  describe("when rendering a generated runtime package README", () => {
    it("describes the target from shared metadata instead of a checked-in per-target file", () => {
      const textFiles = publishWorkspaceTextFiles();
      const linuxReadme = textFiles.find(
        (textFile) =>
          textFile.relativePath ===
          `${PUBLISH_WORKSPACE_DIRECTORY}/packages/linux-x64-gnu/README.md`,
      );
      const wasiReadme = textFiles.find(
        (textFile) =>
          textFile.relativePath ===
          `${PUBLISH_WORKSPACE_DIRECTORY}/packages/wasm32-wasip1/README.md`,
      );

      assert.equal(
        linuxReadme?.text,
        [
          "# `@kpozin/sprawl-guard-linux-x64-gnu`",
          "",
          "This package carries the linux x64 glibc native `sprawl-guard` binary.",
          "Install `@kpozin/sprawl-guard` instead; it selects this package automatically when needed.",
          "",
          "See the [sprawl-guard project](https://github.com/kpozin/sprawl-guard#readme).",
          "",
        ].join("\n"),
      );
      assert.equal(
        wasiReadme?.text,
        [
          "# `@kpozin/sprawl-guard-wasm32-wasip1`",
          "",
          "This package carries the WASI fallback module for `sprawl-guard`.",
          "Install `@kpozin/sprawl-guard` instead; it selects this package automatically when needed.",
          "",
          "See the [sprawl-guard project](https://github.com/kpozin/sprawl-guard#readme).",
          "",
        ].join("\n"),
      );
    });
  });
});
