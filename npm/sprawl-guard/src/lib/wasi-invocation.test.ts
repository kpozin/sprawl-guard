import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { unsafeAbsolutePathFromKnownAbsolute as absolute, unsafeGuestPath } from "./paths.js";
import { buildWasiInvocation, wasiFileSystem } from "./wasi-invocation.js";

describe("buildWasiInvocation", () => {
  describe("when the host is POSIX", () => {
    const plan = buildWasiInvocation(
      ["check", "--root", "/work/repo", "--config=../custom.toml", "src\\odd name.ts"],
      "linux",
      absolute("/work/repo/src"),
      {},
    );

    test("it mounts the host root at its own path", () => {
      assert.deepEqual([...plan.fileSystem.preopens], [["/", "/"]]);
    });

    test("it starts in the host working directory", () => {
      assert.equal(plan.env.PWD, "/work/repo/src");
    });

    test("it passes every argument unchanged", () => {
      assert.deepEqual(plan.args, [
        "sprawl-guard",
        "check",
        "--root",
        "/work/repo",
        "--config=../custom.toml",
        "src\\odd name.ts",
      ]);
    });
  });

  describe("when the host is POSIX and GitHub Actions declares a workspace", () => {
    test("it forwards the workspace unchanged", () => {
      const plan = buildWasiInvocation(["check"], "linux", absolute("/work/repo"), {
        GITHUB_WORKSPACE: "/work",
      });

      assert.deepEqual(plan.env, { PWD: "/work/repo", GITHUB_WORKSPACE: "/work" });
    });
  });

  describe("when no workspace is declared", () => {
    test("it sets only the working directory", () => {
      const plan = buildWasiInvocation(["check"], "linux", absolute("/work/repo"), {});

      assert.deepEqual(plan.env, { PWD: "/work/repo" });
    });
  });

  describe("when the host is Windows", () => {
    describe("and GitHub Actions declares a workspace on a mounted drive", () => {
      test("it spells the workspace under its drive's module path", () => {
        const plan = buildWasiInvocation(["check"], "win32", absolute("D:\\a\\repo\\repo\\app"), {
          GITHUB_WORKSPACE: "D:\\a\\repo\\repo",
        });

        assert.equal(plan.env.GITHUB_WORKSPACE, "/d/a/repo/repo");
      });
    });

    describe("and the declared workspace is on a drive the run does not use", () => {
      const plan = buildWasiInvocation(["--version"], "win32", absolute("C:\\work"), {
        GITHUB_WORKSPACE: "Z:\\gone",
      });

      test("it mounts only the drives the run uses", () => {
        assert.deepEqual([...plan.fileSystem.preopens], [["/c", "C:\\"]]);
      });

      test("it forwards the workspace under a module path that no mount covers", () => {
        assert.deepEqual(plan.env, { PWD: "/c/work", GITHUB_WORKSPACE: "/z/gone" });
        assert.equal(plan.fileSystem.toHost(unsafeGuestPath("/z/gone")), undefined);
      });
    });

    describe("and the declared workspace is not an absolute path", () => {
      test("it leaves the workspace out", () => {
        const plan = buildWasiInvocation(["check"], "win32", absolute("C:\\work"), {
          GITHUB_WORKSPACE: "repo",
        });

        assert.deepEqual(plan.env, { PWD: "/c/work" });
      });
    });

    describe("and every path is on the working directory's drive", () => {
      const plan = buildWasiInvocation(
        ["check", "--root", "C:\\work\\repo", "--config=..\\custom.toml", "src\\lib.ts", "\\x.ts"],
        "win32",
        absolute("C:\\work\\repo\\src"),
        {},
      );

      test("it mounts only that drive", () => {
        assert.deepEqual([...plan.fileSystem.preopens], [["/c", "C:\\"]]);
      });

      test("it starts in the working directory under the drive's module path", () => {
        assert.equal(plan.env.PWD, "/c/work/repo/src");
      });

      test("it respells absolute and relative paths with forward slashes", () => {
        assert.deepEqual(plan.args, [
          "sprawl-guard",
          "check",
          "--root",
          "/c/work/repo",
          "--config=../custom.toml",
          "src/lib.ts",
          "/c/x.ts",
        ]);
      });
    });

    describe("and an argument names another volume", () => {
      const plan = buildWasiInvocation(
        ["check", "--root=d:\\repo", "\\\\server\\share\\a.ts"],
        "win32",
        absolute("C:\\work"),
        {},
      );

      test("it mounts every named volume", () => {
        assert.deepEqual(
          [...plan.fileSystem.preopens],
          [
            ["/c", "C:\\"],
            ["/d", "d:\\"],
            ["/unc/server/share", "\\\\server\\share\\"],
          ],
        );
      });

      test("it spells each path under its volume's module path", () => {
        assert.deepEqual(plan.args, ["sprawl-guard", "check", "--root=/d/repo", "/unc/server/share/a.ts"]);
      });
    });

    describe("and a path has an extended-length prefix", () => {
      const plan = buildWasiInvocation(
        ["check", "--root=\\\\?\\UNC\\server\\share\\repo", "\\\\?\\D:\\a.ts"],
        "win32",
        absolute("C:\\work"),
        {},
      );

      test("it mounts the volume that the path names", () => {
        assert.deepEqual([...plan.fileSystem.preopens.keys()], ["/c", "/unc/server/share", "/d"]);
      });

      test("it spells the path as it would without the prefix", () => {
        assert.deepEqual(plan.args, ["sprawl-guard", "check", "--root=/unc/server/share/repo", "/d/a.ts"]);
      });
    });

    describe("and a path is relative to a drive", () => {
      const plan = buildWasiInvocation(["explain", "C:src\\a.ts"], "win32", absolute("C:\\work"), {});

      test("it resolves the path under the working directory", () => {
        assert.deepEqual(plan.args, ["sprawl-guard", "explain", "/c/work/src/a.ts"]);
      });
    });

    describe("and two paths spell one share in different cases", () => {
      const plan = buildWasiInvocation(["check", "\\\\server\\share\\w\\a.ts"], "win32", absolute("\\\\Server\\Share\\w"), {});

      test("it mounts the share once and starts under that mount", () => {
        assert.deepEqual([...plan.fileSystem.preopens.keys()], ["/unc/server/share"]);
        assert.equal(plan.env.PWD, "/unc/server/share/w");
      });
    });
  });
});

describe("wasiFileSystem", () => {
  describe("when the host is Windows", () => {
    const fileSystem = wasiFileSystem("win32", absolute("C:\\work"));

    describe("and a host path is on a mounted volume", () => {
      test("it maps the path both ways", () => {
        assert.equal(fileSystem.toGuest(absolute("c:\\work\\a.ts")), "/c/work/a.ts");
        assert.equal(fileSystem.toHost(unsafeGuestPath("/c/work/a.ts")), "C:\\work\\a.ts");
      });
    });

    describe("and a host path is on another volume", () => {
      test("it reports that the module cannot see the path", () => {
        assert.equal(fileSystem.toGuest(absolute("D:\\work\\a.ts")), undefined);
      });
    });

    describe("and a module path is outside every volume", () => {
      test("it reports that the path has no host spelling", () => {
        assert.equal(fileSystem.toHost(unsafeGuestPath("/d/work")), undefined);
      });
    });
  });
});
