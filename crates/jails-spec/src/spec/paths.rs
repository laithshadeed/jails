//! Where a project is, and where inside it a class goes.
//!
//! None of this is generation; it is the answer to "which directory am I in
//! and what is it called", which the generators then use. It sits below them
//! because `model::Project`, `config`, `compose` and `inspect` all ask, and a
//! back-edge from any of them into the generators is a cycle.

use jails_support::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Walk up from the current directory looking for a project root.
///
/// **Any** build marker, not only `pom.xml`: most of jails never touches
/// Maven, so refusing at the door refuses commands that would work. The
/// commands that do need Maven refuse themselves, through
/// `build::require_maven`, which is a refusal that can say what still works.
///
/// Nearest wins, so a Gradle sub-module inside a Maven reactor resolves to the
/// sub-module.
pub fn find_project_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().map_err(|e| format!("failed to get cwd: {e}"))?;
    loop {
        if crate::build::detect(&dir) != crate::build::Build::Bare {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(jails_support::Failure::Told(
                "no pom.xml (or build.gradle, settings.gradle, build.xml, BUILD.bazel) \
                 in this or any parent directory"
                    .to_string(),
            ));
        }
    }
}

/// Read the package line off the project's `*Application.java` entry point
/// rather than configuring it.
pub fn base_package(root: &Path) -> Result<String> {
    let src_root = root.join("src/main/java");
    // Spring projects have a *Application.java entry point; `new-cli` ones
    // have App.java, so fall back to whatever source file sits closest to the
    // source root rather than failing on plain Maven projects.
    let entry = find_application_file(&src_root)
        .or_else(|| shallowest_java_file(&src_root))
        .ok_or_else(|| {
            // **Say that this is not a project, not that one file is
            // missing.** The commonest way to reach here is typing `jails g`
            // in the wrong directory, and "no .java file under
            // src/main/java" reads as a project with a gap rather than as no
            // project at all -- so the reader goes looking for the file
            // instead of for their project.
            "this directory is not a Java project: jails reads the base package off the shallowest source under `src/main/java`, and there is none\n       fix: run this inside a project, or create one with `jails new` / `jails new-cli`"
                .to_string()
        })?;
    let contents = fs::read_to_string(&entry)
        .map_err(|e| format!("failed to read {}: {e}", entry.display()))?;
    for line in contents.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("package ")
            && let Some(pkg) = rest.trim().strip_suffix(';')
        {
            return Ok(pkg.trim().to_string());
        }
    }
    Err(format!("no package declaration found in {}", entry.display()).into())
}

fn find_application_file(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_application_file(&path) {
                return Some(found);
            }
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("Application.java"))
        {
            return Some(path);
        }
    }
    None
}

/// The .java file with the fewest path segments below `dir`, i.e. the one in
/// the outermost package -- for a plain Maven project that is the base package
/// by construction.
fn shallowest_java_file(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(usize, PathBuf)> = None;
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((current, depth)) = stack.pop() {
        for entry in fs::read_dir(&current).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push((path, depth + 1));
            } else if path.extension().is_some_and(|e| e == "java") {
                let better = best.as_ref().is_none_or(|(d, _)| depth < *d);
                if better {
                    best = Some((depth, path));
                }
            }
        }
    }
    best.map(|(_, path)| path)
}

/// `com.example.demo` + `domain` -> `com.example.demo.domain`. An empty
/// subpackage leaves the base package alone.
pub fn subpackage(base: &str, sub: &str) -> String {
    if sub.is_empty() {
        base.to_string()
    } else {
        format!("{base}.{sub}")
    }
}

fn pkg_dir(pkg: &str) -> String {
    pkg.replace('.', "/")
}

pub fn main_dir(root: &Path, pkg: &str) -> PathBuf {
    root.join("src/main/java").join(pkg_dir(pkg))
}

pub fn test_dir(root: &Path, pkg: &str) -> PathBuf {
    root.join("src/test/java").join(pkg_dir(pkg))
}
