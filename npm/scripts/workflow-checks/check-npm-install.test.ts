import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import {
  REGISTRY_INSTALL_ATTEMPTS,
  checkNpmInstall,
  npmSpawn,
  parseNpmInstallCheckArguments,
  type NpmInstallCheck,
  type NpmInstallCheckOperations,
} from "./check-npm-install.ts";

const SCRIPT_PATH = fileURLToPath(new URL("./check-npm-install.ts", import.meta.url));
const RELEASE_VERSION = "1.2.3";
const OLDER_VERSION = "1.2.2";
const WRAPPER_PACKAGE_NAME = "@kpozin/sprawl-guard";
const RUNTIME_PACKAGE_NAME = "@kpozin/sprawl-guard-linux-x64-musl";
const TEMPORARY_ROOT = mkdtempSync(path.join(tmpdir(), "check-npm-install-"));

after(() => {
  rmSync(TEMPORARY_ROOT, { force: true, recursive: true });
});

/** Returns a directory path under the test root that does not exist yet. */
function absentDirectory(): string {
  return path.join(mkdtempSync(path.join(TEMPORARY_ROOT, "case-")), "project");
}

/** The versions that one fake `npm install` leaves in the project. */
interface InstalledVersions {
  readonly runtime: string | null;
  readonly wrapper: string;
}

/** Writes an installed package's manifest the way npm lays it out. */
function writeInstalledManifest(
  projectDirectory: string,
  packageName: string,
  version: string,
): void {
  const packageDirectory = path.join(projectDirectory, "node_modules", ...packageName.split("/"));
  mkdirSync(packageDirectory, { recursive: true });
  writeFileSync(
    path.join(packageDirectory, "package.json"),
    JSON.stringify({ name: packageName, version }),
  );
}

/**
 * Fakes npm: each install writes the next queued set of versions or fails like a registry 404,
 * and `exec` prints the installed wrapper's version.
 */
function fakeNpm(args: {
  readonly installs: (InstalledVersions | "not_found")[];
  readonly versionOutput?: string;
}): NpmInstallCheckOperations & {
  readonly calls: (readonly string[])[];
  readonly sleeps: { count: number };
} {
  const calls: (readonly string[])[] = [];
  const sleeps = { count: 0 };
  let installedWrapper = "";
  return {
    calls,
    sleeps,
    runNpm: (npmArguments) => {
      calls.push(npmArguments);
      const [prefixFlag, projectDirectory, command] = npmArguments;
      assert.equal(prefixFlag, "--prefix");
      assert.ok(projectDirectory !== undefined);
      if (command === "install") {
        const installed = args.installs.shift();
        if (installed === undefined || installed === "not_found") {
          return { kind: "failed", detail: "npm error 404 Not Found" };
        }
        installedWrapper = installed.wrapper;
        writeInstalledManifest(projectDirectory, WRAPPER_PACKAGE_NAME, installed.wrapper);
        if (installed.runtime !== null) {
          writeInstalledManifest(projectDirectory, RUNTIME_PACKAGE_NAME, installed.runtime);
        }
        return { kind: "ok", stdout: "added 2 packages\n" };
      }
      return npmArguments.at(-1) === "--version"
        ? { kind: "ok", stdout: args.versionOutput ?? `sprawl-guard ${installedWrapper}\n` }
        : { kind: "ok", stdout: "Usage: sprawl-guard <COMMAND>\n" };
    },
    sleepBeforeRetry: async () => {
      sleeps.count += 1;
    },
  };
}

/** Parses command-line arguments that the test expects to be valid. */
function parsedCheck(argv: readonly string[]): NpmInstallCheck {
  const parsed = parseNpmInstallCheckArguments(argv);
  assert.equal(parsed.kind, "ok", JSON.stringify(parsed));
  return parsed.check;
}

function candidateCheck(projectDirectory: string): NpmInstallCheck {
  return parsedCheck([
    "--project",
    projectDirectory,
    "--version",
    RELEASE_VERSION,
    "--runtime-package",
    RUNTIME_PACKAGE_NAME,
    "--candidate-directory",
    "/candidate",
  ]);
}

function registryCheck(
  projectDirectory: string,
  runtimeInstallation: "automatic" | "explicit",
): NpmInstallCheck {
  return parsedCheck([
    "--project",
    projectDirectory,
    "--version",
    RELEASE_VERSION,
    "--runtime-package",
    RUNTIME_PACKAGE_NAME,
    "--dist-tag",
    "latest",
    "--runtime-installation",
    runtimeInstallation,
  ]);
}

describe("checkNpmInstall", () => {
  describe("when installing candidate archives", () => {
    describe("and both packages have the release version", () => {
      it("installs both archives without optional dependencies and runs the wrapper", async () => {
        const projectDirectory = absentDirectory();
        const npm = fakeNpm({
          installs: [{ runtime: RELEASE_VERSION, wrapper: RELEASE_VERSION }],
        });

        const result = await checkNpmInstall(candidateCheck(projectDirectory), npm);

        assert.deepEqual(result, { kind: "ok" });
        assert.deepEqual(npm.calls, [
          [
            "--prefix",
            projectDirectory,
            "install",
            "--save-exact",
            "--omit=optional",
            path.resolve("/candidate", `kpozin-sprawl-guard-${RELEASE_VERSION}.tgz`),
            path.resolve("/candidate", `kpozin-sprawl-guard-linux-x64-musl-${RELEASE_VERSION}.tgz`),
          ],
          ["--prefix", projectDirectory, "exec", "--", "sprawl-guard", "--version"],
          ["--prefix", projectDirectory, "exec", "--", "sprawl-guard", "--help"],
        ]);
      });
    });

    describe("and the runtime package has another version", () => {
      it("fails without retrying", async () => {
        const npm = fakeNpm({
          installs: [
            { runtime: OLDER_VERSION, wrapper: RELEASE_VERSION },
            { runtime: RELEASE_VERSION, wrapper: RELEASE_VERSION },
          ],
        });

        const result = await checkNpmInstall(candidateCheck(absentDirectory()), npm);

        assert.deepEqual(result, {
          kind: "version_mismatch",
          installedVersion: OLDER_VERSION,
          packageName: RUNTIME_PACKAGE_NAME,
        });
        assert.equal(npm.calls.length, 1);
        assert.equal(npm.sleeps.count, 0);
      });
    });
  });

  describe("when installing from the registry", () => {
    describe("and npm chooses the runtime package", () => {
      it("installs only the wrapper from the dist-tag", async () => {
        const projectDirectory = absentDirectory();
        const npm = fakeNpm({
          installs: [{ runtime: RELEASE_VERSION, wrapper: RELEASE_VERSION }],
        });

        const result = await checkNpmInstall(registryCheck(projectDirectory, "automatic"), npm);

        assert.deepEqual(result, { kind: "ok" });
        assert.deepEqual(npm.calls[0], [
          "--prefix",
          projectDirectory,
          "install",
          "--save-exact",
          "--prefer-online",
          `${WRAPPER_PACKAGE_NAME}@latest`,
        ]);
      });
    });

    describe("and the runtime package is named explicitly", () => {
      it("installs it at the release version without optional dependencies", async () => {
        const projectDirectory = absentDirectory();
        const npm = fakeNpm({
          installs: [{ runtime: RELEASE_VERSION, wrapper: RELEASE_VERSION }],
        });

        const result = await checkNpmInstall(registryCheck(projectDirectory, "explicit"), npm);

        assert.deepEqual(result, { kind: "ok" });
        assert.deepEqual(npm.calls[0], [
          "--prefix",
          projectDirectory,
          "install",
          "--save-exact",
          "--prefer-online",
          "--omit=optional",
          `${WRAPPER_PACKAGE_NAME}@latest`,
          `${RUNTIME_PACKAGE_NAME}@${RELEASE_VERSION}`,
        ]);
      });
    });

    describe("and npm still serves the previous release for a while", () => {
      it("reinstalls into a clean project until the release appears", async () => {
        const npm = fakeNpm({
          installs: [
            { runtime: null, wrapper: OLDER_VERSION },
            { runtime: OLDER_VERSION, wrapper: RELEASE_VERSION },
            { runtime: RELEASE_VERSION, wrapper: RELEASE_VERSION },
          ],
        });

        const result = await checkNpmInstall(
          registryCheck(absentDirectory(), "automatic"),
          npm,
        );

        assert.deepEqual(result, { kind: "ok" });
        assert.equal(npm.sleeps.count, 2);
      });
    });

    describe("and npm does not serve a package archive yet", () => {
      it("retries the install", async () => {
        const npm = fakeNpm({
          installs: ["not_found", { runtime: RELEASE_VERSION, wrapper: RELEASE_VERSION }],
        });

        const result = await checkNpmInstall(
          registryCheck(absentDirectory(), "automatic"),
          npm,
        );

        assert.deepEqual(result, { kind: "ok" });
        assert.equal(npm.sleeps.count, 1);
      });
    });

    describe("and npm never serves the release", () => {
      it("reports the last failure after the attempt budget", async () => {
        const npm = fakeNpm({
          installs: Array.from({ length: REGISTRY_INSTALL_ATTEMPTS + 1 }, () => ({
            runtime: OLDER_VERSION,
            wrapper: OLDER_VERSION,
          })),
        });

        const result = await checkNpmInstall(
          registryCheck(absentDirectory(), "automatic"),
          npm,
        );

        assert.deepEqual(result, {
          kind: "version_mismatch",
          installedVersion: OLDER_VERSION,
          packageName: WRAPPER_PACKAGE_NAME,
        });
        assert.equal(npm.calls.length, REGISTRY_INSTALL_ATTEMPTS);
        assert.equal(npm.sleeps.count, REGISTRY_INSTALL_ATTEMPTS - 1);
      });
    });
  });

  describe("when the installed wrapper reports another version", () => {
    it("fails after installing once", async () => {
      const npm = fakeNpm({
        installs: [{ runtime: RELEASE_VERSION, wrapper: RELEASE_VERSION }],
        versionOutput: `sprawl-guard ${OLDER_VERSION}\n`,
      });

      const result = await checkNpmInstall(
        registryCheck(absentDirectory(), "automatic"),
        npm,
      );

      assert.deepEqual(result, {
        kind: "unexpected_version_output",
        output: `sprawl-guard ${OLDER_VERSION}`,
      });
      assert.equal(npm.sleeps.count, 0);
    });
  });

  describe("when the project directory already exists", () => {
    it("fails before running npm", async () => {
      const projectDirectory = absentDirectory();
      mkdirSync(projectDirectory);
      const npm = fakeNpm({ installs: [] });

      const result = await checkNpmInstall(candidateCheck(projectDirectory), npm);

      assert.deepEqual(result, { kind: "project_exists", projectDirectory });
      assert.deepEqual(npm.calls, []);
    });
  });
});

describe("parseNpmInstallCheckArguments", () => {
  const commonArguments = [
    "--project",
    "install-smoke",
    "--version",
    RELEASE_VERSION,
    "--runtime-package",
    RUNTIME_PACKAGE_NAME,
  ];

  describe("when given a candidate directory", () => {
    it("resolves both directories against the working directory", () => {
      const check = parsedCheck([...commonArguments, "--candidate-directory", "candidate"]);

      assert.equal(check.projectDirectory, path.resolve("install-smoke"));
      assert.deepEqual(check.source, {
        kind: "candidate",
        directory: path.resolve("candidate"),
      });
    });
  });

  describe("when given both a candidate directory and a dist-tag", () => {
    it("rejects the ambiguous source", () => {
      const parsed = parseNpmInstallCheckArguments([
        ...commonArguments,
        "--candidate-directory",
        "candidate",
        "--dist-tag",
        "latest",
        "--runtime-installation",
        "automatic",
      ]);

      assert.equal(parsed.kind, "invalid_arguments");
    });
  });

  describe("when given a dist-tag without a runtime installation mode", () => {
    it("rejects the arguments", () => {
      const parsed = parseNpmInstallCheckArguments([
        ...commonArguments,
        "--dist-tag",
        "latest",
      ]);

      assert.equal(parsed.kind, "invalid_arguments");
    });
  });

  describe("when given a runtime package outside Sprawl Guard's scope", () => {
    it("rejects the arguments", () => {
      const parsed = parseNpmInstallCheckArguments([
        "--project",
        "install-smoke",
        "--version",
        RELEASE_VERSION,
        "--runtime-package",
        "left-pad",
        "--candidate-directory",
        "candidate",
      ]);

      assert.equal(parsed.kind, "invalid_arguments");
    });
  });

  describe("when given a version that is not SemVer", () => {
    it("rejects the arguments", () => {
      const parsed = parseNpmInstallCheckArguments([
        "--project",
        "install-smoke",
        "--version",
        "v1.2.3",
        "--runtime-package",
        RUNTIME_PACKAGE_NAME,
        "--candidate-directory",
        "candidate",
      ]);

      assert.equal(parsed.kind, "invalid_arguments");
    });
  });
});

/** Packs a minimal package into the candidate directory with the file name npm gives it. */
function packFixture(
  candidateDirectory: string,
  manifest: Record<string, unknown>,
  files: Readonly<Record<string, string>>,
): void {
  const sourceDirectory = mkdtempSync(path.join(TEMPORARY_ROOT, "fixture-"));
  writeFileSync(path.join(sourceDirectory, "package.json"), JSON.stringify(manifest));
  for (const [relativePath, contents] of Object.entries(files)) {
    mkdirSync(path.dirname(path.join(sourceDirectory, relativePath)), { recursive: true });
    writeFileSync(path.join(sourceDirectory, relativePath), contents);
  }
  const packed = spawnSync("npm", ["pack", "--pack-destination", candidateDirectory], {
    cwd: sourceDirectory,
    encoding: "utf8",
    shell: process.platform === "win32",
  });
  assert.equal(packed.status, 0, packed.stderr);
}

/** Builds candidate archives for a fake wrapper that reports `reportedVersion`. */
function fixtureCandidate(reportedVersion: string): string {
  const candidateDirectory = mkdtempSync(path.join(TEMPORARY_ROOT, "candidate-"));
  packFixture(
    candidateDirectory,
    {
      bin: { "sprawl-guard": "bin/sprawl-guard.js" },
      name: WRAPPER_PACKAGE_NAME,
      version: RELEASE_VERSION,
    },
    {
      "bin/sprawl-guard.js": `#!/usr/bin/env node\nconsole.log(process.argv[2] === "--version" ? "sprawl-guard ${reportedVersion}" : "Usage: sprawl-guard <COMMAND>");\n`,
    },
  );
  packFixture(
    candidateDirectory,
    { name: RUNTIME_PACKAGE_NAME, version: RELEASE_VERSION },
    { "README.md": "runtime fixture\n" },
  );
  assert.equal(readdirSync(candidateDirectory).length, 2);
  return candidateDirectory;
}

/** Runs the script the way the release workflow does. */
function runScript(candidateDirectory: string): ReturnType<typeof spawnSync> {
  return spawnSync(
    process.execPath,
    [
      SCRIPT_PATH,
      "--project",
      absentDirectory(),
      "--version",
      RELEASE_VERSION,
      "--runtime-package",
      RUNTIME_PACKAGE_NAME,
      "--candidate-directory",
      candidateDirectory,
    ],
    { encoding: "utf8", env: { ...process.env, npm_config_offline: "true" } },
  );
}

describe("npmSpawn", () => {
  describe("on Windows", () => {
    it("starts npm through the shell by its bare name, with each argument quoted", () => {
      assert.deepEqual(npmSpawn("win32", ["install", "C:\\Temp Dir\\wrapper.tgz"]), {
        kind: "shell",
        commandLine: 'npm "install" "C:\\Temp Dir\\wrapper.tgz"',
      });
    });
  });

  describe("on other platforms", () => {
    it("starts npm directly with its arguments", () => {
      assert.deepEqual(npmSpawn("linux", ["install", "/tmp/dir with space/wrapper.tgz"]), {
        kind: "direct",
        args: ["install", "/tmp/dir with space/wrapper.tgz"],
      });
    });
  });
});

describe("check-npm-install command", () => {
  describe("when the candidate wrapper reports the release version", () => {
    it("exits successfully", () => {
      const result = runScript(fixtureCandidate(RELEASE_VERSION));

      assert.equal(result.status, 0, String(result.stderr));
    });
  });

  describe("when the candidate wrapper reports another version", () => {
    it("exits with an explanation", () => {
      const result = runScript(fixtureCandidate(OLDER_VERSION));

      assert.equal(result.status, 1);
      assert.match(String(result.stderr), /sprawl-guard --version printed "sprawl-guard 1\.2\.2"/u);
    });
  });
});
