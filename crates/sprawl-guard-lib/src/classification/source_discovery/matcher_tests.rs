#![cfg(test)]

use super::discover_sources;
use crate::LanguageId;
use crate::test_support::{
    load_config, source_paths, write_ambiguous_rust_go_config, write_config, write_source_file,
};
use indoc::indoc;

mod discover_sources {
    use super::*;

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
