//! Adapter and tests for bundled `tokei` language-path metadata.

use std::path::Path;

use tokei::LanguageType;

use super::bundled_filenames::BUNDLED_FILENAMES;

/// A path owner and metadata view backed by the pinned `tokei` registry.
///
/// Extension classification asks `tokei` for ownership so collisions keep the
/// registry's canonical order.
#[allow(
    dead_code,
    reason = "the adapter deliberately precedes the classification change that consumes it"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct BundledLanguage(LanguageType);

#[allow(
    dead_code,
    reason = "the adapter deliberately precedes the classification change that consumes it"
)]
impl BundledLanguage {
    /// Resolves one path to its canonical bundled language owner.
    pub(super) fn from_path(path: &Path) -> Option<Self> {
        let filename = path.file_name()?.to_string_lossy();

        if let Some(language) = exact_filename_owner(&filename) {
            return Some(Self(language));
        }

        let normalized_filename = filename.to_ascii_lowercase();
        for (separator_index, _) in normalized_filename.match_indices('.') {
            if separator_index == 0 {
                continue;
            }

            let suffix = &normalized_filename[separator_index + 1..];
            if is_bundled_extension(suffix) {
                return LanguageType::from_file_extension(suffix).map(Self);
            }
        }

        None
    }

    /// Returns the resolved `tokei` language identifier.
    pub(super) const fn as_tokei(self) -> LanguageType {
        self.0
    }

    /// Returns the extensions registered for the resolved language.
    pub(super) fn extensions(self) -> &'static [&'static str] {
        LanguageType::list()
            .iter()
            .find_map(|(language, extensions)| (*language == self.0).then_some(*extensions))
            .expect("every bundled language must have registry metadata")
    }

    /// Returns the exact filenames registered for the resolved language.
    pub(super) fn filenames(self) -> &'static [&'static str] {
        BUNDLED_FILENAMES
            .iter()
            .find_map(|(language, filenames)| (*language == self.0).then_some(*filenames))
            .unwrap_or_default()
    }
}

fn exact_filename_owner(filename: &str) -> Option<LanguageType> {
    BUNDLED_FILENAMES.iter().find_map(|(language, filenames)| {
        filenames
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(filename))
            .then_some(*language)
    })
}

fn is_bundled_extension(candidate: &str) -> bool {
    LanguageType::list()
        .iter()
        .any(|(_, extensions)| extensions.contains(&candidate))
}

#[test]
fn bundled_language_resolves_canonical_owners_for_ascii_case_insensitive_paths() {
    let cases = [
        ("fixture.RS", LanguageType::Rust),
        ("BUILD", LanguageType::Bazel),
        ("Dockerfile", LanguageType::Dockerfile),
        ("Justfile", LanguageType::Just),
        ("Makefile", LanguageType::Makefile),
        ("fixture.cs.st", LanguageType::Smalltalk),
        ("fixture.e", LanguageType::Phix),
    ];

    for (path, expected_language) in cases {
        let language = BundledLanguage::from_path(Path::new(path))
            .expect("the bundled registry should classify the fixture path");

        assert_eq!(language.as_tokei(), expected_language, "path {path:?}");
    }
}

#[test]
fn bundled_language_exposes_registry_path_metadata() {
    let rust = BundledLanguage::from_path(Path::new("fixture.RS"))
        .expect("the bundled registry should classify Rust paths");
    let bazel = BundledLanguage::from_path(Path::new("BUILD"))
        .expect("the bundled registry should classify Bazel filenames");

    assert_eq!(rust.extensions(), &["rs"]);
    assert!(rust.filenames().is_empty());
    assert!(
        bazel
            .filenames()
            .iter()
            .any(|filename| filename.eq_ignore_ascii_case("BUILD"))
    );
}

#[test]
fn bundled_language_path_metadata_matches_tokei_path_detection_after_ascii_case_normalization() {
    for (_, extensions) in LanguageType::list() {
        for extension in *extensions {
            let path = format!("fixture.{}", extension.to_ascii_uppercase());
            let canonical_owner =
                LanguageType::from_file_extension(&extension.to_ascii_lowercase());

            assert_eq!(
                BundledLanguage::from_path(Path::new(&path)).map(BundledLanguage::as_tokei),
                canonical_owner,
                "extension {extension:?} should resolve to its canonical owner",
            );
        }
    }

    for (language, filenames) in BUNDLED_FILENAMES {
        for filename in *filenames {
            let path = filename.to_ascii_uppercase();

            assert_eq!(
                BundledLanguage::from_path(Path::new(&path)).map(BundledLanguage::as_tokei),
                Some(*language),
                "filename {filename:?} should resolve to its canonical owner",
            );
        }
    }
}

#[test]
fn bundled_language_prefers_exact_filenames_to_extension_owners() {
    assert_eq!(
        BundledLanguage::from_path(Path::new("CMakeLists.txt")).map(BundledLanguage::as_tokei),
        Some(LanguageType::CMake),
    );
}

#[test]
fn bundled_language_does_not_classify_shebang_only_paths() {
    assert_eq!(BundledLanguage::from_path(Path::new("script")), None);
}
