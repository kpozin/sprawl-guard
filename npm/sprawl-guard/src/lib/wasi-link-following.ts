/** WASI lookup flag that asks a call to follow a final symbolic link. */
const LOOKUP_SYMLINK_FOLLOW = 1;

/** Byte offset of the file type in a WASI `filestat`, after the device and inode. */
const FILESTAT_FILETYPE_OFFSET = 16;

/** WASI file type of a symbolic link. */
const FILETYPE_SYMBOLIC_LINK = 7;

/** WASI right to read a descriptor's file status, the only right the stat below needs. */
const RIGHT_FD_FILESTAT_GET = 1n << 21n;

const SUCCESS = 0;

/** Node's `path_open`, whose arguments after the lookup flags pass through unchanged. */
type PathOpen = (dirFd: number, dirFlags: number, ...rest: unknown[]) => number;
type PathFilestatGet = (
  dirFd: number,
  flags: number,
  pathPointer: number,
  pathLength: number,
  statPointer: number,
) => number;

/**
 * Lets the module follow symbolic links and junctions on Windows.
 *
 * When Node's WASI follows a link itself, it reads the target and treats it as a module path only
 * if it starts with `/`. Windows targets start with a drive letter, so every followed link looked
 * broken. These imports let Windows follow the final link instead, as it already does for every
 * link earlier in a path:
 *
 * - `path_open` opens the path without asking Node's WASI to follow it; Windows opens the target.
 * - `path_filestat_get` stats a link by opening its target and reading that descriptor's status.
 *
 * Both still resolve paths through Node's WASI, which keeps them inside the mounted volumes.
 */
export function withNativeLinkFollowing(
  wasiImport: WebAssembly.ModuleImports,
  memory: () => WebAssembly.Memory,
): WebAssembly.ModuleImports {
  const pathOpen = wasiImport.path_open as PathOpen;
  const pathFilestatGet = wasiImport.path_filestat_get as PathFilestatGet;
  const fdFilestatGet = wasiImport.fd_filestat_get as (fd: number, statPointer: number) => number;
  const fdClose = wasiImport.fd_close as (fd: number) => number;
  return {
    ...wasiImport,
    path_open: (dirFd: number, dirFlags: number, ...rest: unknown[]) =>
      pathOpen(dirFd, dirFlags & ~LOOKUP_SYMLINK_FOLLOW, ...rest),
    path_filestat_get: (dirFd: number, flags: number, pathPointer: number, pathLength: number, statPointer: number) => {
      const errno = pathFilestatGet(dirFd, flags & ~LOOKUP_SYMLINK_FOLLOW, pathPointer, pathLength, statPointer);
      const view = () => new DataView(memory().buffer);
      if (
        errno !== SUCCESS ||
        (flags & LOOKUP_SYMLINK_FOLLOW) === 0 ||
        view().getUint8((statPointer >>> 0) + FILESTAT_FILETYPE_OFFSET) !== FILETYPE_SYMBOLIC_LINK
      ) {
        return errno;
      }
      // The status buffer holds the new descriptor until the descriptor's status overwrites it.
      const opened = pathOpen(dirFd, 0, pathPointer, pathLength, 0, RIGHT_FD_FILESTAT_GET, 0n, 0, statPointer);
      if (opened !== SUCCESS) {
        return opened;
      }
      const fd = view().getUint32(statPointer >>> 0, true);
      const status = fdFilestatGet(fd, statPointer);
      fdClose(fd);
      return status;
    },
  };
}
