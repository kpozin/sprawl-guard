"""Structured validation of Cargo's normalized crate archives."""

from __future__ import annotations

import tarfile
import tomllib
from hashlib import sha256
from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path, PurePosixPath
from typing import Any, cast

from sprawl_guard_public_archive.errors import PackageValidationError


class CargoPackageName(StrEnum):
    """Cargo package names produced by the public workspace."""

    CLI = "sprawl-guard"
    LIBRARY = "sprawl-guard-lib"


class CargoDependencyName(StrEnum):
    """Registry dependencies whose normalization is part of the package contract."""

    LIBRARY = "sprawl-guard-lib"
    TOKEI = "tokei"


@dataclass(frozen=True, order=True)
class CargoVersion:
    """A stable three-component Cargo package version."""

    major: int
    minor: int
    patch: int

    def __post_init__(self) -> None:
        if any(
            type(component) is not int
            for component in (self.major, self.minor, self.patch)
        ):
            raise TypeError("Cargo version components must be integers")
        if min(self.major, self.minor, self.patch) < 0:
            raise ValueError("Cargo version components must be non-negative")

    def exact_requirement(self) -> str:
        """Returns Cargo's exact version-requirement spelling."""
        return f"={self.major}.{self.minor}.{self.patch}"

    def render(self) -> str:
        """Returns the package-version spelling used in manifests and filenames."""
        return f"{self.major}.{self.minor}.{self.patch}"


class DefaultFeaturesExpectation(StrEnum):
    """Required normalized state of Cargo's `default-features` option."""

    ENABLED = "enabled"
    DISABLED = "disabled"

    def cargo_value(self) -> bool:
        """Returns the boolean Cargo manifest representation."""
        return self is DefaultFeaturesExpectation.ENABLED


@dataclass(frozen=True)
class FixedDependencyExpectation:
    """Contract for a registry dependency pinned independently of the package version."""

    name: CargoDependencyName
    version: CargoVersion
    default_features: DefaultFeaturesExpectation | None = None


@dataclass(frozen=True)
class PackageVersionDependencyExpectation:
    """Contract for a registry dependency pinned to the package's own version."""

    name: CargoDependencyName


DependencyExpectation = FixedDependencyExpectation | PackageVersionDependencyExpectation


@dataclass(frozen=True)
class PackageMetadata:
    """Publication metadata that must survive Cargo manifest normalization."""

    description: str
    documentation: str
    keywords: tuple[str, ...]
    categories: tuple[str, ...]


@dataclass(frozen=True)
class PackageSpec:
    """Expected identity, files, and normalized dependency of one crate archive."""

    name: CargoPackageName
    metadata: PackageMetadata
    dependency: DependencyExpectation
    allowed_content_directories: tuple[PurePosixPath, ...]
    required_extra_files: tuple[PurePosixPath, ...] = ()


LIBRARY_PACKAGE = PackageSpec(
    name=CargoPackageName.LIBRARY,
    metadata=PackageMetadata(
        description="Implementation library for the sprawl-guard CLI.",
        documentation="https://docs.rs/sprawl-guard-lib",
        keywords=("lint", "repository", "architecture", "tooling"),
        categories=("development-tools",),
    ),
    dependency=FixedDependencyExpectation(
        name=CargoDependencyName.TOKEI,
        version=CargoVersion(14, 0, 0),
        default_features=DefaultFeaturesExpectation.DISABLED,
    ),
    allowed_content_directories=(PurePosixPath("src"),),
    required_extra_files=(
        PurePosixPath("testdata/line_counts/markdown.md"),
        PurePosixPath("testdata/line_counts/rust_doc_comment.rs"),
    ),
)

CLI_PACKAGE = PackageSpec(
    name=CargoPackageName.CLI,
    metadata=PackageMetadata(
        description="Repository sprawl checker CLI.",
        documentation="https://docs.rs/sprawl-guard",
        keywords=("lint", "repository", "architecture", "ratchet"),
        categories=("command-line-utilities", "development-tools"),
    ),
    dependency=PackageVersionDependencyExpectation(
        name=CargoDependencyName.LIBRARY,
    ),
    allowed_content_directories=(
        PurePosixPath("src"),
        PurePosixPath("templates"),
        PurePosixPath("tests"),
    ),
    required_extra_files=(PurePosixPath("templates/machine-contract.ts.askama"),),
)

PACKAGE_SPECS = (LIBRARY_PACKAGE, CLI_PACKAGE)

REQUIRED_PACKAGE_FILES = (
    PurePosixPath("Cargo.lock"),
    PurePosixPath("Cargo.toml"),
    PurePosixPath("Cargo.toml.orig"),
    PurePosixPath("LICENSE-APACHE"),
    PurePosixPath("LICENSE-MIT"),
    PurePosixPath("README.md"),
)
OPTIONAL_CARGO_GENERATED_FILES = frozenset({PurePosixPath(".cargo_vcs_info.json")})

FORBIDDEN_BAZEL_FILES = frozenset(
    {
        "BUILD",
        "BUILD.bazel",
        ".bazelignore",
        ".bazelrc",
        ".bazelversion",
        "MODULE.bazel",
        "MODULE.bazel.lock",
        "REPO.bazel",
        "WORKSPACE",
        "WORKSPACE.bazel",
        "WORKSPACE.bzlmod",
    }
)
EXPECTED_LICENSE_DIGESTS = {
    PurePosixPath("LICENSE-APACHE"): bytes.fromhex(
        "e0ff874802b92bd4bc64c44d1f8c977ad312bcb28ca94475a04a84ca4b991547"
    ),
    PurePosixPath("LICENSE-MIT"): bytes.fromhex(
        "7370ef47ad0fbe3b74955afa4f488da39de14453f6a327f0f0f90917ae50f0a4"
    ),
}
_DEPENDENCY_TABLE_NAMES = ("dependencies", "dev-dependencies", "build-dependencies")
_FORBIDDEN_DEPENDENCY_LOCATION_KEYS = frozenset({"git", "path", "registry"})


def _toml_table(value: object, name: str) -> dict[str, Any]:
    if not isinstance(value, dict) or not all(isinstance(key, str) for key in value):
        raise PackageValidationError(f"normalized Cargo.toml has invalid {name} table")
    return cast(dict[str, Any], value)


def _archive_members(
    package: tarfile.TarFile,
) -> tuple[tuple[PurePosixPath, tarfile.TarInfo], ...]:
    members: list[tuple[PurePosixPath, tarfile.TarInfo]] = []
    seen: set[PurePosixPath] = set()
    for member in package.getmembers():
        path = PurePosixPath(member.name)
        if (
            not path.parts
            or path.is_absolute()
            or ".." in path.parts
            or path.as_posix() != member.name
        ):
            raise PackageValidationError(
                f"crate archive contains unsafe or noncanonical path: {member.name}"
            )
        if path in seen:
            raise PackageValidationError(
                f"crate archive contains duplicate path: {path}"
            )
        if not member.isfile() and not member.isdir():
            raise PackageValidationError(
                f"crate archive contains unsupported member type: {path}"
            )
        seen.add(path)
        members.append((path, member))
    return tuple(members)


def _package_prefix(paths: tuple[PurePosixPath, ...]) -> PurePosixPath:
    top_level_paths = {path.parts[0] for path in paths if path.parts}
    if len(top_level_paths) != 1:
        raise PackageValidationError(
            "crate archive must contain exactly one top-level directory"
        )
    return PurePosixPath(next(iter(top_level_paths)))


def _read_normalized_manifest(
    package: tarfile.TarFile,
    member: tarfile.TarInfo,
) -> dict[str, Any]:
    manifest_file = package.extractfile(member)
    if manifest_file is None:
        raise PackageValidationError("crate archive Cargo.toml is not a regular file")
    try:
        parsed = tomllib.loads(manifest_file.read().decode("utf-8"))
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise PackageValidationError(
            f"crate archive has invalid Cargo.toml: {error}"
        ) from error
    return _toml_table(parsed, "root")


def _validate_normalized_dependency(
    manifest: dict[str, Any], spec: PackageSpec, package_version: str
) -> None:
    dependencies = _toml_table(manifest.get("dependencies"), "dependencies")
    expectation = spec.dependency
    dependency = _toml_table(
        dependencies.get(expectation.name.value),
        f"dependency {expectation.name.value}",
    )
    match expectation:
        case FixedDependencyExpectation():
            exact_requirement = expectation.version.exact_requirement()
            expected: dict[str, object] = {"version": exact_requirement}
            if expectation.default_features is not None:
                expected["default-features"] = (
                    expectation.default_features.cargo_value()
                )
        case PackageVersionDependencyExpectation():
            exact_requirement = f"={package_version}"
            expected = {"version": exact_requirement}
    if dependency != expected:
        raise PackageValidationError(
            f"dependency {expectation.name.value} is not registry-normalized at "
            f"{exact_requirement}"
        )


def _dependency_tables(manifest: dict[str, Any]) -> tuple[dict[str, Any], ...]:
    tables = [
        _toml_table(manifest[name], name)
        for name in _DEPENDENCY_TABLE_NAMES
        if name in manifest
    ]
    if "target" in manifest:
        targets = _toml_table(manifest["target"], "target")
        for target_name, target_value in targets.items():
            target = _toml_table(target_value, f"target {target_name}")
            tables.extend(
                _toml_table(target[name], f"target {target_name} {name}")
                for name in _DEPENDENCY_TABLE_NAMES
                if name in target
            )
    return tuple(tables)


def _validate_dependency_locations(manifest: dict[str, Any]) -> None:
    for dependencies in _dependency_tables(manifest):
        for dependency_name, dependency_value in dependencies.items():
            if isinstance(dependency_value, str):
                continue
            dependency = _toml_table(
                dependency_value,
                f"dependency {dependency_name}",
            )
            if _FORBIDDEN_DEPENDENCY_LOCATION_KEYS & dependency.keys():
                raise PackageValidationError(
                    f"dependency {dependency_name} contains a non-registry location"
                )


def _validate_package_metadata(
    package_table: dict[str, Any], spec: PackageSpec
) -> None:
    expected = {
        "categories": list(spec.metadata.categories),
        "description": spec.metadata.description,
        "documentation": spec.metadata.documentation,
        "homepage": "https://github.com/kpozin/sprawl-guard",
        "keywords": list(spec.metadata.keywords),
        "license": "MIT OR Apache-2.0",
        "readme": "README.md",
        "repository": "https://github.com/kpozin/sprawl-guard",
    }
    mismatches = {
        key: package_table.get(key)
        for key, value in expected.items()
        if package_table.get(key) != value
    }
    if mismatches:
        raise PackageValidationError(
            f"crate archive has unexpected publication metadata: {mismatches}"
        )


def validate_package_archive(archive: Path, spec: PackageSpec) -> str:
    """Checks one `.crate` archive and returns its normalized package version."""
    try:
        with tarfile.open(archive, mode="r:gz") as package:
            members = _archive_members(package)
            paths = tuple(path for path, _ in members)
            prefix = _package_prefix(paths)
            relative_members = {
                PurePosixPath(*path.parts[1:]): member
                for path, member in members
                if len(path.parts) > 1
            }
            relative_paths = set(relative_members)
            required_paths = {*REQUIRED_PACKAGE_FILES, *spec.required_extra_files}
            allowed_root_files = required_paths | OPTIONAL_CARGO_GENERATED_FILES
            missing_paths = sorted(required_paths - relative_paths)
            if missing_paths:
                rendered = ", ".join(path.as_posix() for path in missing_paths)
                raise PackageValidationError(
                    f"crate archive is missing required files: {rendered}"
                )
            nonregular_root_paths = sorted(
                path
                for path in allowed_root_files & relative_paths
                if not relative_members[path].isfile()
            )
            if nonregular_root_paths:
                rendered = ", ".join(path.as_posix() for path in nonregular_root_paths)
                raise PackageValidationError(
                    f"crate archive root paths are not regular files: {rendered}"
                )

            forbidden_paths = sorted(
                path
                for path in relative_paths
                if path.name in FORBIDDEN_BAZEL_FILES
                or "__pycache__" in path.parts
                or path.suffix == ".pyc"
            )
            if forbidden_paths:
                rendered = ", ".join(path.as_posix() for path in forbidden_paths)
                raise PackageValidationError(
                    f"crate archive contains forbidden files: {rendered}"
                )

            unexpected_paths = sorted(
                path
                for path in relative_paths
                if path not in allowed_root_files
                and not any(
                    path.is_relative_to(directory)
                    for directory in spec.allowed_content_directories
                )
            )
            if unexpected_paths:
                rendered = ", ".join(path.as_posix() for path in unexpected_paths)
                raise PackageValidationError(
                    f"crate archive contains unexpected files: {rendered}"
                )

            manifest = _read_normalized_manifest(
                package,
                relative_members[PurePosixPath("Cargo.toml")],
            )
            package_table = _toml_table(manifest.get("package"), "package")
            package_name = package_table.get("name")
            package_version = package_table.get("version")
            if (
                package_name != spec.name.value
                or not isinstance(package_version, str)
                or not package_version
            ):
                raise PackageValidationError(
                    f"crate archive has unexpected package identity: {package_name} {package_version}"
                )
            expected_prefix = f"{spec.name.value}-{package_version}"
            expected_filename = f"{expected_prefix}.crate"
            if (
                prefix.as_posix() != expected_prefix
                or archive.name != expected_filename
            ):
                raise PackageValidationError(
                    f"crate archive path {archive.name} and prefix {prefix} do not match "
                    f"package identity {expected_prefix}"
                )
            _validate_package_metadata(package_table, spec)
            _validate_dependency_locations(manifest)
            _validate_normalized_dependency(manifest, spec, package_version)
            for license_path, expected_digest in EXPECTED_LICENSE_DIGESTS.items():
                license_file = package.extractfile(relative_members[license_path])
                if (
                    license_file is None
                    or sha256(license_file.read()).digest() != expected_digest
                ):
                    raise PackageValidationError(
                        f"crate archive contains unexpected license text: {license_path}"
                    )
            return package_version
    except PackageValidationError:
        raise
    except (OSError, tarfile.TarError) as error:
        raise PackageValidationError(
            f"cannot read crate archive {archive}: {error}"
        ) from error


def package_archive_for(package_directory: Path, spec: PackageSpec) -> Path:
    """Finds the single archive for `spec` without matching longer package-name prefixes."""
    archives = tuple(package_directory.glob(f"{spec.name.value}-[0-9]*.crate"))
    if len(archives) != 1:
        raise PackageValidationError(
            f"expected one archive for {spec.name.value}, found {len(archives)}"
        )
    return archives[0]


def validate_package_directory(package_directory: Path) -> None:
    """Validates the complete set of crate archives produced by the public workspace."""
    expected_archives = tuple(
        package_archive_for(package_directory, spec) for spec in PACKAGE_SPECS
    )
    unexpected_archives = sorted(
        set(package_directory.glob("*.crate")) - set(expected_archives)
    )
    if unexpected_archives:
        rendered = ", ".join(archive.name for archive in unexpected_archives)
        raise PackageValidationError(f"unexpected crate archives: {rendered}")
    package_versions = {
        validate_package_archive(archive, spec)
        for archive, spec in zip(expected_archives, PACKAGE_SPECS, strict=True)
    }
    if len(package_versions) != 1:
        rendered = ", ".join(sorted(package_versions))
        raise PackageValidationError(f"crate archive versions disagree: {rendered}")
