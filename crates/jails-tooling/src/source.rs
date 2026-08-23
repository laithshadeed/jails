//! `jails src <Type>`: where is this type's source?
//!
//! `plan.md` §14. An editor asking "take me to `JdbcClient`" has two bad
//! answers available to it. jdt.ls knows, but only once it has indexed a
//! project that compiles — which is the state you are least often in when you
//! need this. A `find` over the filesystem is instant and gets the wrong file
//! whenever two packages declare the same simple name.
//!
//! So this reads the `package` line and reports **fully qualified names with
//! their paths**, one per line, and does not pick for you when there is more
//! than one. A tool that silently picked would send an editor to the wrong
//! `Status.java` in a project that has three, which is worse than a list.
//!
//! ## Where it looks, and why that is configuration
//!
//! The project's own source roots first. Then whatever `JAILS_SOURCE_PATH`
//! names, colon-separated — and `deps` when it exists and that variable does
//! not, because a directory of read-only upstream checkouts beside the project
//! is the shape this was built for. Naming it in an environment variable
//! rather than hardcoding one convention is the difference between a tool that
//! works here and a tool that assumes here.
//!
//! Nothing is downloaded, unpacked or indexed. If the source is not on disk,
//! the answer is that it is not on disk.

use jails_support::Result;
use std::path::{Path, PathBuf};

/// One `.java` file that declares the requested simple name.
pub struct Found {
    pub qualified: String,
    pub path: PathBuf,
}

pub fn src(type_name: &str, json: bool) -> Result<()> {
    // The project root when there is one, else here. This is the one command
    // that has no business requiring a build file: "where is this type" is a
    // question about a directory of sources, and the case §14 describes --
    // jumping into a library checkout -- is often asked from a repository that
    // is not a Maven project at all.
    let root = crate::generate::find_project_root()
        .or_else(|_| std::env::current_dir().map_err(|e| format!("failed to get cwd: {e}")))?;
    let found = search(&root, type_name);
    if found.is_empty() {
        return Err(format!(
            "no source for `{type_name}` under this project{}.\n       \
             fix: check the spelling, or point JAILS_SOURCE_PATH at a directory holding \
             the library's checked-out source.",
            match std::env::var("JAILS_SOURCE_PATH") {
                Ok(path) if !path.is_empty() => format!(" or {path}"),
                _ => String::new(),
            }
        ));
    }
    if json {
        let items = found
            .iter()
            .map(|hit| {
                format!(
                    "{{\"type\":{},\"path\":{}}}",
                    crate::json::string(&hit.qualified),
                    crate::json::string(&hit.path.to_string_lossy())
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!("{{\"schema_version\":1,\"matches\":[{items}]}}");
        return Ok(());
    }
    for hit in &found {
        println!("{}  {}", hit.qualified, hit.path.display());
    }
    Ok(())
}

/// Every `.java` file declaring `type_name`, project first.
///
/// Sorted within each root so two runs on one machine agree, and the project's
/// own sources come first because a type you own shadowing a library type is
/// almost always the one you meant.
pub fn search(root: &Path, type_name: &str) -> Vec<Found> {
    let mut found = Vec::new();
    for dir in roots(root) {
        let mut here = Vec::new();
        collect(&dir, type_name, &mut here);
        here.sort_by(|a, b| a.qualified.cmp(&b.qualified));
        found.extend(here);
    }
    found
}

fn roots(root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![root.join("src/main/java"), root.join("src/test/java")];
    match std::env::var_os("JAILS_SOURCE_PATH") {
        Some(value) if !value.is_empty() => dirs.extend(std::env::split_paths(&value)),
        // `deps` beside the project is the default only because it is what
        // this was built against; it is not searched when the variable says
        // otherwise, so "somewhere else entirely" is one export away.
        _ => dirs.push(root.join("deps")),
    }
    dirs
}

fn collect(dir: &Path, type_name: &str, out: &mut Vec<Found>) {
    let wanted = format!("{type_name}.java");
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // A build output directory holds generated copies of sources
                // that are already on this list, so it would double every hit.
                if !matches!(
                    path.file_name().and_then(|n| n.to_str()),
                    Some("target" | "build" | ".git")
                ) {
                    stack.push(path);
                }
            } else if path.file_name().is_some_and(|name| name == wanted.as_str()) {
                let qualified = match package_of(&path) {
                    Some(package) => format!("{package}.{type_name}"),
                    None => type_name.to_string(),
                };
                out.push(Found { qualified, path });
            }
        }
    }
}

/// The `package` declaration, read rather than derived from the path.
///
/// A checkout's directory layout does not always match its packages -- a Maven
/// module nests them under `src/main/java`, a Gradle one may not -- and the
/// declaration is the only thing that is always right.
fn package_of(path: &Path) -> Option<String> {
    let source = std::fs::read_to_string(path).ok()?;
    source.lines().find_map(|line| {
        line.trim()
            .strip_prefix("package ")
            .map(|rest| rest.trim_end_matches(';').trim().to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        let root = jails_support::scratch::ScratchDir::in_temp(&format!("jails-src-{tag}"))
            .unwrap()
            .keep();
        let _ = fs::remove_dir_all(&root);
        root
    }

    fn write(path: &Path, package: &str, name: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("package {package};\n\nclass {name} {{}}\n")).unwrap();
    }

    #[test]
    fn a_type_is_reported_with_the_package_it_declares() {
        let root = scratch("one");
        write(
            &root.join("src/main/java/com/example/demo/domain/Note.java"),
            "com.example.demo.domain",
            "Note",
        );

        let found = search(&root, "Note");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].qualified, "com.example.demo.domain.Note");
    }

    /// The reason this lists rather than picks: a project with three
    /// `Status.java` files is ordinary, and choosing one silently sends an
    /// editor to the wrong file.
    #[test]
    fn two_types_with_one_simple_name_are_both_reported() {
        let root = scratch("two");
        write(
            &root.join("src/main/java/com/example/a/Status.java"),
            "com.example.a",
            "Status",
        );
        write(
            &root.join("src/main/java/com/example/b/Status.java"),
            "com.example.b",
            "Status",
        );

        let found = search(&root, "Status");

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].qualified, "com.example.a.Status");
        assert_eq!(found[1].qualified, "com.example.b.Status");
    }

    /// `target/` holds generated copies of sources already on the list.
    #[test]
    fn build_output_is_not_searched() {
        let root = scratch("target");
        write(
            &root.join("src/main/java/com/example/Note.java"),
            "com.example",
            "Note",
        );
        write(
            &root.join("src/main/java/target/generated/com/example/Note.java"),
            "com.example",
            "Note",
        );

        assert_eq!(search(&root, "Note").len(), 1);
    }

    #[test]
    fn a_name_nothing_declares_is_simply_absent() {
        let root = scratch("missing");
        fs::create_dir_all(root.join("src/main/java")).unwrap();
        assert!(search(&root, "Nowhere").is_empty());
    }
}
