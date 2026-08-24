//! What changed on disk since last time.
//!
//! Split out of `run.rs` because it is a different secret from "how do I
//! invoke the build tool": nothing here knows what Maven or Gradle is, and
//! every rule in it is about filesystem observation. `watch` is the only
//! caller.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// What every watched file looked like at one moment: path -> mtime.
///
/// A map, not a high-water mark. The mtime *maximum* the watcher used before
/// could only answer "has anything got newer", which gets three cases wrong,
/// all of them ordinary: it cannot name the file that changed, a **deletion**
/// lowers nothing so it goes unnoticed, and `git checkout` of an older
/// revision moves mtimes backwards -- the exact moment a reader most wants a
/// restart. Comparing maps with `!=` catches all three.
///
/// The watched set is the whole project, not just `.java`: a template, a
/// migration, `application.properties`, `pom.xml`, `compose.yaml` and
/// `jails.toml` all change what a running application does, and a watcher
/// that ignores them makes the reader wonder why their change did nothing.
pub(super) fn fingerprint(root: &Path) -> BTreeMap<PathBuf, std::time::SystemTime> {
    let mut found = BTreeMap::new();
    for dir in [
        "src/main/java",
        "src/main/resources",
        "src/test/java",
        "src/test/resources",
    ] {
        collect_mtimes(&root.join(dir), &mut found);
    }
    for file in ["pom.xml", "compose.yaml", "jails.toml"] {
        let path = root.join(file);
        if let Ok(modified) = fs::metadata(&path).and_then(|m| m.modified()) {
            found.insert(path, modified);
        }
    }
    found
}

fn collect_mtimes(dir: &Path, out: &mut BTreeMap<PathBuf, std::time::SystemTime>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Build output is a *consequence* of a change, not one.
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect_mtimes(&path, out);
        } else if let Ok(modified) = fs::metadata(&path).and_then(|m| m.modified()) {
            out.insert(path, modified);
        }
    }
}

/// What moved between two fingerprints, as lines a reader can act on.
pub(super) fn changes_between(
    before: &BTreeMap<PathBuf, std::time::SystemTime>,
    after: &BTreeMap<PathBuf, std::time::SystemTime>,
    root: &Path,
) -> Vec<String> {
    let relative = |path: &Path| {
        path.strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string()
    };
    let mut changes = Vec::new();
    for (path, when) in after {
        match before.get(path) {
            None => changes.push(format!("added   {}", relative(path))),
            // `!=`, not `>`: `git checkout` of an older revision moves an
            // mtime backwards, and that is still a change.
            Some(previous) if previous != when => {
                changes.push(format!("changed {}", relative(path)))
            }
            Some(_) => {}
        }
    }
    for path in before.keys() {
        if !after.contains_key(path) {
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
        assert_eq!(before.len(), 3, "{before:?}");
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

        // A new file, and a deleted one -- which the old high-water mark
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
    fn an_mtime_that_moves_backwards_is_still_a_change() {
        // `git checkout` of an older revision does exactly this, and it is
        // the moment a reader most wants a restart.
        let root = scratch("fingerprint-backwards");
        let java = root.join("src/main/java");
        fs::create_dir_all(&java).unwrap();
        fs::write(java.join("App.java"), "x").unwrap();

        let before = fingerprint(&root);
        let mut older = before.clone();
        let path = java.join("App.java");
        older.insert(
            path,
            before
                .values()
                .next()
                .unwrap()
                .checked_sub(std::time::Duration::from_secs(60))
                .unwrap(),
        );
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
    fn build_output_is_not_a_change() {
        let root = scratch("fingerprint-target");
        let java = root.join("src/main/java");
        fs::create_dir_all(java.join("target")).unwrap();
        fs::write(java.join("App.java"), "x").unwrap();
        fs::write(java.join("target/App.class"), "compiled").unwrap();
        assert_eq!(fingerprint(&root).len(), 1);
    }
}
