import path from "node:path";
import {
  type AbsolutePath,
  type GuestPath,
  resolveAbsolutePath,
  unsafeAbsolutePathFromKnownAbsolute,
  unsafeGuestPath,
} from "./paths.js";

/**
 * How the WASI module sees the host file system.
 *
 * On POSIX hosts the module sees `/` at `/`, so every host path keeps its spelling. Windows paths
 * have no WASI spelling, so each Windows volume the run needs appears under its own module path:
 * `C:\work` becomes `/c/work`, and `\\server\share\work` becomes `/unc/server/share/work`.
 */
export type WasiFileSystem = {
  /** Host directories that the module sees, keyed by the module path of each. */
  readonly preopens: ReadonlyMap<GuestPath, AbsolutePath>;
  /** Spells a host path for the module, or returns `undefined` if it is not visible. */
  readonly toGuest: (hostPath: AbsolutePath) => GuestPath | undefined;
  /** Spells a module path for the host, or returns `undefined` if it is outside every preopen. */
  readonly toHost: (guestPath: GuestPath) => AbsolutePath | undefined;
};

/** Direct WASI invocation for one wrapper run. */
export type WasiInvocationPlan = {
  readonly args: string[];
  readonly env: { readonly PWD: GuestPath };
  readonly fileSystem: WasiFileSystem;
};

/** Start of a Windows path that names a drive, such as `C:\dir` or the drive-relative `C:dir`. */
const DRIVE_PREFIX = /^[A-Za-z]:/u;

/**
 * Prefix of an extended-length Windows path: `\\?\C:\dir`, or `\\?\UNC\server\share\dir` for a
 * share. The group captures `UNC`.
 */
const EXTENDED_PREFIX = /^\\\\\?\\(?:(UNC)\\)?/iu;

/** Spells an extended-length path in its usual form, which names the same file. */
function withoutExtendedPrefix(value: string): string {
  return value.replace(EXTENDED_PREFIX, (_prefix, unc: string | undefined) => (unc === undefined ? "" : "\\\\"));
}

/** The POSIX view, where host and module paths are spelled alike. */
const POSIX_FILE_SYSTEM: WasiFileSystem = {
  preopens: new Map([[unsafeGuestPath("/"), unsafeAbsolutePathFromKnownAbsolute("/")]]),
  toGuest: unsafeGuestPath,
  toHost: unsafeAbsolutePathFromKnownAbsolute,
};

/**
 * Returns the file-system view for a host platform.
 *
 * On Windows the module sees the volumes of `cwd` and of each path in `paths`.
 */
export function wasiFileSystem(
  platform: NodeJS.Platform = process.platform,
  cwd: AbsolutePath = resolveAbsolutePath(process.cwd()),
  paths: readonly AbsolutePath[] = [],
): WasiFileSystem {
  if (platform !== "win32") {
    return POSIX_FILE_SYSTEM;
  }
  const mounts = new Map(
    [cwd, ...paths].map((hostPath) => {
      const volume = windowsVolume(hostPath);
      return [volume.key, volume] as const;
    }),
  );
  const preopens = new Map(
    [...mounts.values()].map(({ guest, root }) => [guest, unsafeAbsolutePathFromKnownAbsolute(root)]),
  );
  return {
    preopens,
    toGuest: (hostPath) => {
      const volume = windowsVolume(hostPath);
      if (!mounts.has(volume.key)) {
        return undefined;
      }
      const rest = hostPath.slice(volume.root.length).replaceAll("\\", "/");
      return unsafeGuestPath(rest === "" ? volume.guest : `${volume.guest}/${rest}`);
    },
    toHost: (guestPath) => {
      for (const [guest, root] of preopens) {
        if (guestPath === guest || guestPath.startsWith(`${guest}/`)) {
          const rest = guestPath.slice(guest.length).split("/");
          return unsafeAbsolutePathFromKnownAbsolute(path.win32.join(root, ...rest));
        }
      }
      return undefined;
    },
  };
}

/**
 * A Windows volume: its root as the host spells it, a case-insensitive key, and its module path.
 *
 * Windows compares drive letters and share names without case, so the module path is lowercase and
 * every spelling of a volume shares one mount.
 */
function windowsVolume(hostPath: AbsolutePath) {
  const root = path.win32.parse(hostPath).root;
  const components = root
    .toLowerCase()
    .split(/[\\/]/u)
    .filter((component) => component !== "");
  const drive = /^([a-z]):$/u.exec(components[0] ?? "")?.[1];
  return {
    root,
    key: root.toLowerCase().replaceAll("/", "\\"),
    guest: unsafeGuestPath(drive === undefined ? `/unc/${components.join("/")}` : `/${drive}`),
  };
}

/**
 * Starts the module in the host working directory with arguments it can resolve.
 *
 * POSIX arguments pass through unchanged. On Windows, an absolute path in an argument or in an
 * `--option=value` value is respelled for the module, and backslashes become forward slashes.
 */
export function buildWasiInvocation(
  userArgv: readonly string[],
  platform: NodeJS.Platform = process.platform,
  cwd: AbsolutePath = resolveAbsolutePath(process.cwd()),
): WasiInvocationPlan {
  if (platform !== "win32") {
    return {
      args: ["sprawl-guard", ...userArgv],
      env: { PWD: unsafeGuestPath(cwd) },
      fileSystem: POSIX_FILE_SYSTEM,
    };
  }
  // A value is a host path when it is absolute or names a drive. As natively, `\dir` is on the
  // working directory's drive, and `D:dir` is under the current directory of drive D.
  const split = userArgv.map((argument) => {
    const separator = argument.startsWith("--") ? argument.indexOf("=") : -1;
    const [prefix, value] =
      separator < 0 ? ["", argument] : [argument.slice(0, separator + 1), argument.slice(separator + 1)];
    const hostValue = withoutExtendedPrefix(value);
    return {
      prefix,
      value,
      hostPath:
        path.win32.isAbsolute(hostValue) || DRIVE_PREFIX.test(hostValue)
          ? unsafeAbsolutePathFromKnownAbsolute(path.win32.resolve(cwd, hostValue))
          : undefined,
    };
  });
  const fileSystem = wasiFileSystem(
    platform,
    cwd,
    split.flatMap(({ hostPath }) => (hostPath === undefined ? [] : [hostPath])),
  );
  return {
    args: [
      "sprawl-guard",
      ...split.map(
        ({ prefix, value, hostPath }) =>
          `${prefix}${(hostPath && fileSystem.toGuest(hostPath)) ?? value.replaceAll("\\", "/")}`,
      ),
    ],
    // The preopens always contain the working directory's volume.
    env: { PWD: fileSystem.toGuest(cwd) ?? unsafeGuestPath("/") },
    fileSystem,
  };
}
