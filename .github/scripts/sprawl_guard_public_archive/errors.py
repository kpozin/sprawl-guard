"""Typed failures for Cargo package archive validation."""


class PackageValidationError(Exception):
    """Raised when a crate archive violates the publication contract."""
