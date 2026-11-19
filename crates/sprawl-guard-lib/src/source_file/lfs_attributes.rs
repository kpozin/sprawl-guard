//! Which files `.gitattributes` assigns to Git LFS.
//!
//! Git LFS stores a file when `.gitattributes` gives its path the attribute `filter=lfs`. `check`
//! reads `.gitattributes` itself rather than asking `git check-attr`, because the WASI build can't
//! run `git`. The result is the same whether or not Git LFS has downloaded the file.
//!
//! The lines are parsed by hand rather than with `gix-attributes`, which would bring in a family of
//! `gix` crates to read one attribute. Only the attribute tokens are parsed here: patterns use
//! gitignore syntax, so `ignore`'s gitignore matcher, already a dependency, matches them. Macro
//! definitions, quoted patterns and `.git/info/attributes` are not read. Like Git, the reader
//! skips a `.gitattributes` file that is a symbolic link.

use std::collections::HashMap;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::{FileStorage, RelativePath};

/// Name of the file that assigns attributes to the paths in its directory and below.
const ATTRIBUTES_FILE: &str = ".gitattributes";

/// Name of the entry that marks the top of a Git work tree.
const GIT_DIR: &str = ".git";

/// Why a `.gitattributes` entry that is neither a regular file nor a symbolic link isn't read.
const NOT_A_REGULAR_FILE: &str = "not a regular file";

/// Attribute that names a file's clean and smudge filter.
const FILTER_ATTRIBUTE: &str = "filter";

/// Value of the filter attribute for files that Git LFS stores.
const LFS_FILTER: &str = "lfs";

/// Prefix of a line that defines an attribute macro.
const MACRO_PREFIX: &str = "[attr]";

/// POSIX character class that `git lfs track` writes for a space, which the gitignore matcher
/// doesn't support.
const SPACE_CLASS: &str = "[:space:]";

/// Characters that `[:space:]` matches.
const SPACE_CHARACTERS: &str = " \t\n\r\x0B\x0C";

/// Byte order mark that Git skips at the start of a `.gitattributes` file.
const BYTE_ORDER_MARK: char = '\u{FEFF}';

/// Character that makes the next pattern character literal.
const ESCAPE: char = '\\';

/// Opens a character class in a pattern.
const CLASS_START: char = '[';

/// Closes a character class, unless it comes first in the class.
const CLASS_END: char = ']';

/// Characters that negate a character class when they follow its opening bracket.
const CLASS_NEGATIONS: [char; 2] = ['!', '^'];

/// Finds how each file is stored, reading every `.gitattributes` file at most once.
pub(crate) struct LfsAttributes {
    /// Absolute check root.
    root: PathBuf,
    /// Directories above the root, nearest first, up to the top of the Git work tree.
    ancestors: Vec<PathBuf>,
    /// `.gitattributes` file of each directory looked at so far.
    files: HashMap<PathBuf, AttributesFile>,
    /// `.gitattributes` files that exist but couldn't be read, so their lines were ignored.
    failures: Vec<UnreadableAttributes>,
}

/// What a directory's `.gitattributes` file says about the filter attribute.
enum AttributesFile {
    /// The directory has no `.gitattributes` file, or only a symbolic link that Git ignores.
    Missing,
    /// The file exists but couldn't be read, so it may assign any file below it to Git LFS.
    Unreadable,
    /// The file was read; the matcher holds its lines that set or unset the filter attribute.
    Read(Gitignore),
}

/// A `.gitattributes` file that exists but couldn't be read.
#[derive(Debug)]
pub(crate) struct UnreadableAttributes {
    /// Absolute path of the file.
    pub(crate) path: PathBuf,
    /// Why it couldn't be read.
    pub(crate) source: io::Error,
}

impl LfsAttributes {
    /// Creates a reader for the files under `root`, an absolute path.
    ///
    /// `.gitattributes` files in the directories above the root count too, up to the top of the
    /// Git work tree. When no directory above the root holds `.git`, or none can be seen, only the
    /// root and its subdirectories count.
    pub(crate) fn new(root: &Path) -> Self {
        let ancestors = if root.join(GIT_DIR).exists() {
            Vec::new()
        } else {
            work_tree_ancestors(root)
        };
        Self {
            root: root.to_path_buf(),
            ancestors,
            files: HashMap::new(),
            failures: Vec::new(),
        }
    }

    /// Returns how the file at `path` is stored, or `None` when it can't be known.
    ///
    /// The deepest `.gitattributes` file that sets or unsets the filter attribute for the path
    /// decides. When a `.gitattributes` file that can't be read comes before the one that decides,
    /// its lines might have decided, so the storage is unknown. Such files are recorded in
    /// [`Self::take_failures`].
    pub(crate) fn storage(&mut self, path: &RelativePath) -> Option<FileStorage> {
        let file = self.root.join(path.to_path_buf());
        let directories = file
            .ancestors()
            .skip(1)
            .take_while(|directory| directory.starts_with(&self.root))
            .map(Path::to_path_buf)
            .chain(self.ancestors.iter().cloned())
            .collect::<Vec<_>>();
        for directory in directories {
            match self.file(directory) {
                AttributesFile::Missing => {}
                AttributesFile::Unreadable => return None,
                AttributesFile::Read(matcher) => match matcher.matched(&file, false) {
                    Match::Ignore(_) => return Some(FileStorage::Lfs),
                    Match::Whitelist(_) => return Some(FileStorage::Regular),
                    Match::None => {}
                },
            }
        }
        Some(FileStorage::Regular)
    }

    /// Returns the `.gitattributes` files that couldn't be read since the last call.
    pub(crate) fn take_failures(&mut self) -> Vec<UnreadableAttributes> {
        std::mem::take(&mut self.failures)
    }

    /// Returns the `.gitattributes` file in `directory`, reading it on first use.
    fn file(&mut self, directory: PathBuf) -> &AttributesFile {
        let failures = &mut self.failures;
        self.files.entry(directory).or_insert_with_key(|directory| {
            let path = directory.join(ATTRIBUTES_FILE);
            match read_regular_file(&path) {
                Ok(Some(text)) => AttributesFile::Read(filter_matcher(directory, &text)),
                Ok(None) => AttributesFile::Missing,
                Err(source) => {
                    failures.push(UnreadableAttributes { path, source });
                    AttributesFile::Unreadable
                }
            }
        })
    }
}

/// Reads the `.gitattributes` file at `path`, or returns `None` when Git wouldn't read one there.
///
/// Git ignores a missing file and a symbolic link alike. Any other entry that isn't a regular
/// file, such as a directory or a named pipe, is an error and isn't opened, so a pipe can't block
/// the check.
fn read_regular_file(path: &Path) -> io::Result<Option<String>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(source),
    };
    if metadata.is_symlink() {
        return Ok(None);
    }
    if !metadata.is_file() {
        return Err(io::Error::new(ErrorKind::InvalidInput, NOT_A_REGULAR_FILE));
    }
    fs::read_to_string(path).map(Some)
}

/// Returns the directories above `root`, nearest first, up to the one that holds `.git`, or none
/// when no directory above the root holds it.
fn work_tree_ancestors(root: &Path) -> Vec<PathBuf> {
    let mut ancestors = Vec::new();
    for directory in root.ancestors().skip(1) {
        ancestors.push(directory.to_path_buf());
        if directory.join(GIT_DIR).exists() {
            return ancestors;
        }
    }
    Vec::new()
}

/// Builds a matcher from the text of the `.gitattributes` file in `directory`.
///
/// A line that sets `filter=lfs` becomes an ignore pattern, and a line that sets the filter to
/// anything else or unsets it becomes a whitelist pattern. The gitignore matcher returns the last
/// pattern that matches, which is the line that decides the attribute in Git too. Lines that
/// don't mention the filter, and patterns the matcher rejects, are skipped.
fn filter_matcher(directory: &Path, text: &str) -> Gitignore {
    let mut builder = GitignoreBuilder::new(directory);
    let text = text.strip_prefix(BYTE_ORDER_MARK).unwrap_or(text);
    for (pattern, storage) in text.lines().filter_map(filter_line) {
        let Some(pattern) = matcher_pattern(&pattern.replace(SPACE_CLASS, SPACE_CHARACTERS)) else {
            continue;
        };
        let line = match storage {
            FileStorage::Lfs => pattern,
            FileStorage::Regular => format!("!{pattern}"),
        };
        // Git skips a pattern it can't parse too, so the rest of the file still applies.
        let _ = builder.add_line(None, &line);
    }
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

/// Translates a Git pattern for the gitignore matcher, or returns `None` for a pattern that Git
/// never matches.
///
/// Braces are escaped, because Git matches them literally but the matcher reads them as
/// alternatives such as `{a,b}`. A character that the pattern already escapes is copied with its
/// escape. Braces inside a character class such as `[{]` are already literal there, and the
/// matcher would read an escape inside the class as a backslash, so they are copied unchanged.
///
/// A class that is never closed makes Git match nothing, while the matcher would read its bracket
/// literally, so such a pattern is dropped.
fn matcher_pattern(pattern: &str) -> Option<String> {
    let mut escaped = String::with_capacity(pattern.len());
    let mut characters = pattern.chars().peekable();
    let mut in_class = false;
    while let Some(character) = characters.next() {
        match character {
            ESCAPE => {
                escaped.push(ESCAPE);
                escaped.extend(characters.next());
            }
            CLASS_START if !in_class => {
                in_class = true;
                escaped.push(character);
                // A negation and then a `]` right after the opening bracket belong to the class.
                escaped.extend(characters.next_if(|next| CLASS_NEGATIONS.contains(next)));
                escaped.extend(characters.next_if_eq(&CLASS_END));
            }
            CLASS_END if in_class => {
                in_class = false;
                escaped.push(character);
            }
            '{' | '}' if !in_class => {
                escaped.push(ESCAPE);
                escaped.push(character);
            }
            _ => escaped.push(character),
        }
    }
    (!in_class).then_some(escaped)
}

/// Parses one `.gitattributes` line into its pattern and how it says matching files are stored.
///
/// Returns `None` for blank lines, comments, macro definitions, quoted or negated patterns, and
/// lines that don't mention the filter attribute. When a line mentions the filter more than once,
/// the last mention wins, as in Git.
fn filter_line(line: &str) -> Option<(&str, FileStorage)> {
    let mut tokens = line.split_ascii_whitespace();
    let pattern = tokens.next()?;
    if pattern.starts_with(['#', '"', '!']) || pattern.starts_with(MACRO_PREFIX) {
        return None;
    }
    let storage = tokens.filter_map(filter_storage).next_back()?;
    Some((pattern, storage))
}

/// Returns where an attribute token says Git stores a file, or `None` when the token isn't about
/// the filter attribute.
fn filter_storage(token: &str) -> Option<FileStorage> {
    match token.split_once('=') {
        Some((FILTER_ATTRIBUTE, LFS_FILTER)) => Some(FileStorage::Lfs),
        Some((FILTER_ATTRIBUTE, _)) => Some(FileStorage::Regular),
        Some(_) => None,
        // `filter`, `-filter` and `!filter` set the filter to true, unset it, or leave it
        // unspecified, and none of those stores the file in Git LFS.
        None => (token.trim_start_matches(['-', '!']) == FILTER_ATTRIBUTE)
            .then_some(FileStorage::Regular),
    }
}

#[cfg(test)]
mod tests;
