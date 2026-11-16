use clap::{Parser, ValueEnum};
use sprawl_guard_lib::config::authored_config_json_schema;
use sprawl_guard_lib::{authored_ratchet_json_schema, render_authored_schema};

#[derive(Parser)]
struct Args {
    #[arg(value_enum)]
    document: SchemaDocument,
}

#[derive(Clone, ValueEnum)]
enum SchemaDocument {
    Config,
    Ratchet,
}

fn main() {
    let schema = match Args::parse().document {
        SchemaDocument::Config => authored_config_json_schema(),
        SchemaDocument::Ratchet => authored_ratchet_json_schema(),
    };
    print!("{}", render_authored_schema(schema));
}
