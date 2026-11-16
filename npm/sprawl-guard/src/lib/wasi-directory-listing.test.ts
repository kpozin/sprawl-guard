import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, describe, test } from "node:test";
import { unsafeAbsolutePathFromKnownAbsolute as absolute } from "./paths.js";
import { withDirectoryListing } from "./wasi-directory-listing.js";
import { wasiFileSystem } from "./wasi-invocation.js";

/** Descriptor that the fake `path_open` hands out. */
const OPENED_DESCRIPTOR = 7;
/** Byte offsets of the request areas in test memory. */
const PATH_OFFSET = 0;
const OPENED_OFFSET = 1_024;
const USED_OFFSET = 1_028;
const LISTING_OFFSET = 2_048;

type Entry = { readonly next: bigint; readonly type: number; readonly name: string };

describe("withDirectoryListing", () => {
  const workspace = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "directory-listing-")));
  after(() => fs.rmSync(workspace, { recursive: true, force: true }));
  fs.mkdirSync(path.join(workspace, "project", "src"), { recursive: true });
  fs.writeFileSync(path.join(workspace, "project", "lib.rs"), "");

  const fileSystem = wasiFileSystem(process.platform, absolute(workspace));
  const [mount = "/"] = fileSystem.preopens.keys();
  const memory = new WebAssembly.Memory({ initial: 1 });
  const closed: number[] = [];
  const imports = withDirectoryListing(
    {
      path_open: (...args: number[]) => {
        new DataView(memory.buffer).setUint32(args.at(-1) ?? 0, OPENED_DESCRIPTOR, true);
        return 0;
      },
      fd_close: (fd: number) => {
        closed.push(fd);
        return 0;
      },
      fd_renumber: () => 0,
      clock_time_get: () => 0,
    },
    fileSystem,
    () => memory,
  );

  /** Opens a host directory relative to the first mounted volume, as the module would. */
  function open(hostPath: string) {
    const guestPath = fileSystem.toGuest(absolute(hostPath)) ?? assert.fail(`${hostPath} is not mounted`);
    const bytes = Buffer.from(path.posix.relative(mount, guestPath), "utf8");
    new Uint8Array(memory.buffer).set(bytes, PATH_OFFSET);
    const pathOpen = imports.path_open as (...args: unknown[]) => number;
    return pathOpen(3, 0, PATH_OFFSET, bytes.length, 0, 0n, 0n, 0, OPENED_OFFSET);
  }

  /** Reads one page of a listing and decodes its complete entries. */
  function read(fd: number, cookie: bigint, length = 1_024) {
    const readdir = imports.fd_readdir as (...args: unknown[]) => number;
    const errno = readdir(fd, LISTING_OFFSET, length, cookie, USED_OFFSET);
    const view = new DataView(memory.buffer);
    const used = view.getUint32(USED_OFFSET, true);
    const entries: Entry[] = [];
    for (let offset = 0; offset + 24 <= used; ) {
      const start = LISTING_OFFSET + offset;
      const nameLength = view.getUint32(start + 16, true);
      if (offset + 24 + nameLength > used) break;
      entries.push({
        next: view.getBigUint64(start, true),
        type: view.getUint8(start + 20),
        name: Buffer.from(memory.buffer, start + 24, nameLength).toString("utf8"),
      });
      offset += 24 + nameLength;
    }
    return { errno, used, entries };
  }

  describe("when the module lists a directory it opened", () => {
    open(path.join(workspace, "project"));
    const listing = read(OPENED_DESCRIPTOR, 0n);

    test("it returns every entry with its type", () => {
      assert.equal(listing.errno, 0);
      assert.deepEqual(
        listing.entries.map(({ name, type }) => [name, type]).sort(),
        [
          ["lib.rs", 4],
          ["src", 3],
        ],
      );
    });

    test("it gives each entry the cookie of the entry after it", () => {
      assert.deepEqual(
        listing.entries.map(({ next }) => next),
        [1n, 2n],
      );
    });

    describe("and the buffer holds only part of the listing", () => {
      const first = read(OPENED_DESCRIPTOR, 0n, 30);

      test("it fills the buffer", () => {
        assert.equal(first.used, 30);
      });

      test("it continues from the cookie of the last complete entry", () => {
        const rest = read(OPENED_DESCRIPTOR, first.entries.at(-1)?.next ?? 0n);
        assert.deepEqual(
          [...first.entries, ...rest.entries].map(({ name }) => name),
          listing.entries.map(({ name }) => name),
        );
      });
    });
  });

  describe("when the module closes a directory", () => {
    test("it stops listing the closed descriptor and lets Node's WASI close it", () => {
      open(path.join(workspace, "project"));
      (imports.fd_close as (fd: number) => number)(OPENED_DESCRIPTOR);
      assert.equal(read(OPENED_DESCRIPTOR, 0n).errno, 8);
      assert.deepEqual(closed, [OPENED_DESCRIPTOR]);
    });
  });

  describe("when the module renumbers a directory", () => {
    test("it lists the directory under its new descriptor only", () => {
      open(path.join(workspace, "project"));
      (imports.fd_renumber as (from: number, to: number) => number)(OPENED_DESCRIPTOR, 9);
      assert.equal(read(9, 0n).entries.length, 2);
      assert.equal(read(OPENED_DESCRIPTOR, 0n).errno, 8);
    });
  });

  describe("when the opened path does not exist", () => {
    test("it reports that the directory is missing", () => {
      open(path.join(workspace, "missing"));
      assert.equal(read(OPENED_DESCRIPTOR, 0n).errno, 44);
    });
  });

  describe("when the module calls another function", () => {
    test("it passes the call to Node's WASI", () => {
      assert.equal(typeof imports.clock_time_get, "function");
    });
  });
});
