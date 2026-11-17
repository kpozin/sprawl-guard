use serde_json::{Value, json};

use super::common::{assert_exit_code, exit_codes, json_stdout, sprawl_guard_no_root, stderr};

fn command<'document>(document: &'document Value, path: &[&str]) -> &'document Value {
    document["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["path"] == json!(path))
        .unwrap()
}

fn argument<'command>(command: &'command Value, id: &str) -> &'command Value {
    command["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|argument| argument["id"] == id)
        .unwrap()
}

mod when_the_hidden_cli_reference_generator_is_requested {
    use super::*;

    #[test]
    fn it_renders_the_public_mechanical_reference_as_json() {
        let output = sprawl_guard_no_root()
            .args(["machine", "generate-cli-reference-json"])
            .output()
            .unwrap();

        assert_exit_code(&output, exit_codes::SUCCESS);
        assert_eq!(stderr(&output), "");
        let document = json_stdout(&output);

        assert_eq!(document["schema_version"], 2);
        assert!(
            command(&document, &["sprawl-guard"])["arguments"]
                .as_array()
                .unwrap()
                .iter()
                .all(|argument| argument["id"] != "color")
        );
        assert_eq!(
            argument(command(&document, &["sprawl-guard"]), "version")["syntax"],
            "-V, --version",
        );
        let init_arguments = command(&document, &["sprawl-guard", "init"])["arguments"]
            .as_array()
            .unwrap();
        assert!(init_arguments.iter().all(|argument| {
            !matches!(argument["id"].as_str(), Some("languages" | "all_languages"))
        }));
        assert_eq!(
            argument(command(&document, &["sprawl-guard", "check"]), "paths")["syntax"],
            "[PATHS]...",
        );
        assert_eq!(
            argument(command(&document, &["sprawl-guard", "check"]), "format")["possible_values"],
            json!([
                {"name": "human", "help": "Human-readable text output"},
                {"name": "json", "help": "JSON output"},
                {"name": "sarif", "help": "SARIF output"},
                {"name": "github", "help": "Human-readable text output, followed by GitHub Actions annotations"},
            ]),
        );
        assert_eq!(
            argument(
                command(&document, &["sprawl-guard", "check"]),
                "require_ratchet_current",
            )["conflicts_with"],
            json!(["--no-require-ratchet-current"]),
        );
    }

    #[test]
    fn it_omits_hidden_commands_and_claps_synthesized_help_tree() {
        let output = sprawl_guard_no_root()
            .args(["machine", "generate-cli-reference-json"])
            .output()
            .unwrap();
        let document = json_stdout(&output);
        let commands = document["commands"].as_array().unwrap();

        assert!(commands.iter().all(|command| {
            command["path"]
                .as_array()
                .unwrap()
                .iter()
                .all(|part| part != "help")
        }));
        assert!(commands.iter().all(|command| command["path"]
            != json!(["sprawl-guard", "machine", "generate-cli-reference-json"])));
    }
}
