"""Checks or tightens a Git index snapshot without staging files (Python 3.11+)."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
from enum import StrEnum
from pathlib import Path

import tomllib

# Payload read size, independent of the total blob size.
BLOB_COPY_BUFFER_BYTES = 64 * 1024


class Action(StrEnum):
    """Whether to verify only or also propose a tightened ratchet."""

    CHECK = "check"
    PREPARE = "prepare"


class SnapshotError(Exception):
    """The staged tree cannot be scanned or published safely."""


def git(root: Path, *arguments: str) -> bytes:
    """Reads Git's index or objects without changing staged contents."""
    return subprocess.check_output(["git", "-C", str(root), *arguments])


def materialize(root: Path, tree: str, destination: Path) -> None:
    """Copies raw regular blobs, rejecting unsupported types and changed names."""
    entries = git(root, "ls-tree", "-rz", "--full-tree", tree).split(b"\0")
    with subprocess.Popen(
        ["git", "-C", str(root), "cat-file", "--batch"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
    ) as reader:
        assert reader.stdin is not None and reader.stdout is not None
        for entry in filter(None, entries):
            metadata, name = entry.split(b"\t", 1)
            mode, kind, oid = metadata.split()
            if mode not in (b"100644", b"100755") or kind != b"blob":
                raise SnapshotError(
                    f"Staged symlinks and submodules are unsupported: {os.fsdecode(name)!r}"
                )
            relative = Path(os.fsdecode(name))
            if relative.anchor or ".." in relative.parts:
                raise SnapshotError(f"Unsafe staged path: {os.fsdecode(name)!r}")
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            reader.stdin.write(oid + b"\n")
            reader.stdin.flush()
            header = reader.stdout.readline().split()
            if (
                len(header) != 3
                or header[:2] != [oid, b"blob"]
                or not header[2].isdigit()
            ):
                raise SnapshotError("Git could not read a staged blob.")
            remaining = int(header[2])
            # Exclusive creation also rejects names that alias on this filesystem.
            with target.open("xb") as output:
                while remaining:
                    chunk = reader.stdout.read(min(remaining, BLOB_COPY_BUFFER_BYTES))
                    if not chunk:
                        raise SnapshotError("Git returned an incomplete staged blob.")
                    output.write(chunk)
                    remaining -= len(chunk)
            if reader.stdout.read(1) != b"\n":
                raise SnapshotError("Git returned an incomplete staged blob.")
            parent = destination
            for part in target.relative_to(destination).parts:
                if part not in os.listdir(parent):
                    raise SnapshotError(
                        f"The filesystem cannot preserve staged path {os.fsdecode(name)!r}."
                    )
                parent = parent / part
        reader.stdin.close()
        if reader.wait() != 0:
            raise SnapshotError("Git could not materialize the staged tree.")


def run(action: Action, command: list[str]) -> int:
    """Verifies one immutable index tree and publishes only reviewed ratchet changes."""
    root = Path(
        os.fsdecode(
            subprocess.check_output(
                ["git", "rev-parse", "--show-toplevel"]
            ).removesuffix(b"\n")
        )
    ).resolve()
    tree = git(root, "write-tree").decode().strip()
    with tempfile.TemporaryDirectory(prefix="sprawl-guard-index-") as temporary:
        snapshot = Path(temporary)
        materialize(root, tree, snapshot)
        options = ["--root", str(snapshot), "--config", "sprawl-guard.toml"]
        subprocess.run(
            [*command, "config", "upgrade", "--check", *options], cwd=root, check=True
        )
        resolved = subprocess.check_output(
            [*command, "config", "resolved", *options], cwd=root
        )
        ratchet_path = Path(tomllib.loads(resolved.decode())["ratchet"]["file"])
        staged = snapshot / ratchet_path
        before = staged.read_bytes()
        if action is Action.PREPARE:
            subprocess.run(
                [*command, "tighten", "--write", *options], cwd=root, check=True
            )
        subprocess.run([*command, "check", *options], cwd=root, check=True)
        if git(root, "write-tree").decode().strip() != tree:
            raise SnapshotError("The index changed during verification; retry.")
        after = staged.read_bytes()
        if after == before:
            return 0
        target = root / ratchet_path
        if any(path.is_symlink() for path in [target, *target.parents]):
            raise SnapshotError("The worktree ratchet path must not contain symlinks.")
        if target.read_bytes() != before:
            raise SnapshotError(
                "Review or set aside unstaged ratchet edits before preparing."
            )
        with tempfile.NamedTemporaryFile(dir=target.parent, delete=False) as output:
            pending = Path(output.name)
            output.write(after)
        try:
            pending.chmod(target.stat().st_mode)
            pending.replace(target)
        finally:
            pending.unlink(missing_ok=True)
        print(
            "Review and stage the ratchet changes, then retry the commit.",
            file=sys.stderr,
        )
        return 1


def main() -> int:
    """Runs the selected action with an executable command prefix after `--`."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", type=Action, choices=list(Action))
    parser.add_argument("command", nargs=argparse.REMAINDER)
    arguments = parser.parse_args()
    command = arguments.command
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        parser.error("provide a Sprawl Guard command after --")
    try:
        return run(arguments.action, command)
    except subprocess.CalledProcessError as error:
        return error.returncode
    except (SnapshotError, OSError, tomllib.TOMLDecodeError) as error:
        print(error, file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
