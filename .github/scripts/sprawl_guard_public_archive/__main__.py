"""Command-line entry point for validating packaged sprawl-guard crates."""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Sequence

from sprawl_guard_public_archive import (
    PackageValidationError,
    validate_package_directory,
)


def main(arguments: Sequence[str] | None = None) -> int:
    """Validates both public crate archives in the supplied Cargo package directory."""
    resolved_arguments = tuple(sys.argv[1:] if arguments is None else arguments)
    if len(resolved_arguments) != 1:
        print(
            "usage: python -m sprawl_guard_public_archive PACKAGE_DIRECTORY",
            file=sys.stderr,
        )
        return 2
    try:
        validate_package_directory(Path(resolved_arguments[0]))
    except (OSError, PackageValidationError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
