use super::*;

const OID: &str = "oid sha256:4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393\n";

/// Builds a pointer from its lines after the version line.
fn pointer(lines: &str) -> String {
    format!("{}{lines}", VERSION_LINES[0])
}

fn declared(content: &str) -> Option<u64> {
    declared_size(content.as_bytes()).map(FileBytes::into_inner)
}

mod when_the_content_is_a_valid_pointer {
    use super::*;

    #[test]
    fn it_returns_the_declared_size() {
        assert_eq!(
            declared(&pointer(&format!("{OID}size 12345\n"))),
            Some(12345)
        );
    }

    #[test]
    fn it_accepts_extension_lines_sorted_before_the_oid() {
        let extension =
            "ext-0-foo sha256:0000000000000000000000000000000000000000000000000000000000000000\n";
        assert_eq!(
            declared(&pointer(&format!("{extension}{OID}size 7\n"))),
            Some(7)
        );
    }

    #[test]
    fn it_accepts_the_legacy_version_url() {
        let content = format!("version https://hawser.github.com/spec/v1\n{OID}size 7\n");
        assert_eq!(declared(&content), Some(7));
    }

    #[test]
    fn it_accepts_an_unknown_key_in_order() {
        assert_eq!(
            declared(&pointer(&format!("{OID}size 7\nx-custom value\n"))),
            Some(7)
        );
    }
}

mod when_the_content_is_not_a_valid_pointer {
    use super::*;

    #[test]
    fn it_rejects_an_empty_file() {
        assert_eq!(declared(""), None);
    }

    #[test]
    fn it_rejects_a_missing_final_newline() {
        assert_eq!(declared(&pointer(&format!("{OID}size 7"))), None);
    }

    #[test]
    fn it_rejects_windows_line_endings() {
        assert_eq!(declared(&pointer(&format!("{OID}size 7\r\n"))), None);
    }

    #[test]
    fn it_rejects_keys_out_of_order() {
        assert_eq!(declared(&pointer(&format!("size 7\n{OID}"))), None);
    }

    #[test]
    fn it_rejects_a_repeated_key() {
        assert_eq!(declared(&pointer(&format!("{OID}size 7\nsize 8\n"))), None);
    }

    #[test]
    fn it_rejects_a_second_version_line() {
        assert_eq!(
            declared(&pointer(&format!("{OID}size 7\nversion 2\n"))),
            None
        );
    }

    #[test]
    fn it_rejects_a_missing_oid() {
        assert_eq!(declared(&pointer("size 7\n")), None);
    }

    #[test]
    fn it_rejects_a_missing_size() {
        assert_eq!(declared(&pointer(OID)), None);
    }

    #[test]
    fn it_rejects_an_uppercase_oid() {
        let oid = OID.to_uppercase().replacen("OID SHA256", "oid sha256", 1);
        assert_eq!(declared(&pointer(&format!("{oid}size 7\n"))), None);
    }

    #[test]
    fn it_rejects_a_short_oid() {
        assert_eq!(declared(&pointer("oid sha256:abc\nsize 7\n")), None);
    }

    #[test]
    fn it_rejects_a_signed_size() {
        assert_eq!(declared(&pointer(&format!("{OID}size +7\n"))), None);
    }

    #[test]
    fn it_rejects_an_uppercase_key() {
        assert_eq!(declared(&pointer(&format!("{OID}size 7\nX y\n"))), None);
    }

    #[test]
    fn it_rejects_content_that_is_not_utf8() {
        let mut content = pointer(&format!("{OID}size 7\n")).into_bytes();
        content.push(0xff);
        assert_eq!(declared_size(&content), None);
    }
}
