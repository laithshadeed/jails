//! Splicing `@Import(TestcontainersConfig.class)` into tests the reader owns.
//!
//! `add db` cannot skip this, and `CLAUDE.md` records why: once the JDBC
//! starter is present, auto-configuration demands a `DataSource` for **every**
//! `@SpringBootTest` -- including the `contextLoads` test that shipped with the
//! project, which nobody wrote and which now fails with "Failed to determine a
//! suitable driver class". Docker Compose is skipped in tests by default, so
//! there is no URL to find.
//!
//! The obvious fix is the one that was tried and backed out: an
//! `ApplicationContextInitializer` in test `META-INF/spring.factories` gave
//! every `@SpringBootTest` a DataSource for free **and** made every pure slice
//! and `@WebMvcTest` start a PostgreSQL it never queried. So the container is
//! an `@Import`ed `@TestConfiguration` instead, and the import has to be
//! spliced into classes that already exist -- including ones in other packages,
//! which need the import statement too.
//!
//! **Deleting a leftover `spring.factories` is not optional.** Left behind it
//! keeps registering the old initializer, a second container starts for every
//! test, and the migration looks like it did not work.
//!
//! Every edit here is surgical, because these are files the reader owns: the
//! annotation is rewritten member by member rather than replaced, and the
//! unsplice puts it back exactly as it was.

use super::*;
use jails_java::annotate::{
    import_annotation, is_spring_boot_test, splice_import, unsplice_import,
};

#[cfg(test)]
pub(super) fn spring_factories_block(fqcn: &str) -> String {
    crate::codemod::Marked::new("db").render(&format!("{SPRING_FACTORIES_KEY}={fqcn}\n"))
}

/// Import the container config into every `@SpringBootTest` in the project.
///
/// This is an edit to a file the user owns, which jails does sparingly and
/// only surgically: one annotation line above an anchor that is already
/// there, and the import statement it needs. It is idempotent -- a class that
/// already has the annotation is skipped, not duplicated.
///
/// Why `add db` does this at all rather than leaving it to the reader: the
/// moment `spring-boot-starter-jdbc` lands in the pom, JDBC auto-config
/// demands a DataSource for *every* `@SpringBootTest`, including the
/// `contextLoads` test that came with the project and never touches a
/// database. Adding the capability and walking away would break a test the
/// user did not write, with a message ("Failed to determine a suitable driver
/// class") that names neither the cause nor the fix.
///
/// Returns whether anything changed.
pub(super) fn install_test_container_import(
    root: &Path,
    cfg: &SpringTestImport,
    dry_run: bool,
) -> Result<bool> {
    let annotation = import_annotation(cfg.class);
    let mut changed = false;
    for path in find_spring_boot_tests(&root.join("src/test/java")) {
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        if source.contains(&annotation) {
            println!("  exists  {} in {}", cfg.class, rel(root, &path));
            continue;
        }
        let tests_pkg = package_of(&source).unwrap_or_else(|| cfg.pkg.clone());
        let extra = import_of(&tests_pkg, &cfg.pkg, cfg.class);
        let Some(next) = splice_import(&source, cfg.class, &extra) else {
            continue;
        };
        if dry_run {
            println!("  would import  {} into {}", cfg.class, rel(root, &path));
            changed = true;
            continue;
        }
        crate::apply::put(&path, next)?;
        println!("  import  {} -> {}", cfg.class, rel(root, &path));
        changed = true;
    }
    Ok(changed)
}

/// Remove the test-classpath `spring.factories` an earlier jails wrote.
///
/// Left in place it would register the old global initializer *as well as* the
/// new `@Import`, so every test would still start a container and the change
/// would look like it had not worked.
pub(super) fn remove_legacy_spring_factories(root: &Path) -> Result<bool> {
    let path = spring_factories_path(root);
    if !path.exists() {
        return Ok(false);
    }
    let existing =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    if !existing.contains(SPRING_FACTORIES_KEY) {
        return Ok(false);
    }
    let Some(next) = remove_jails_db_block(&existing, SPRING_FACTORIES_KEY) else {
        return Ok(false);
    };
    if next.trim().is_empty() {
        jails_support::apply::remove(&path)?;
        println!("  delete  {} (superseded by @Import)", rel(root, &path));
    } else {
        crate::apply::put(&path, next)?;
        println!("  unsplice  {}", rel(root, &path));
    }
    Ok(true)
}

pub(super) fn uninstall_postgres_test_initializer(
    root: &Path,
    cfg: &SpringTestImport,
) -> Result<()> {
    let path = spring_factories_path(root);
    if !path.exists() {
        return Ok(());
    }
    let existing =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let fqcn = cfg.fqcn();
    let Some(next) = remove_jails_db_block(&existing, &fqcn) else {
        return Ok(());
    };
    if next.trim().is_empty() {
        jails_support::apply::remove(&path)?;
        println!("  delete  {}", rel(root, &path));
    } else {
        crate::apply::put(&path, next)?;
        println!("  unsplice  {}", rel(root, &path));
    }
    Ok(())
}

pub(super) fn remove_jails_db_block(source: &str, fqcn: &str) -> Option<String> {
    let marked = crate::codemod::Marked::new("db");
    // Two removals, and both are needed: the block jails wrote, and any bare
    // line naming the initializer that an older jails left unmarked. A file
    // that has one and not the other still registers the initializer, and a
    // second container then starts for every test.
    let stripped = marked.strip_from(source);
    let text = stripped.clone().unwrap_or_else(|| source.to_string());
    let mut out = String::with_capacity(text.len());
    let mut dropped_a_line = false;
    for line in text.lines() {
        if line.trim().contains(fqcn) {
            dropped_a_line = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    (stripped.is_some() || dropped_a_line).then_some(out)
}

/// Drop `@Import(PostgresContainerConfig)` left by earlier jails versions.
pub(super) fn strip_legacy_postgres_imports(root: &Path, cfg: &SpringTestImport) -> Result<bool> {
    let mut changed = false;
    for path in find_spring_boot_tests(&root.join("src/test/java")) {
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        if !source.contains(&import_annotation(cfg.class)) {
            continue;
        }
        let tests_pkg = package_of(&source).unwrap_or_else(|| cfg.pkg.clone());
        let extra = import_of(&tests_pkg, &cfg.pkg, cfg.class);
        let Some(next) = unsplice_import(&source, cfg.class, &extra) else {
            continue;
        };
        crate::apply::put(&path, next)?;
        println!("  unsplice  {} from {}", cfg.class, rel(root, &path));
        changed = true;
    }
    Ok(changed)
}

pub(super) fn find_spring_boot_tests(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "java")
                && fs::read_to_string(&path).is_ok_and(|source| is_spring_boot_test(&source))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
