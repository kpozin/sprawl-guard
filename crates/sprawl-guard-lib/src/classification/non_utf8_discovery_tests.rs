#![cfg(target_os = "linux")]

use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

use crate::classification::discover_sources_report;
use crate::config::UnreadablePathPolicy;
use crate::test_support::{load_config, write_rust_enabled_config};
use crate::{RelativePath, SprawlError};

mod when_scoped_discovery_encounters_a_non_utf8_repository_entry {
    use super::*;

    #[test]
    fn it_returns_the_typed_error() {
        let tempdir = tempfile::tempdir().unwrap();
        write_rust_enabled_config(tempdir.path());
        let src = tempdir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let invalid_name = OsString::from_vec(vec![0xff, b'.', b'r', b's']);
        let invalid_path = src.join(PathBuf::from(invalid_name));
        fs::write(&invalid_path, "fn production() {}\n").unwrap();
        let scope = RelativePath::new("src").unwrap();

        let error = discover_sources_report(
            tempdir.path(),
            &load_config(tempdir.path()),
            &[scope],
            Some(UnreadablePathPolicy::Fail),
            None,
        )
        .unwrap_err();

        assert!(matches!(error, SprawlError::NonUtf8Path { .. }));
    }
}
