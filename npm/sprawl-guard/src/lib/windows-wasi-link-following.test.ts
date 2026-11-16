import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { unsafeAbsolutePathFromKnownAbsolute as absolute } from "./paths.js";
import { wasiFileSystem } from "./wasi-invocation.js";
import { withNativeLinkFollowing } from "./windows-wasi.js";

/** WASI lookup flag that asks a call to follow a final symbolic link. */
const FOLLOW = 1;
/** WASI file types the fake status calls report. */
const DIRECTORY = 3;
const SYMBOLIC_LINK = 7;
const NOT_FOUND = 44;
/** Descriptor that the fake `path_open` hands out. */
const OPENED_DESCRIPTOR = 9;
/** Byte offsets of the request areas in test memory. */
const STAT_OFFSET = 64;
const USED_OFFSET = 128;
const TARGET_OFFSET = 256;
/** The module's view of a Windows host whose working directory is on drive C. */
const FILE_SYSTEM = wasiFileSystem("win32", absolute("C:\\work"));

type Call = readonly [name: string, ...args: unknown[]];

/** Returns link-following imports over fakes that record each call and report the given results. */
function linkFollowing(options: { readonly lstatType: number; readonly openErrno?: number; readonly linkTarget?: string }) {
  const memory = new WebAssembly.Memory({ initial: 1 });
  const view = () => new DataView(memory.buffer);
  const calls: Call[] = [];
  const imports = withNativeLinkFollowing(
    {
      path_open: (...args: unknown[]) => {
        calls.push(["path_open", ...args]);
        if (options.openErrno !== undefined) return options.openErrno;
        view().setUint32(args.at(-1) as number, OPENED_DESCRIPTOR, true);
        return 0;
      },
      path_filestat_get: (...args: unknown[]) => {
        calls.push(["path_filestat_get", ...args]);
        view().setUint8((args.at(-1) as number) + 16, options.lstatType);
        return 0;
      },
      fd_filestat_get: (fd: number, statPointer: number) => {
        calls.push(["fd_filestat_get", fd]);
        view().setUint8(statPointer + 16, DIRECTORY);
        return 0;
      },
      fd_close: (fd: number) => {
        calls.push(["fd_close", fd]);
        return 0;
      },
      // Like Node's WASI, the fake cuts a long target off at the end of the buffer.
      path_readlink: (_fd: number, _path: number, _length: number, buffer: number, bufferLength: number, used: number) => {
        const written = Buffer.from(options.linkTarget ?? "", "utf8").copy(
          Buffer.from(memory.buffer, buffer, bufferLength),
        );
        view().setUint32(used, written, true);
        return 0;
      },
    },
    FILE_SYSTEM,
    () => memory,
  );
  const filestat = (flags: number) =>
    (imports.path_filestat_get as (...args: number[]) => number)(3, flags, 0, 4, STAT_OFFSET);
  const readlink = (bufferLength = 64) => {
    const errno = (imports.path_readlink as (...args: number[]) => number)(3, 0, 4, TARGET_OFFSET, bufferLength, USED_OFFSET);
    const used = view().getUint32(USED_OFFSET, true);
    return { errno, target: Buffer.from(memory.buffer, TARGET_OFFSET, used).toString("utf8") };
  };
  return { calls, imports, filestat, readlink, fileType: () => view().getUint8(STAT_OFFSET + 16) };
}

describe("withNativeLinkFollowing", () => {
  describe("when the module opens a path and asks to follow links", () => {
    test("opens it without asking Node's WASI to follow, and passes the rest through", () => {
      const { calls, imports } = linkFollowing({ lstatType: DIRECTORY });

      const errno = (imports.path_open as (...args: unknown[]) => number)(3, FOLLOW | 2, 0, 4, 2, 1n, 2n, 0, 128);

      assert.equal(errno, 0);
      assert.deepEqual(calls, [["path_open", 3, 2, 0, 4, 2, 1n, 2n, 0, 128]]);
    });
  });

  describe("when the module stats a link and asks to follow it", () => {
    test("reports the status of the opened target and closes it", () => {
      const link = linkFollowing({ lstatType: SYMBOLIC_LINK });

      const errno = link.filestat(FOLLOW);

      assert.equal(errno, 0);
      assert.equal(link.fileType(), DIRECTORY);
      assert.deepEqual(link.calls, [
        ["path_filestat_get", 3, 0, 0, 4, STAT_OFFSET],
        ["path_open", 3, 0, 0, 4, 0, 1n << 21n, 0n, 0, STAT_OFFSET],
        ["fd_filestat_get", OPENED_DESCRIPTOR],
        ["fd_close", OPENED_DESCRIPTOR],
      ]);
    });

    describe("and the target is missing", () => {
      test("reports the open error", () => {
        const link = linkFollowing({ lstatType: SYMBOLIC_LINK, openErrno: NOT_FOUND });

        assert.equal(link.filestat(FOLLOW), NOT_FOUND);
      });
    });
  });

  describe("when the module stats a link without following it", () => {
    test("reports the link itself", () => {
      const link = linkFollowing({ lstatType: SYMBOLIC_LINK });

      assert.equal(link.filestat(0), 0);
      assert.equal(link.fileType(), SYMBOLIC_LINK);
      assert.equal(link.calls.length, 1);
    });
  });

  describe("when the module stats a path that is not a link", () => {
    test("reports its status without opening it", () => {
      const link = linkFollowing({ lstatType: DIRECTORY });

      assert.equal(link.filestat(FOLLOW), 0);
      assert.deepEqual(link.calls, [["path_filestat_get", 3, 0, 0, 4, STAT_OFFSET]]);
    });
  });

  describe("when the module reads a link", () => {
    describe("and its target is on a mounted volume", () => {
      test("reports the target as the module spells it", () => {
        const link = linkFollowing({ lstatType: SYMBOLIC_LINK, linkTarget: "C:\\work\\real" });

        assert.deepEqual(link.readlink(), { errno: 0, target: "/c/work/real" });
      });

      describe("and the target fills the buffer", () => {
        test("reports it unchanged, so the module reads it again with a larger buffer", () => {
          const link = linkFollowing({ lstatType: SYMBOLIC_LINK, linkTarget: "\\\\?\\C:\\work\\real" });

          assert.deepEqual(link.readlink(8), { errno: 0, target: "\\\\?\\C:\\w" });
        });
      });
    });

    describe("and its target is relative", () => {
      test("reports it with forward slashes", () => {
        const link = linkFollowing({ lstatType: SYMBOLIC_LINK, linkTarget: "..\\real" });

        assert.deepEqual(link.readlink(), { errno: 0, target: "../real" });
      });
    });

    describe("and its target is on an unmounted volume", () => {
      test("reports it unchanged", () => {
        const link = linkFollowing({ lstatType: SYMBOLIC_LINK, linkTarget: "D:\\other" });

        assert.deepEqual(link.readlink(), { errno: 0, target: "D:\\other" });
      });
    });
  });
});
