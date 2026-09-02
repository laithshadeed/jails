//! What changed on disk since last time.
//!
//! A different secret from "how do I invoke the build tool": nothing here
//! knows what Maven or Gradle is, and every rule in it is about filesystem
//! observation. `watch` is the only caller.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// What every watched file looked like at one moment.
///
/// A content-addressed map, not a timestamp high-water mark. Digests make file
/// bytes authoritative even when an editor preserves timestamps, while the
/// map shape detects additions and deletions and names every changed input.
///
/// The watched set is the whole project, not just `.java`: a template, a
/// migration, `application.properties`, `pom.xml`, `compose.yaml` and
/// `jails.toml` all change what a running application does, and a watcher
/// that ignores them makes the reader wonder why their change did nothing.
#[derive(Clone, Debug, Default)]
pub(super) struct Snapshot {
    files: BTreeMap<PathBuf, FileStamp>,
    gaps: Vec<String>,
}

impl Snapshot {
    pub(super) fn overflowed(&self) -> bool {
        !self.gaps.is_empty()
    }

    pub(super) fn gaps(&self) -> &[String] {
        &self.gaps
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileStamp {
    size: u64,
    digest: [u8; 32],
}

pub(super) fn fingerprint(root: &Path) -> Snapshot {
    let mut snapshot = Snapshot::default();
    for dir in [
        "src/main/java",
        "src/main/resources",
        "src/test/java",
        "src/test/resources",
        ".mvn",
        "gradle",
    ] {
        collect_files(&root.join(dir), &mut snapshot);
    }
    for file in [
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
        "gradle.properties",
        "mvnw",
        "mvnw.cmd",
        "gradlew",
        "gradlew.bat",
        "compose.yaml",
        "jails.toml",
        ".jails/app.toml",
    ] {
        let path = root.join(file);
        if path.symlink_metadata().is_ok() {
            collect_file(&path, &mut snapshot);
        }
    }
    snapshot.gaps.sort();
    snapshot.gaps.dedup();
    snapshot
}

fn collect_files(dir: &Path, snapshot: &mut Snapshot) {
    if dir
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        snapshot
            .gaps
            .push(format!("{} is a symlink", dir.display()));
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            snapshot
                .gaps
                .push(format!("{} could not be scanned ({error})", dir.display()));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                snapshot.gaps.push(format!(
                    "{} has an unreadable entry ({error})",
                    dir.display()
                ));
                continue;
            }
        };
        let path = entry.path();
        let kind = match entry.file_type() {
            Ok(kind) => kind,
            Err(error) => {
                snapshot.gaps.push(format!(
                    "{} could not be classified ({error})",
                    path.display()
                ));
                continue;
            }
        };
        if kind.is_symlink() {
            snapshot
                .gaps
                .push(format!("{} is a symlink", path.display()));
        } else if kind.is_dir() {
            // Build output is a *consequence* of a change, not one.
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect_files(&path, snapshot);
        } else if kind.is_file() {
            collect_file(&path, snapshot);
        }
    }
}

fn collect_file(path: &Path, snapshot: &mut Snapshot) {
    match path.symlink_metadata() {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            snapshot
                .gaps
                .push(format!("{} is a symlink", path.display()));
            return;
        }
        Ok(metadata) if !metadata.is_file() => return,
        Ok(_) => {}
        Err(error) => {
            snapshot.gaps.push(format!(
                "{} could not be inspected ({error})",
                path.display()
            ));
            return;
        }
    }
    match fs::read(path) {
        Ok(bytes) => {
            snapshot.files.insert(
                path.to_path_buf(),
                FileStamp {
                    size: bytes.len() as u64,
                    digest: jails_support::sha256(&bytes),
                },
            );
        }
        Err(error) => snapshot
            .gaps
            .push(format!("{} could not be hashed ({error})", path.display())),
    }
}

/// What moved between two fingerprints, as lines a reader can act on.
pub(super) fn changes_between(before: &Snapshot, after: &Snapshot, root: &Path) -> Vec<String> {
    let relative = |path: &Path| {
        path.strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string()
    };
    let mut changes = Vec::new();
    for (path, stamp) in &after.files {
        match before.files.get(path) {
            None => changes.push(format!("added   {}", relative(path))),
            Some(previous) if previous != stamp => {
                changes.push(format!("changed {}", relative(path)))
            }
            Some(_) => {}
        }
    }
    for path in before.files.keys() {
        if !after.files.contains_key(path) {
            changes.push(format!("deleted {}", relative(path)));
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        jails_support::scratch::ScratchDir::in_temp(&format!("jails-watch-test-{label}"))
            .unwrap()
            .keep()
    }

    #[test]
    fn the_watcher_notices_every_kind_of_change_and_names_the_file() {
        let root = scratch("fingerprint");
        let java = root.join("src/main/java/com/example");
        let resources = root.join("src/main/resources");
        fs::create_dir_all(&java).unwrap();
        fs::create_dir_all(&resources).unwrap();
        fs::write(java.join("App.java"), "x").unwrap();
        fs::write(resources.join("application.properties"), "a=1").unwrap();
        fs::write(root.join("pom.xml"), "<project/>").unwrap();

        let before = fingerprint(&root);
        assert_eq!(before.files.len(), 3, "{before:?}");
        assert!(changes_between(&before, &before, &root).is_empty());

        // A resource is a change: it decides what the running application
        // does just as much as a class does.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(resources.join("application.properties"), "a=2").unwrap();
        let changed = fingerprint(&root);
        assert_eq!(
            changes_between(&before, &changed, &root),
            vec!["changed src/main/resources/application.properties"]
        );

        // A new file, and a deleted one -- which a timestamp high-water mark
        // could not see at all, since removing a file lowers nothing.
        fs::write(java.join("Extra.java"), "x").unwrap();
        fs::remove_file(java.join("App.java")).unwrap();
        let after = fingerprint(&root);
        let changes = changes_between(&changed, &after, &root);
        assert!(
            changes.contains(&"added   src/main/java/com/example/Extra.java".to_string()),
            "{changes:?}"
        );
        assert!(
            changes.contains(&"deleted src/main/java/com/example/App.java".to_string()),
            "{changes:?}"
        );
    }

    #[test]
    fn an_older_checkout_with_different_bytes_is_still_a_change() {
        // `git checkout` can move metadata backwards; the bytes remain the
        // authority in either comparison direction.
        let root = scratch("fingerprint-backwards");
        let java = root.join("src/main/java");
        fs::create_dir_all(&java).unwrap();
        fs::write(java.join("App.java"), "x").unwrap();

        let before = fingerprint(&root);
        let path = java.join("App.java");
        fs::write(&path, "older checkout bytes").unwrap();
        let older = fingerprint(&root);
        assert_eq!(
            changes_between(&older, &before, &root),
            vec!["changed src/main/java/App.java"]
        );
        assert_eq!(
            changes_between(&before, &older, &root),
            vec!["changed src/main/java/App.java"],
            "a change is a change in either direction"
        );
    }

    #[test]
    fn content_is_authority_even_when_metadata_does_not_help() {
        let root = scratch("fingerprint-content");
        let java = root.join("src/main/java");
        fs::create_dir_all(&java).unwrap();
        let path = java.join("App.java");
        fs::write(&path, "aaaa").unwrap();
        let before = fingerprint(&root);
        fs::write(&path, "bbbb").unwrap();
        let after = fingerprint(&root);
        assert_eq!(
            changes_between(&before, &after, &root),
            vec!["changed src/main/java/App.java"]
        );
    }

    #[test]
    fn build_output_is_not_a_change() {
        let root = scratch("fingerprint-target");
        let java = root.join("src/main/java");
        fs::create_dir_all(java.join("target")).unwrap();
        fs::write(java.join("App.java"), "x").unwrap();
        fs::write(java.join("target/App.class"), "compiled").unwrap();
        assert_eq!(fingerprint(&root).files.len(), 1);
    }
}
