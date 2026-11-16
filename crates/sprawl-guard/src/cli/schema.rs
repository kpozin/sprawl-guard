use super::SchemaDocumentKind;

/// Returns the embedded schema with its trailing newline.
pub(super) const fn schema_stdout(format: SchemaDocumentKind) -> &'static str {
    match format {
        SchemaDocumentKind::Config => include_str!("schemas/config.json"),
        SchemaDocumentKind::Ratchet => include_str!("schemas/ratchet.json"),
    }
}
