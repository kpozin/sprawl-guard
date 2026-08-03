#![cfg(test)]

use super::discover_sources;
use crate::test_support::{
    load_config, source_paths, write_ambiguous_rust_go_config, write_config, write_source_file,
};
use crate::{LanguageGroupId, LanguageId};
use indoc::indoc;
use tokei::LanguageType;

mod discover_sources {
    use super::*;

    fn path_languages(
        sources: &[crate::classification::ClassifiedSource],
    ) -> Vec<(&str, LanguageId)> {
        sources
            .iter()
            .map(|source| (source.path.as_str(), source.language()))
            .collect()
    }

    mod when_paths_match_the_bundled_registry {
        use super::*;

        #[test]
        fn it_classifies_extensions_and_exact_filenames_case_insensitively() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(tempdir.path(), "");
            for path in [
                "fixture.RS",
                "BUILD",
                "Dockerfile",
                "Justfile",
                "Makefile",
                "fixture.cs.st",
                "fixture.e",
            ] {
                write_source_file(tempdir.path(), path);
            }

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(
                path_languages(&sources),
                vec![
                    ("BUILD", LanguageId::from_tokei(LanguageType::Bazel)),
                    (
                        "Dockerfile",
                        LanguageId::from_tokei(LanguageType::Dockerfile),
                    ),
                    ("Justfile", LanguageId::from_tokei(LanguageType::Just)),
                    ("Makefile", LanguageId::from_tokei(LanguageType::Makefile)),
                    ("fixture.RS", LanguageId::from_tokei(LanguageType::Rust)),
                    (
                        "fixture.cs.st",
                        LanguageId::from_tokei(LanguageType::Smalltalk)
                    ),
                    ("fixture.e", LanguageId::from_tokei(LanguageType::Phix)),
                ],
            );
        }

        #[test]
        fn it_does_not_classify_a_shebang_only_path() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(tempdir.path(), "");
            write_source_file(tempdir.path(), "script");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert!(sources.is_empty());
        }
    }

    mod when_language_paths_are_authored {
        use super::*;

        #[test]
        fn additional_globs_preserve_bundled_paths() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [languages.rust]
                    additional_globs = ["generated/**/*.rust"]
                "#},
            );
            write_source_file(tempdir.path(), "src/lib.rs");
            write_source_file(tempdir.path(), "generated/src/lib.rust");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(
                source_paths(&sources),
                vec!["generated/src/lib.rust", "src/lib.rs"]
            );
        }

        #[test]
        fn user_globs_remain_case_sensitive() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [languages.rust]
                    include = []
                    additional_globs = ["**/*.rust"]
                "#},
            );
            write_source_file(tempdir.path(), "lower.rust");
            write_source_file(tempdir.path(), "upper.RUST");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["lower.rust"]);
        }

        #[test]
        fn an_extension_can_be_reassigned_from_its_canonical_owner() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [languages.phix]
                    exclude = ["**/*.[eE]"]

                    [languages."specman e"]
                    additional_globs = ["**/*.[eE]"]
                "#},
            );
            write_source_file(tempdir.path(), "lower.e");
            write_source_file(tempdir.path(), "upper.E");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["lower.e", "upper.E"]);
            assert!(sources.iter().all(|source| {
                source.language() == LanguageId::from_tokei(LanguageType::SpecmanE)
            }));
        }

        #[test]
        fn a_custom_extension_uses_its_languages_resolved_custom_group() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [languages.markdown]
                    additional_globs = ["**/*.guide"]

                    [language_groups.content]
                    languages = ["markdown"]
                "#},
            );
            write_source_file(tempdir.path(), "docs/start.guide");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(sources.len(), 1);
            assert_eq!(
                sources[0].group(),
                &LanguageGroupId::custom("content").unwrap()
            );
        }
    }

    mod when_language_include_and_exclude_are_overridden {
        use super::*;

        #[test]
        fn it_uses_the_user_patterns_instead_of_defaults() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [languages.rust]
                    enabled = true
                    include = ["custom/**"]
                    exclude = ["custom/excluded/**"]
                "#},
            );
            write_source_file(tempdir.path(), "src/lib.rs");
            write_source_file(tempdir.path(), "custom/included/file");
            write_source_file(tempdir.path(), "custom/excluded/file");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["custom/included/file"]);
        }
    }

    mod when_multiple_languages_match {
        use super::*;

        #[test]
        fn it_reports_an_ambiguous_language_error() {
            let tempdir = tempfile::tempdir().unwrap();
            write_ambiguous_rust_go_config(tempdir.path(), "src/shared");
            write_source_file(tempdir.path(), "src/shared");

            let error = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap_err();

            assert!(error.to_string().contains("matched multiple languages"));
            assert!(error.to_string().contains("Rust"));
            assert!(error.to_string().contains("Go"));
        }

        #[test]
        fn it_applies_language_excludes_before_reporting_ambiguity() {
            let tempdir = tempfile::tempdir().unwrap();
            write_config(
                tempdir.path(),
                indoc! {r#"
                    [languages.rust]
                    enabled = true
                    include = ["src/shared"]
                    exclude = ["src/shared"]

                    [languages.go]
                    enabled = true
                    include = ["src/shared"]
                "#},
            );
            write_source_file(tempdir.path(), "src/shared");

            let sources = discover_sources(tempdir.path(), &load_config(tempdir.path())).unwrap();

            assert_eq!(source_paths(&sources), vec!["src/shared"]);
            assert_eq!(sources[0].language(), LanguageId::new("go").unwrap());
        }
    }
}
