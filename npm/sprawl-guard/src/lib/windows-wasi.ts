import fs from "node:fs";
import path from "node:path";
import { type GuestPath, unsafeAbsolutePathFromKnownAbsolute, unsafeGuestPath } from "./paths.js";
import { type WasiFileSystem, withoutExtendedPrefix } from "./wasi-invocation.js";

/** Descriptor that Node's WASI gives the first preopened directory; the rest follow in order. */
const FIRST_PREOPEN_DESCRIPTOR = 3;

/** Bytes in a WASI `dirent` header: next cookie, inode, name length, file type, padding. */
const DIRENT_HEADER_BYTES = 24;

/** WASI error numbers that a directory listing can report. */
enum Errno {
  Success = 0,
  Access = 2,
  BadDescriptor = 8,
  Io = 29,
  NotFound = 44,
  NotDirectory = 54,
}

/** WASI file types, as stored in a `dirent`. */
enum FileType {
  Unknown = 0,
  BlockDevice = 1,
  CharacterDevice = 2,
  Directory = 3,
  RegularFile = 4,
  SocketStream = 6,
  SymbolicLink = 7,
}

/** WASI lookup flag that asks a call to follow a final symbolic link. */
const LOOKUP_SYMLINK_FOLLOW = 1;

/** Byte offset of the file type in a WASI `filestat`, after the device and inode. */
const FILESTAT_FILETYPE_OFFSET = 16;

/** WASI right to read a descriptor's file status, the only right a followed stat needs. */
const RIGHT_FD_FILESTAT_GET = 1n << 21n;

/** One open descriptor's module path, and its listing while the module reads it. */
type OpenDescriptor = { readonly guestPath: GuestPath; entries?: readonly fs.Dirent[] };

type PathOpen = (dirFd: number, dirFlags: number, pathPointer: number, pathLength: number, ...rest: unknown[]) => number;
type PathFilestatGet = (
  dirFd: number,
  flags: number,
  pathPointer: number,
  pathLength: number,
  statPointer: number,
) => number;
type PathReadlink = (
  dirFd: number,
  pathPointer: number,
  pathLength: number,
  bufferPointer: number,
  bufferLength: number,
  usedPointer: number,
) => number;

/**
 * Adds directory listing to Node's WASI imports on Windows.
 *
 * Node's WASI cannot list directories on Windows, so the module could not scan a repository there.
 * These imports record the module path of every descriptor the module opens and list a directory
 * with `fs.readdirSync`. Every other call goes to Node's WASI unchanged.
 */
export function withDirectoryListing(
  wasiImport: WebAssembly.ModuleImports,
  fileSystem: WasiFileSystem,
  memory: () => WebAssembly.Memory,
): WebAssembly.ModuleImports {
  const descriptors = new Map<number, OpenDescriptor>(
    [...fileSystem.preopens.keys()].map((guestPath, index) => [FIRST_PREOPEN_DESCRIPTOR + index, { guestPath }]),
  );
  const pathOpen = wasiImport.path_open as PathOpen;
  const fdClose = wasiImport.fd_close as (fd: number) => number;
  const fdRenumber = wasiImport.fd_renumber as (from: number, to: number) => number;
  return {
    ...wasiImport,
    path_open: (dirFd: number, dirFlags: number, pathPointer: number, pathLength: number, ...rest: unknown[]) => {
      const errno = pathOpen(dirFd, dirFlags, pathPointer, pathLength, ...rest);
      const parent = descriptors.get(dirFd);
      if (errno === Errno.Success && parent !== undefined) {
        const buffer = memory().buffer;
        const relative = Buffer.from(buffer, pathPointer >>> 0, pathLength >>> 0).toString("utf8");
        const openedPointer = (rest.at(-1) as number) >>> 0;
        descriptors.set(new DataView(buffer).getUint32(openedPointer, true), {
          guestPath: unsafeGuestPath(path.posix.join(parent.guestPath, relative)),
        });
      }
      return errno;
    },
    fd_close: (fd: number) => {
      descriptors.delete(fd);
      return fdClose(fd);
    },
    fd_renumber: (from: number, to: number) => {
      const errno = fdRenumber(from, to);
      const descriptor = descriptors.get(from);
      if (errno === Errno.Success) {
        descriptors.delete(from);
        if (descriptor === undefined) descriptors.delete(to);
        else descriptors.set(to, descriptor);
      }
      return errno;
    },
    fd_readdir: (fd: number, bufferPointer: number, bufferLength: number, cookie: bigint, usedPointer: number) => {
      const descriptor = descriptors.get(fd);
      if (descriptor === undefined) {
        return Errno.BadDescriptor;
      }
      if (cookie === 0n || descriptor.entries === undefined) {
        const hostPath = fileSystem.toHost(descriptor.guestPath);
        if (hostPath === undefined) {
          return Errno.NotFound;
        }
        try {
          descriptor.entries = fs.readdirSync(hostPath, { withFileTypes: true });
        } catch (error) {
          return errnoOf(error);
        }
      }
      // The last record may be cut off; the module then reads again from that record's cookie.
      const target = Buffer.from(memory().buffer, bufferPointer >>> 0, bufferLength >>> 0);
      let used = 0;
      for (let index = Number(cookie); index < descriptor.entries.length && used < target.length; index++) {
        const entry = descriptor.entries[index] as fs.Dirent;
        used += direntRecord(BigInt(index) + 1n, entry).copy(target, used);
      }
      new DataView(memory().buffer).setUint32(usedPointer >>> 0, used, true);
      return Errno.Success;
    },
  };
}

/**
 * Lets the module follow symbolic links and junctions on Windows.
 *
 * Node's WASI reports link targets as Windows spells them, and when it follows a link itself, it
 * treats the target as a module path only if it starts with `/`. Targets that start with a drive
 * letter therefore looked broken. These imports let Windows follow the final link instead, as it
 * already does for every link earlier in a path, and respell the targets the module reads:
 *
 * - `path_open` opens the path without asking Node's WASI to follow it; Windows opens the target.
 * - `path_filestat_get` stats a link by opening its target and reading that descriptor's status.
 * - `path_readlink` reports a target on a mounted volume as the module spells it, so resolving a
 *   path through a link, as `std::fs::canonicalize` does, reaches the target.
 *
 * Node's WASI resolves each path as the module spells it; Windows then follows the links in it, as
 * a native build does.
 */
export function withNativeLinkFollowing(
  wasiImport: WebAssembly.ModuleImports,
  fileSystem: WasiFileSystem,
  memory: () => WebAssembly.Memory,
): WebAssembly.ModuleImports {
  const pathOpen = wasiImport.path_open as PathOpen;
  const pathFilestatGet = wasiImport.path_filestat_get as PathFilestatGet;
  const pathReadlink = wasiImport.path_readlink as PathReadlink;
  const fdFilestatGet = wasiImport.fd_filestat_get as (fd: number, statPointer: number) => number;
  const fdClose = wasiImport.fd_close as (fd: number) => number;
  return {
    ...wasiImport,
    path_readlink: (
      dirFd: number,
      pathPointer: number,
      pathLength: number,
      bufferPointer: number,
      bufferLength: number,
      usedPointer: number,
    ) => {
      const errno = pathReadlink(dirFd, pathPointer, pathLength, bufferPointer, bufferLength, usedPointer);
      if (errno !== Errno.Success) {
        return errno;
      }
      const view = new DataView(memory().buffer);
      const used = view.getUint32(usedPointer >>> 0, true);
      const target = withoutExtendedPrefix(
        Buffer.from(memory().buffer, bufferPointer >>> 0, used).toString("utf8"),
      );
      const guestTarget = path.win32.isAbsolute(target)
        ? fileSystem.toGuest(unsafeAbsolutePathFromKnownAbsolute(target))
        : undefined;
      if (guestTarget !== undefined) {
        // Like `readlink`, a target longer than the buffer is cut off.
        const written = Buffer.from(guestTarget, "utf8").copy(
          Buffer.from(memory().buffer, bufferPointer >>> 0, bufferLength >>> 0),
        );
        view.setUint32(usedPointer >>> 0, written, true);
      }
      return Errno.Success;
    },
    path_open: (dirFd: number, dirFlags: number, pathPointer: number, pathLength: number, ...rest: unknown[]) =>
      pathOpen(dirFd, dirFlags & ~LOOKUP_SYMLINK_FOLLOW, pathPointer, pathLength, ...rest),
    path_filestat_get: (dirFd: number, flags: number, pathPointer: number, pathLength: number, statPointer: number) => {
      const errno = pathFilestatGet(dirFd, flags & ~LOOKUP_SYMLINK_FOLLOW, pathPointer, pathLength, statPointer);
      const view = () => new DataView(memory().buffer);
      if (
        errno !== Errno.Success ||
        (flags & LOOKUP_SYMLINK_FOLLOW) === 0 ||
        view().getUint8((statPointer >>> 0) + FILESTAT_FILETYPE_OFFSET) !== FileType.SymbolicLink
      ) {
        return errno;
      }
      // The status buffer holds the new descriptor until the descriptor's status overwrites it.
      const opened = pathOpen(dirFd, 0, pathPointer, pathLength, 0, RIGHT_FD_FILESTAT_GET, 0n, 0, statPointer);
      if (opened !== Errno.Success) {
        return opened;
      }
      const fd = view().getUint32(statPointer >>> 0, true);
      const status = fdFilestatGet(fd, statPointer);
      fdClose(fd);
      return status;
    },
  };
}

/**
 * Encodes one `dirent` header and name.
 *
 * The inode is 0 because `fs.readdirSync` does not report one. wasi-libc then reads each entry's
 * inode and type with a separate file-status call, as it does for any WASI host without inodes.
 */
function direntRecord(next: bigint, entry: fs.Dirent): Buffer {
  const name = Buffer.from(entry.name, "utf8");
  const record = Buffer.alloc(DIRENT_HEADER_BYTES + name.length);
  record.writeBigUInt64LE(next, 0);
  record.writeUInt32LE(name.length, 16);
  record.writeUInt8(fileTypeOf(entry), 20);
  name.copy(record, DIRENT_HEADER_BYTES);
  return record;
}

function fileTypeOf(entry: fs.Dirent): FileType {
  if (entry.isDirectory()) return FileType.Directory;
  if (entry.isFile()) return FileType.RegularFile;
  if (entry.isSymbolicLink()) return FileType.SymbolicLink;
  if (entry.isBlockDevice()) return FileType.BlockDevice;
  if (entry.isCharacterDevice()) return FileType.CharacterDevice;
  if (entry.isSocket()) return FileType.SocketStream;
  return FileType.Unknown;
}

function errnoOf(error: unknown): Errno {
  switch ((error as NodeJS.ErrnoException).code) {
    case "ENOENT":
      return Errno.NotFound;
    case "ENOTDIR":
      return Errno.NotDirectory;
    case "EACCES":
    case "EPERM":
      return Errno.Access;
    default:
      return Errno.Io;
  }
}
