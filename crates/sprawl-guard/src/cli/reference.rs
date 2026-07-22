use std::collections::HashSet;

use clap::{Arg, Command, CommandFactory};
use serde::Serialize;

use super::Cli;
use crate::error::{CliError, Result};

macro_rules! string_newtype {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Serialize)]
        #[serde(transparent)]
        struct $name(String);

        impl $name {
            fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }
        }
    };
}

/// Renders the public, mechanical CLI reference as JSON for the documentation site.
pub(crate) fn render_cli_reference_json() -> Result<String> {
    let mut command = Cli::command();
    command.build();

    let document = CliReferenceDocument::from_command(&mut command);
    let mut json = serde_json::to_string_pretty(&document)
        .map_err(|source| CliError::RenderCliReference { source })?;
    json.push('\n');
    Ok(json)
}

/// The versioned CLI-reference document consumed by Starlight.
#[derive(Serialize)]
struct CliReferenceDocument {
    schema_version: CliReferenceSchemaVersion,
    commands: Vec<CliReferenceCommand>,
}

impl CliReferenceDocument {
    fn from_command(command: &mut Command) -> Self {
        let mut commands = Vec::new();
        let mut path = Vec::new();
        collect_public_commands(command, &mut path, &HashSet::new(), &mut commands);
        Self {
            schema_version: CliReferenceSchemaVersion::CURRENT,
            commands,
        }
    }
}

/// The mechanical reference for one public command.
#[derive(Serialize)]
struct CliReferenceCommand {
    path: Vec<CommandName>,
    usage: UsageText,
    arguments: Vec<CliReferenceArgument>,
}

impl CliReferenceCommand {
    fn from_command(
        command: &mut Command,
        path: Vec<CommandName>,
        inherited_global_ids: &HashSet<String>,
    ) -> Self {
        let usage = UsageText::new(command.render_usage().to_string());
        let mut arguments = command
            .get_arguments()
            .filter(|argument| {
                argument_is_visible_in_long_help(argument)
                    && !(argument.is_global_set()
                        && inherited_global_ids.contains(argument.get_id().as_str()))
            })
            .collect::<Vec<_>>();
        arguments.sort_by_key(|argument| argument.get_display_order());

        Self {
            path,
            usage,
            arguments: arguments
                .into_iter()
                .map(|argument| CliReferenceArgument::from_arg(command, argument))
                .collect(),
        }
    }
}

fn collect_public_commands(
    command: &mut Command,
    path: &mut Vec<CommandName>,
    inherited_global_ids: &HashSet<String>,
    commands: &mut Vec<CliReferenceCommand>,
) {
    let is_root = path.is_empty();
    if !is_root && (command.is_hide_set() || command.get_name() == "help") {
        return;
    }

    path.push(CommandName::new(command.get_name()));
    commands.push(CliReferenceCommand::from_command(
        command,
        path.clone(),
        inherited_global_ids,
    ));
    let mut descendant_global_ids = inherited_global_ids.clone();
    descendant_global_ids.extend(
        command
            .get_arguments()
            .filter(|argument| argument.is_global_set())
            .map(|argument| argument.get_id().as_str().to_owned()),
    );
    let mut subcommands = command.get_subcommands_mut().collect::<Vec<_>>();
    subcommands.sort_by_key(|subcommand| subcommand.get_display_order());
    for subcommand in subcommands {
        collect_public_commands(subcommand, path, &descendant_global_ids, commands);
    }
    path.pop();
}

/// One visible argument or option row rendered by Starlight.
#[derive(Serialize)]
struct CliReferenceArgument {
    id: ArgumentId,
    syntax: ArgumentSyntax,
    help: Option<HelpText>,
    conflicts_with: Vec<ArgumentSyntax>,
    default_values: Vec<DefaultValue>,
    possible_values: Vec<CliReferencePossibleValue>,
}

impl CliReferenceArgument {
    fn from_arg(command: &Command, argument: &Arg) -> Self {
        Self {
            id: ArgumentId::new(argument.get_id().as_str()),
            syntax: argument_syntax(argument),
            help: argument
                .get_long_help()
                .or_else(|| argument.get_help())
                .map(HelpText::from_styled),
            conflicts_with: command
                .get_arg_conflicts_with(argument)
                .into_iter()
                .filter(|conflict| argument_is_visible_in_long_help(conflict))
                .map(argument_syntax)
                .collect(),
            default_values: visible_default_values(argument),
            possible_values: visible_possible_values(argument),
        }
    }
}

fn argument_is_visible_in_long_help(argument: &Arg) -> bool {
    !argument.is_hide_set() && !argument.is_hide_long_help_set()
}

fn argument_syntax(argument: &Arg) -> ArgumentSyntax {
    if argument.is_positional() {
        return ArgumentSyntax::new(argument.to_string());
    }

    let rendered = argument.to_string();
    let canonical = argument
        .get_long()
        .map(|long| format!("--{long}"))
        .or_else(|| argument.get_short().map(|short| format!("-{short}")))
        .unwrap_or_default();
    let suffix = rendered.strip_prefix(&canonical).unwrap_or(&rendered);
    let mut spellings = Vec::new();
    if let Some(short) = argument.get_short() {
        spellings.push(format!("-{short}"));
    }
    spellings.extend(
        argument
            .get_visible_short_aliases()
            .unwrap_or_default()
            .into_iter()
            .map(|alias| format!("-{alias}")),
    );
    if let Some(long) = argument.get_long() {
        spellings.push(format!("--{long}"));
    }
    spellings.extend(
        argument
            .get_visible_aliases()
            .unwrap_or_default()
            .into_iter()
            .map(|alias| format!("--{alias}")),
    );
    ArgumentSyntax::new(format!("{}{suffix}", spellings.join(", ")))
}

fn visible_default_values(argument: &Arg) -> Vec<DefaultValue> {
    let takes_values = argument
        .get_num_args()
        .is_some_and(|range| range.max_values() > 0);
    if !takes_values || argument.is_hide_default_value_set() {
        return Vec::new();
    }

    argument
        .get_default_values()
        .iter()
        .map(|value| DefaultValue::new(value.to_string_lossy()))
        .collect()
}

fn visible_possible_values(argument: &Arg) -> Vec<CliReferencePossibleValue> {
    if argument.is_hide_possible_values_set() {
        return Vec::new();
    }

    argument
        .get_possible_values()
        .iter()
        .filter(|value| !value.is_hide_set())
        .map(|value| CliReferencePossibleValue {
            name: PossibleValueName::new(value.get_name()),
            help: value.get_help().map(HelpText::from_styled),
        })
        .collect()
}

/// One visible value accepted by an argument.
#[derive(Serialize)]
struct CliReferencePossibleValue {
    name: PossibleValueName,
    help: Option<HelpText>,
}

/// The schema version for the generated document.
#[derive(Serialize)]
#[serde(transparent)]
struct CliReferenceSchemaVersion(u8);

impl CliReferenceSchemaVersion {
    const CURRENT: Self = Self(2);
}

string_newtype!(CommandName, "A canonical Clap command name.");
string_newtype!(ArgumentId, "A canonical Clap argument identifier.");
string_newtype!(
    ArgumentSyntax,
    "Rendered syntax for one argument or option."
);
string_newtype!(DefaultValue, "A visible default value for an argument.");
string_newtype!(
    PossibleValueName,
    "A visible value accepted by an argument."
);

/// Human-facing text supplied by Clap.
#[derive(Serialize)]
#[serde(transparent)]
struct HelpText(String);

impl HelpText {
    fn from_styled(value: &clap::builder::StyledStr) -> Self {
        Self(value.to_string())
    }
}

/// One usage line rendered by Clap.
#[derive(Serialize)]
#[serde(transparent)]
struct UsageText(String);

impl UsageText {
    fn new(value: String) -> Self {
        Self(value)
    }
}

#[cfg(test)]
mod tests {
    use clap::builder::PossibleValue;
    use clap::{Arg, Command};
    use serde_json::json;

    use super::*;

    fn rendered_command(command: &mut Command, path: &[&str]) -> serde_json::Value {
        command.build();
        let document = serde_json::to_value(CliReferenceDocument::from_command(command)).unwrap();
        document["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|command| command["path"] == json!(path))
            .unwrap()
            .clone()
    }

    mod when_arguments_have_hidden_and_visible_help_metadata {
        use super::*;

        #[test]
        fn it_emits_only_the_rows_rendered_by_the_site() {
            let mut command = Command::new("fixture")
                .disable_help_flag(true)
                .arg(
                    Arg::new("option")
                        .long("option")
                        .visible_alias("visible-long-alias")
                        .alias("hidden-long-alias")
                        .short('o')
                        .visible_short_alias('v')
                        .short_alias('x')
                        .value_name("VALUE")
                        .default_value("visible-value")
                        .value_parser([
                            PossibleValue::new("visible-value").help("Shown value"),
                            PossibleValue::new("hidden-value").hide(true),
                        ]),
                )
                .arg(Arg::new("hidden").long("hidden").hide(true))
                .arg(
                    Arg::new("hidden-from-long-help")
                        .long("hidden-from-long-help")
                        .hide_long_help(true),
                );
            command.build();

            let document = CliReferenceDocument::from_command(&mut command);
            let json = serde_json::to_value(document).unwrap();

            assert_eq!(
                json["commands"][0]["arguments"],
                json!([{
                    "id": "option",
                    "syntax": "-o, -v, --option, --visible-long-alias <VALUE>",
                    "help": null,
                    "conflicts_with": [],
                    "default_values": ["visible-value"],
                    "possible_values": [{"name": "visible-value", "help": "Shown value"}],
                }]),
            );
        }
    }

    mod when_an_argument_hides_its_possible_values {
        use super::*;

        #[test]
        fn it_omits_the_values_from_the_site_document() {
            let mut command = Command::new("fixture").disable_help_flag(true).arg(
                Arg::new("option")
                    .long("option")
                    .hide_possible_values(true)
                    .value_parser(["visible-value"]),
            );
            command.build();

            let document =
                serde_json::to_value(CliReferenceDocument::from_command(&mut command)).unwrap();

            assert_eq!(
                document["commands"][0]["arguments"][0]["possible_values"],
                json!([]),
            );
        }
    }

    mod when_a_command_group_declares_a_global_option {
        use super::*;

        #[test]
        fn it_emits_the_option_at_its_declaring_scope_only() {
            let mut command = Command::new("fixture").disable_help_flag(true).subcommand(
                Command::new("group")
                    .disable_help_flag(true)
                    .arg(Arg::new("group-option").long("group-option").global(true))
                    .subcommand(Command::new("child").disable_help_flag(true)),
            );
            command.build();

            let document =
                serde_json::to_value(CliReferenceDocument::from_command(&mut command)).unwrap();
            let commands = document["commands"].as_array().unwrap();
            let group = commands
                .iter()
                .find(|command| command["path"] == json!(["fixture", "group"]))
                .unwrap();
            let child = commands
                .iter()
                .find(|command| command["path"] == json!(["fixture", "group", "child"]))
                .unwrap();

            assert_eq!(group["arguments"][0]["id"], "group-option");
            assert_eq!(child["arguments"], json!([]));
        }
    }

    mod when_a_child_shadows_an_inherited_global_option {
        use super::*;

        #[test]
        fn it_emits_the_child_declaration() {
            let mut command = Command::new("fixture")
                .disable_help_flag(true)
                .arg(Arg::new("scope").long("scope").global(true))
                .subcommand(
                    Command::new("child")
                        .disable_help_flag(true)
                        .arg(Arg::new("scope").long("child-scope")),
                );
            let child = rendered_command(&mut command, &["fixture", "child"]);

            assert_eq!(child["arguments"][0]["syntax"], "--child-scope <scope>");
        }
    }

    mod when_an_argument_conflicts_with_an_inherited_global_option {
        use super::*;

        #[test]
        fn it_preserves_the_cross_scope_conflict() {
            let mut command = Command::new("fixture")
                .disable_help_flag(true)
                .arg(Arg::new("config").long("config").global(true))
                .subcommand(
                    Command::new("child")
                        .disable_help_flag(true)
                        .arg(Arg::new("local").long("local").conflicts_with("config")),
                );
            let child = rendered_command(&mut command, &["fixture", "child"]);

            assert_eq!(
                child["arguments"][0]["conflicts_with"],
                json!(["--config <config>"]),
            );
        }
    }
}
