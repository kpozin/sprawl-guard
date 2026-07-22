"""Validation API for sprawl-guard Cargo package archives."""

from sprawl_guard_public_archive.errors import PackageValidationError
from sprawl_guard_public_archive.package_archive import (
    CLI_PACKAGE,
    LIBRARY_PACKAGE,
    package_archive_for,
    validate_package_archive,
    validate_package_directory,
)

__all__ = [
    "CLI_PACKAGE",
    "LIBRARY_PACKAGE",
    "PackageValidationError",
    "package_archive_for",
    "validate_package_archive",
    "validate_package_directory",
]
