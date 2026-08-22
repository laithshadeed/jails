//! Provenance for generated paths.
//!
//! `.jails/files` is the portable, sorted union required by plan.md §11.2.
//! Per-intent files preserve the extra fact `destroy` needs: which invocation
//! created which paths. Bodies and ownership hashes deliberately do not live
//! here; drift repair is a regeneration/merge problem, not a reason to claim
//! user-edited bytes.

use crate::Result;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

const FILES: &str = ".jails/files";
const VERSION: &str = ".jails/version";
const INTENTS: &str = ".jails/intents";
const MODELS: &str = ".jails/models";

fn safe_relative(root: &Path, path: &Path) -> Result<String> {
    let relative = path.strip_prefix(root).map_err(|_| {
        format!(
            "generated path {} escapes project root {}",
            path.display(),
            root.display()
        )
    })?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "generated path {} is not a confined relative path",
                    path.display()
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err("refusing to record the project root as a generated file".to_string());
    }
    Ok(parts.join("/"))
}

fn path_from_relative(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!(
            "invalid generated path `{relative}` in {FILES}; expected a confined relative path"
        ));
    }
    Ok(root.join(path))
}

fn identity_file(kind: &str, name: &str, package: Option<&str>) -> String {
    // Stable FNV-1a keeps arbitrary names (including markdown paths) out of
    // filenames without pulling a hashing dependency into the CLI.
    let identity = format!("{kind}\0{name}\0{}", package.unwrap_or(""));
    let hash = identity.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    let hint: String = format!("{kind}-{name}")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(48)
        .collect();
    format!("{hint}-{hash:016x}.files")
}

fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    fs::write(&temporary, contents)
        .map_err(|error| format!("failed to write {}: {error}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|error| {
        format!(
            "failed to replace {} with {}: {error}",
            path.display(),
            temporary.display()
        )
    })
}

fn rebuild_union(root: &Path) -> Result<()> {
    let intents = root.join(INTENTS);
    let mut paths = BTreeSet::new();
    if intents.is_dir() {
        let entries = fs::read_dir(&intents)
            .map_err(|error| format!("failed to read {}: {error}", intents.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!(
                    "failed to read an entry under {}: {error}",
                    intents.display()
                )
            })?;
            if !entry.path().extension().is_some_and(|ext| ext == "files") {
                continue;
            }
            let source = fs::read_to_string(entry.path())
                .map_err(|error| format!("failed to read {}: {error}", entry.path().display()))?;
            paths.extend(
                source
                    .lines()
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned),
            );
        }
    }
    let body = paths.into_iter().collect::<Vec<_>>().join("\n");
    let body = if body.is_empty() {
        body
    } else {
        format!("{body}\n")
    };
    atomic_write(&root.join(FILES), &body)?;
    atomic_write(
        &root.join(VERSION),
        &format!("{}\n", env!("CARGO_PKG_VERSION")),
    )
}

pub(crate) fn record(
    root: &Path,
    kind: &str,
    name: &str,
    package: Option<&str>,
    paths: &[PathBuf],
) -> Result<()> {
    let mut relative = BTreeSet::new();
    for path in paths {
        relative.insert(safe_relative(root, path)?);
    }
    let body = relative.into_iter().collect::<Vec<_>>().join("\n");
    let path = root.join(INTENTS).join(identity_file(kind, name, package));
    let body = if body.is_empty() {
        body
    } else {
        format!("{body}\n")
    };
    atomic_write(&path, &body)?;
    rebuild_union(root)
}

pub(crate) fn paths(
    root: &Path,
    kind: &str,
    name: &str,
    package: Option<&str>,
) -> Result<Option<Vec<PathBuf>>> {
    let path = root.join(INTENTS).join(identity_file(kind, name, package));
    if !path.is_file() {
        return Ok(None);
    }
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    source
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| path_from_relative(root, line))
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

pub(crate) fn forget(root: &Path, kind: &str, name: &str, package: Option<&str>) -> Result<()> {
    let path = root.join(INTENTS).join(identity_file(kind, name, package));
    if path.is_file() {
        fs::remove_file(&path)
            .map_err(|error| format!("failed to remove {}: {error}", path.display()))?;
        rebuild_union(root)?;
    }
    Ok(())
}

pub(crate) fn record_model(
    root: &Path,
    name: &str,
    package: Option<&str>,
    fields: &[String],
) -> Result<()> {
    let path = root
        .join(MODELS)
        .join(identity_file("model", name, package));
    let body = if fields.is_empty() {
        String::new()
    } else {
        format!("{}\n", fields.join("\n"))
    };
    atomic_write(&path, &body)
}

pub(crate) fn model_fields(
    root: &Path,
    name: &str,
    package: Option<&str>,
) -> Result<Option<Vec<String>>> {
    let path = root
        .join(MODELS)
        .join(identity_file("model", name, package));
    if !path.is_file() {
        return Ok(None);
    }
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    Ok(Some(
        source
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_paths_are_sorted_normalised_and_scoped_per_intent() {
        let root = std::env::temp_dir().join(format!(
            "jails-generated-files-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        fs::create_dir_all(&root).unwrap();
        let first = root.join("src/test/java/A.java");
        let second = root.join("src/main/java/B.java");

        record(&root, "record", "A", None, &[first.clone(), second.clone()]).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(FILES)).unwrap(),
            "src/main/java/B.java\nsrc/test/java/A.java\n"
        );
        assert_eq!(
            paths(&root, "record", "A", None).unwrap().unwrap(),
            vec![second, first]
        );
        assert_eq!(
            fs::read_to_string(root.join(VERSION)).unwrap(),
            format!("{}\n", env!("CARGO_PKG_VERSION"))
        );

        forget(&root, "record", "A", None).unwrap();
        assert!(paths(&root, "record", "A", None).unwrap().is_none());
        assert_eq!(fs::read_to_string(root.join(FILES)).unwrap(), "");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn generated_paths_refuse_to_escape_the_project() {
        let root = Path::new("/tmp/project");
        assert!(safe_relative(root, Path::new("/tmp/elsewhere/X.java")).is_err());
        assert!(path_from_relative(root, "../X.java").is_err());
        assert!(path_from_relative(root, "/tmp/X.java").is_err());
    }
}
