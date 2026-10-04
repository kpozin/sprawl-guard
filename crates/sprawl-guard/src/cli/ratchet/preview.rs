use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;

use sprawl_guard_lib::{RatchetFile, RatchetLookup};

use crate::error::Result;

/// Describes changed ceilings by their complete metric identities.
pub(super) fn render_changes(previous: &RatchetFile, next: &RatchetFile) -> Result<String> {
    let previous = RatchetLookup::try_from(previous.clone())?;
    let next = RatchetLookup::try_from(next.clone())?;
    let mut output = render_map_changes(previous.files(), next.files(), |key| {
        format!("file {:?} [{}] max_loc", key.path().as_str(), key.role())
    });
    output.push_str(&render_map_changes(
        previous.directories(),
        next.directories(),
        |key| {
            let path = if key.path().as_str().is_empty() {
                "."
            } else {
                key.path().as_str()
            };
            format!(
                "directory {path:?} [{}, {}] leaf_files",
                key.role(),
                key.group()
            )
        },
    ));
    Ok(output)
}

fn render_map_changes<K: Ord, V: Ord + Display>(
    previous: &BTreeMap<K, V>,
    next: &BTreeMap<K, V>,
    describe: impl Fn(&K) -> String,
) -> String {
    let mut output = String::new();
    for key in previous.keys().chain(next.keys()).collect::<BTreeSet<_>>() {
        let change = match (previous.get(key), next.get(key)) {
            (None, Some(after)) => format!("+ add {}: none -> {after}\n", describe(key)),
            (Some(before), None) => format!("- remove {}: {before} -> none\n", describe(key)),
            (Some(before), Some(after)) => match after.cmp(before) {
                std::cmp::Ordering::Greater => {
                    format!("! raise {}: {before} -> {after}\n", describe(key))
                }
                std::cmp::Ordering::Less => {
                    format!("- lower {}: {before} -> {after}\n", describe(key))
                }
                std::cmp::Ordering::Equal => continue,
            },
            (None, None) => continue,
        };
        output.push_str(&change);
    }
    output
}
