import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import {
  REGISTRY_INSTALL_ATTEMPTS,
  WRAPPER_ENTRY_POINT,
  checkNpmInstall,
  parseNpmInstallCheckArguments,
  type NpmInstallCheck,
  type NpmInstallCheckOperations,
} from "./check-npm-install.ts";

const SCRIPT_PATH = fileURLToPath(new URL("./check-npm-install.ts", import.meta.url));
const RELEASE_VERSION = "1.2.3";
const OLDER_VERSION = "1.2.2";
const WRAPPER_PACKAGE_NAME = "@kpozin/sprawl-guard";
const RUNTIME_PACKAGE_NAME = "@kpozin/sprawl-guard-linux-x64-musl";
const LARGE_FILE_BYTES = 600 * 1024;
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
 * the wrapper's entry point prints the installed version and any configured stderr, and `exec`
 * runs the bin link unless it is broken.
 */
function fakeNpm(args: {
  readonly binLink?: "broken";
  readonly installs: (InstalledVersions | "not_found")[];
  readonly versionOutput?: string;
  readonly wrapperStderr?: string;
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
      if (command === "exec") {
        return args.binLink === "broken"
          ? { kind: "failed", detail: "npm exec exited with 127" }
          : { kind: "ok", stdout: "Usage: sprawl-guard <COMMAND>\n" };
      }
      assert.equal(command, "install");
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
    },
    runEntryPoint: (entryPoint, argument) => {
      calls.push([entryPoint, argument]);
      const stderr = args.wrapperStderr ?? "";
      return argument === "--version"
        ? {
          kind: "ok",
          stdout: args.versionOutput ?? `sprawl-guard ${installedWrapper}\n`,
          stderr,
        }
        : { kind: "ok", stdout: "Usage: sprawl-guard <COMMAND>\n", stderr };
    },
    sleepBeforeRetry: async () => {
      sleeps.count += 1;
    },
  };
}

/** Where the check finds the installed wrapper's entry point. */
function wrapperEntryPoint(projectDirectory: string): string {
  return path.join(projectDirectory, "node_modules", WRAPPER_PACKAGE_NAME, WRAPPER_ENTRY_POINT);
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

        assert.equal(result.kind, "ok");
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
          [wrapperEntryPoint(projectDirectory), "--version"],
          [wrapperEntryPoint(projectDirectory), "--help"],
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

        assert.equal(result.kind, "ok");
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

        assert.equal(result.kind, "ok");
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

        assert.equal(result.kind, "ok");
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

        assert.equal(result.kind, "ok");
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

  describe("when the installed wrapper writes to stderr", () => {
    it("fails with the warning even though the version matches", async () => {
      const warning = "ExperimentalWarning: WASI is an experimental feature\n";
      const npm = fakeNpm({
        installs: [{ runtime: RELEASE_VERSION, wrapper: RELEASE_VERSION }],
        wrapperStderr: warning,
      });

      const result = await checkNpmInstall(
        registryCheck(absentDirectory(), "automatic"),
        npm,
      );

      assert.deepEqual(result, {
        kind: "unexpected_stderr",
        argument: "--version",
        stderr: warning,
      });
    });
  });

  describe("when npm cannot run the installed bin link", () => {
    it("fails after the entry point runs cleanly", async () => {
      const npm = fakeNpm({
        binLink: "broken",
        installs: [{ runtime: RELEASE_VERSION, wrapper: RELEASE_VERSION }],
      });

      const result = await checkNpmInstall(
        registryCheck(absentDirectory(), "automatic"),
        npm,
      );

      assert.deepEqual(result, { kind: "bin_link_failed", detail: "npm exec exited with 127" });
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

/** Builds candidate archives for an install with a payload larger than the overhead budget. */
function fixtureCandidate(
  reportedVersion: string,
  extraWrapperFiles: Readonly<Record<string, string>> = {},
  wrapperStderr = "",
): string {
  const candidateDirectory = mkdtempSync(path.join(TEMPORARY_ROOT, "candidate-"));
  packFixture(
    candidateDirectory,
    {
      bin: { "sprawl-guard": WRAPPER_ENTRY_POINT },
      name: WRAPPER_PACKAGE_NAME,
      version: RELEASE_VERSION,
    },
    {
      ...extraWrapperFiles,
      [WRAPPER_ENTRY_POINT]: `#!/usr/bin/env node\nprocess.stderr.write(${JSON.stringify(wrapperStderr)});\nconsole.log(process.argv[2] === "--version" ? "sprawl-guard ${reportedVersion}" : "Usage: sprawl-guard <COMMAND>");\n`,
    },
  );
  packFixture(
    candidateDirectory,
    { name: RUNTIME_PACKAGE_NAME, version: RELEASE_VERSION },
    { "bin/sprawl-guard": "x".repeat(LARGE_FILE_BYTES) },
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
    {
      encoding: "utf8",
      // npm warns about the unknown setting on stderr, which the wrapper check must not mistake
      // for the wrapper's own output.
      env: { ...process.env, npm_config_offline: "true", npm_config_sprawl_guard_unknown: "true" },
    },
  );
}

describe("WRAPPER_ENTRY_POINT", () => {
  it("is the wrapper package's `sprawl-guard` bin", () => {
    const manifest = JSON.parse(
      readFileSync(new URL("../../sprawl-guard/package.json", import.meta.url), "utf8"),
    ) as { readonly bin: Readonly<Record<string, string>> };

    assert.equal(path.posix.normalize(manifest.bin["sprawl-guard"] ?? ""), WRAPPER_ENTRY_POINT);
  });
});

describe("check-npm-install command", () => {
  describe("when the candidate wrapper reports the release version", () => {
    it("reports the installed footprint with the CLI payload outside the overhead budget", () => {
      const result = runScript(fixtureCandidate(RELEASE_VERSION));

      assert.equal(result.status, 0, String(result.stderr));
      const report = /node_modules: (\d+) bytes total, (\d+) bytes outside CLI payloads/u.exec(
        String(result.stdout),
      );
      assert.ok(report);
      assert.equal(Number(report[1]) - Number(report[2]), LARGE_FILE_BYTES);
    });
  });

  describe("when unpacked wrapper files exceed the footprint budget", () => {
    it("fails and reports the size outside CLI payloads", () => {
      const result = runScript(
        fixtureCandidate(RELEASE_VERSION, { "dist/large.js": "x".repeat(LARGE_FILE_BYTES) }),
      );

      assert.equal(result.status, 1);
      assert.match(String(result.stderr), /npm install exceeds the footprint budget/u);
    });
  });

  describe("when the candidate wrapper reports another version", () => {
    it("exits with an explanation", () => {
      const result = runScript(fixtureCandidate(OLDER_VERSION));

      assert.equal(result.status, 1);
      assert.match(String(result.stderr), /sprawl-guard --version printed "sprawl-guard 1\.2\.2"/u);
    });
  });

  describe("when the candidate wrapper prints a warning", () => {
    it("exits with the warning", () => {
      const result = runScript(fixtureCandidate(RELEASE_VERSION, {}, "loader warning\n"));

      assert.equal(result.status, 1);
      assert.match(
        String(result.stderr),
        /sprawl-guard --version wrote to stderr: "loader warning"/u,
      );
    });
  });
});
