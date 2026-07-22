use super::common::{assert_exit_code, exit_codes, sprawl_guard_no_root, stderr, stdout};

const EXPECTED_VERSION_OUTPUT: &str = concat!("sprawl-guard ", env!("CARGO_PKG_VERSION"), "\n");

mod when_the_long_version_flag_is_requested {
    use super::*;

    #[test]
    fn it_prints_the_cli_version() {
        let output = sprawl_guard_no_root().arg("--version").output().unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(stdout(&output), EXPECTED_VERSION_OUTPUT);
        assert_eq!(stderr(&output), "");
    }
}

mod when_the_short_version_flag_is_requested {
    use super::*;

    #[test]
    fn it_prints_the_cli_version() {
        let output = sprawl_guard_no_root().arg("-V").output().unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(stdout(&output), EXPECTED_VERSION_OUTPUT);
        assert_eq!(stderr(&output), "");
    }
}
