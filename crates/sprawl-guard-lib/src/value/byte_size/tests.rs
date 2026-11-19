use super::*;

mod parse_byte_size {
    use super::*;

    mod when_the_size_is_well_formed {
        use super::*;

        #[test]
        fn it_reads_decimal_and_binary_units() {
            for (text, bytes) in [
                ("512 B", 512),
                ("2 kB", 2_000),
                ("10 MB", 10_000_000),
                ("3 GB", 3_000_000_000),
                ("1 TB", 1_000_000_000_000),
                ("2 KiB", 2_048),
                ("10 MiB", 10_485_760),
                ("3 GiB", 3_221_225_472),
                ("1 TiB", 1_099_511_627_776),
            ] {
                assert_eq!(parse_byte_size(text), Ok(bytes), "{text}");
            }
        }

        #[test]
        fn it_allows_the_unit_to_follow_the_number_directly() {
            assert_eq!(parse_byte_size("10MB"), Ok(10_000_000));
        }

        #[test]
        fn it_reads_a_fraction_that_comes_to_whole_bytes() {
            assert_eq!(parse_byte_size("1.5 GiB"), Ok(1_610_612_736));
            assert_eq!(parse_byte_size("0.25 kB"), Ok(250));
        }

        #[test]
        fn it_ignores_trailing_zeros_in_the_fraction() {
            assert_eq!(parse_byte_size(&format!("1.{} B", "0".repeat(40))), Ok(1));
        }

        #[test]
        fn it_reads_zero_and_leaves_rejecting_it_to_the_limit() {
            assert_eq!(parse_byte_size("0 MB"), Ok(0));
        }
    }

    mod when_the_size_is_malformed {
        use super::*;

        #[test]
        fn it_asks_for_a_number_and_a_unit() {
            for text in [
                "", "MB", "10", "10 ", " 10 MB", "10  MB", "10 MB ", "1. MB", ".5 MB", "1.2.3 MB",
                "-1 MB", "+1 MB", "1e3 B", "1_000 B", "1,000 B",
            ] {
                assert_eq!(
                    parse_byte_size(text),
                    Err(ByteSizeError::Malformed(text.to_owned())),
                    "{text:?}"
                );
            }
        }
    }

    mod when_the_unit_is_unknown {
        use super::*;

        #[test]
        fn it_names_the_accepted_units() {
            for (text, unit) in [
                ("10M", "M"),
                ("10 mb", "mb"),
                ("10 Mb", "Mb"),
                ("10 KB", "KB"),
                ("10 kiB", "kiB"),
                ("10 PB", "PB"),
                ("10 bytes", "bytes"),
            ] {
                assert_eq!(
                    parse_byte_size(text),
                    Err(ByteSizeError::UnknownUnit(unit.to_owned())),
                    "{text:?}"
                );
            }
        }
    }

    mod when_the_size_is_a_fraction_of_a_byte {
        use super::*;

        #[test]
        fn it_rejects_it() {
            for text in ["1.5 B", "1.0001 kB", "0.1 KiB", "1.000000000001 B"] {
                assert_eq!(
                    parse_byte_size(text),
                    Err(ByteSizeError::FractionalBytes(text.to_owned())),
                    "{text:?}"
                );
            }
        }
    }

    mod when_the_fraction_has_too_many_digits {
        use super::*;

        #[test]
        fn it_rejects_it_even_if_it_comes_to_whole_bytes() {
            // One byte more than 1 MiB.
            let text = "1.00000095367431640625 MiB";
            assert_eq!(
                parse_byte_size(text),
                Err(ByteSizeError::TooPrecise(text.to_owned()))
            );
        }
    }

    mod when_the_size_is_larger_than_a_toml_integer {
        use super::*;

        #[test]
        fn it_rejects_it() {
            for text in [
                "9223372036854775808 B",
                "8388608 TiB",
                "18446744073709551616 B",
                "99999999999999999999999999999999999999999 B",
            ] {
                assert_eq!(
                    parse_byte_size(text),
                    Err(ByteSizeError::TooLarge(text.to_owned())),
                    "{text:?}"
                );
            }
        }

        #[test]
        fn it_reads_the_largest_toml_integer() {
            assert_eq!(
                parse_byte_size("9223372036854775807 B"),
                Ok(i64::MAX.unsigned_abs())
            );
        }
    }
}

mod byte_size_pattern {
    use super::*;

    #[test]
    fn it_lists_every_unit() {
        assert_eq!(
            byte_size_pattern(),
            "^[0-9]+(\\.[0-9]+)? ?(B|kB|MB|GB|TB|KiB|MiB|GiB|TiB)$"
        );
    }
}
