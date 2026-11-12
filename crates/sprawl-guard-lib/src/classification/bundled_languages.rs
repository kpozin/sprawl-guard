//! Adapter and tests for bundled `tokei` language-path metadata.

use std::path::Path;

use tokei::LanguageType;

use super::bundled_filenames::{BUNDLED_FILENAMES, BUNDLED_PATH_SUFFIXES};
use crate::LanguageId;

/// Returns the extensions canonically owned by a bundled language.
pub fn bundled_extensions(language: LanguageId) -> Vec<&'static str> {
    LanguageType::list()
        .iter()
        .find_map(|(candidate, extensions)| {
            (*candidate == language.as_tokei()).then_some(*extensions)
        })
        .expect("every bundled language must have registry metadata")
        .iter()
        .copied()
        .filter(|extension| {
            LanguageType::from_file_extension(extension) == Some(language.as_tokei())
        })
        .collect()
}

/// Returns the exact filenames registered for a bundled language.
pub fn bundled_filenames(language: LanguageId) -> &'static [&'static str] {
    BUNDLED_FILENAMES
        .iter()
        .find_map(|(candidate, filenames)| {
            (*candidate == language.as_tokei()).then_some(*filenames)
        })
        .unwrap_or_default()
}

/// A dot-prefixed ASCII suffix from the Tokei filename registry.
pub(super) struct FilenameSuffix(&'static str);

impl FilenameSuffix {
    /// Constructs a registry suffix, checking generated constants at compile time.
    pub(super) const fn new(suffix: &'static str) -> Self {
        assert!(
            suffix.len() > 1 && suffix.as_bytes()[0] == b'.' && suffix.is_ascii(),
            "expected a dot-prefixed ASCII filename suffix"
        );
        Self(suffix)
    }

    fn matches(&self, normalized_filename: &str) -> bool {
        normalized_filename.ends_with(self.0)
    }
}

/// A path owner and metadata view backed by the pinned `tokei` registry.
///
/// Extension classification asks `tokei` for ownership so collisions keep the
/// registry's canonical order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct BundledLanguage(LanguageType);

impl BundledLanguage {
    /// Resolves one path to its canonical bundled language owner.
    pub(super) fn from_path(path: &Path) -> Option<Self> {
        let filename = path.file_name()?.to_string_lossy();

        if let Some(language) = exact_filename_owner(&filename) {
            return Some(Self(language));
        }

        let normalized_filename = filename.to_ascii_lowercase();
        for (language, suffixes) in BUNDLED_PATH_SUFFIXES {
            if suffixes
                .iter()
                .any(|suffix| suffix.matches(&normalized_filename))
            {
                return Some(Self(*language));
            }
        }

        for (separator_index, _) in normalized_filename.match_indices('.') {
            if separator_index == 0 {
                continue;
            }

            let suffix = &normalized_filename[separator_index + 1..];
            if let Some(language) = LanguageType::from_file_extension(suffix) {
                return Some(Self(language));
            }
        }

        None
    }

    /// Returns the resolved `tokei` language identifier.
    pub(super) const fn as_tokei(self) -> LanguageType {
        self.0
    }

    /// Returns the extensions registered for the resolved language.
    #[cfg(test)]
    pub(super) fn extensions(self) -> Vec<&'static str> {
        bundled_extensions(LanguageId::from_tokei(self.0))
    }

    /// Returns the exact filenames registered for the resolved language.
    #[cfg(test)]
    pub(super) fn filenames(self) -> &'static [&'static str] {
        bundled_filenames(LanguageId::from_tokei(self.0))
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

#[test]
fn bundled_language_resolves_canonical_owners_for_ascii_case_insensitive_paths() {
    let cases = [
        ("fixture.RS", LanguageType::Rust),
        ("BUILD", LanguageType::Bazel),
        ("Dockerfile", LanguageType::Dockerfile),
        ("Justfile", LanguageType::Just),
        ("Makefile", LanguageType::Makefile),
        ("fixture.cs.st", LanguageType::Smalltalk),
        ("fixture.BLADE.PHP", LanguageType::Blade),
        ("fixture.xblade.php", LanguageType::Php),
        ("fixture.PHP", LanguageType::Php),
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

    assert_eq!(rust.extensions(), vec!["rs"]);
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
