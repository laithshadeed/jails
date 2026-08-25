//! What a directory holds, as project-relative paths.
//!
//! Split out of `jails-project`'s `capture` because it is the one thing there
//! that does not know what a Java project is: a directory listing is a fact
//! about a filesystem. `jails-commit`'s executor needs it to recheck a captured
//! listing before activation, and reaching up two crates for it was
//! `pending.md` §7.3's whole complaint.

use crate::Result;
use jails_protocol::identity::ProjectPath;
use std::path::Path;

/// The entries of one directory, by name, in one order.
///
/// Sorted, because `read_dir` order is the filesystem's and two captures of an
/// unchanged directory must be the same value. An absent directory lists as
/// empty rather than failing: "nothing has been generated yet" is the ordinary
/// state of a migrations directory.
/// One directory listing, as the snapshot records it.
///
/// Public because the commit-time recheck has to produce the *same* list from
/// the same directory -- §R4.3 step 2 compares its digest against the one the
/// plan captured, and two enumerations that sorted differently would report a
/// change nobody made.
pub fn list_directory(at: &Path, path: &ProjectPath) -> Result<Vec<ProjectPath>> {
    list(at, path)
}

pub fn list(at: &Path, path: &ProjectPath) -> Result<Vec<ProjectPath>> {
    let entries = match std::fs::read_dir(at) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("failed to list {path}: {error}").into()),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("failed to list {path}: {error}"))?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| format!("{path} contains a name that is not valid UTF-8"))?;
        names.push(ProjectPath::parse(&format!("{path}/{name}"))?);
    }
    names.sort();
    Ok(names)
}
