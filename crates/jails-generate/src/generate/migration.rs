//! `generate migration` and `generate cases`: the two kinds whose NAME is
//! not a Java class.
//!
//! A migration name is a description and a cases name is a markdown path, so
//! both are handled before the shared capitalisation every other kind gets.
//! Migrations are forward-only and deliberately cannot be destroyed.

use super::*;

// ---- migration: the next forward-only SQL file. ----

/// Where a migration with this description goes.
///
/// **The one already there, if there is one.** A migration's file name carries
/// a serial number, and deriving that number from a count meant re-planning an
/// entity that already exists produced a *different* path -- so a second `app
/// apply` wrote `V005__create_crawl_runs.sql` beside the `V001` it had written
/// the first time. Matching on the description makes the path a function of
/// the entity rather than of how many times it has been planned.
///
/// The listing is the projection, not disk, so two rows of one transition
/// cannot both take the next number.
pub(crate) fn migration_file(project: &Project, description: &str) -> Result<std::path::PathBuf> {
    const DIR: &str = "src/main/resources/db/migration";
    let names = project.projected_names_in(DIR);
    let suffix = format!("__{description}.sql");
    let existing = names
        .iter()
        .filter(|name| name.ends_with(&suffix))
        .filter_map(|name| migration_version(name).map(|version| (version, name)))
        .max_by_key(|(version, _)| *version);
    let dropped_after = description.strip_prefix("create_").is_some_and(|table| {
        let drop_suffix = format!("__drop_{table}.sql");
        let last_drop = names
            .iter()
            .filter(|name| name.ends_with(&drop_suffix))
            .filter_map(|name| migration_version(name))
            .max();
        last_drop.is_some_and(|drop| existing.is_some_and(|(create, _)| drop >= create))
    });
    if let Some((_, existing)) = existing.filter(|_| !dropped_after) {
        return Ok(project.root().join(DIR).join(existing));
    }
    let mut highest = 0;
    for name in &names {
        if let Some(version) = migration_version(name) {
            highest = highest.max(version);
        }
    }
    let version = highest
        .checked_add(1)
        .ok_or_else(|| "migration version overflow".to_string())?;
    Ok(project
        .root()
        .join(DIR)
        .join(format!("V{version:03}__{description}.sql")))
}

fn migration_version(name: &str) -> Option<u32> {
    name.strip_prefix('V')
        .and_then(|rest| rest.split_once("__").map(|(version, _)| version))
        .or_else(|| name.split_once('_').map(|(version, _)| version))
        .and_then(|value| value.parse::<u32>().ok())
}

/// Plan the forward-only migration that retires one generated table.
/// Confirmation and dependency evidence belong to the lifecycle route; this
/// renderer owns only the deterministic path and SQL bytes.
pub fn drop_table_change(project: &Project, table: &str) -> Result<Change> {
    let path = migration_file(project, &format!("drop_{table}"))?;
    Ok(Change {
        files: vec![Artifact {
            kind: "migration",
            path,
            contents: format!("drop table {table};\n"),
        }],
        ..Change::default()
    })
}

/// The forward migration that retires an association's foreign key.
///
/// Appending `drop constraint` is not un-running the migration that added it:
/// it is the next one, exactly as `drop table` is the next one after `create
/// table`. Refusing the whole verb because "associations are forward-only"
/// conflated the two and left a hard deadlock -- neither half of an
/// association could be destroyed, because removing either entity was refused
/// for pointing at the other, and the refusal's own `fix:` named a command
/// that refused on principle.
///
/// `if exists` because the reader may already have dropped it by hand while
/// looking for a way out of that deadlock, and a migration that fails on the
/// second attempt is a worse answer than one that is idempotent.
pub fn drop_constraint_change(
    project: &Project,
    table: &str,
    constraint: &str,
    slug: &str,
) -> Result<Change> {
    let path = migration_file(project, &format!("drop_{slug}_association"))?;
    Ok(Change {
        files: vec![Artifact {
            kind: "migration",
            path,
            contents: format!("alter table {table}\n  drop constraint if exists {constraint};\n"),
        }],
        ..Change::default()
    })
}

/// Plan the one forward PostgreSQL migration for a coordinated physical
/// table cutover. Both identifiers have already crossed the protocol's
/// `SqlName` boundary, so quoting them here is deterministic and cannot turn
/// caller input into an SQL fragment.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StorageObjectKind {
    Constraint,
    Index,
    Sequence,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct StorageObjectRename {
    pub kind: StorageObjectKind,
    pub current: jails_protocol::identity::SqlName,
    pub target: jails_protocol::identity::SqlName,
}

pub fn rename_table_change(
    project: &Project,
    current: &str,
    target: &str,
    objects: &[StorageObjectRename],
) -> Result<Change> {
    if current == target {
        return Err("a table rename needs distinct current and target names.\n       fix: preserve the current table or choose a different target".into());
    }
    let path = migration_file(project, &format!("rename_{current}_to_{target}"))?;
    let mut contents = format!("alter table public.\"{current}\" rename to \"{target}\";\n");
    for object in objects {
        match object.kind {
            StorageObjectKind::Constraint => contents.push_str(&format!(
                "alter table public.\"{target}\" rename constraint \"{}\" to \"{}\";\n",
                object.current.as_str(),
                object.target.as_str()
            )),
            StorageObjectKind::Index => contents.push_str(&format!(
                "alter index public.\"{}\" rename to \"{}\";\n",
                object.current.as_str(),
                object.target.as_str()
            )),
            StorageObjectKind::Sequence => contents.push_str(&format!(
                "alter sequence public.\"{}\" rename to \"{}\";\n",
                object.current.as_str(),
                object.target.as_str()
            )),
        }
    }
    Ok(Change {
        files: vec![Artifact {
            kind: "migration",
            path,
            contents,
        }],
        ..Change::default()
    })
}

pub fn next_migration_version(dir: &Path) -> Result<u32> {
    if !dir.exists() {
        return Ok(1);
    }
    let entries =
        fs::read_dir(dir).map_err(|e| format!("failed to read {}: {e}", dir.display()))?;
    let mut highest = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(version) = migration_version(&name) {
            highest = highest.max(version);
        }
    }
    Ok(highest
        .checked_add(1)
        .ok_or_else(|| "migration version overflow".to_string())?)
}

pub fn sql_name(value: &str) -> Result<String> {
    let mut out = String::new();
    let mut previous_was_lower_or_digit = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && previous_was_lower_or_digit && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            previous_was_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else if matches!(ch, '-' | '_' | ' ') {
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            previous_was_lower_or_digit = false;
        } else {
            return Err(format!("'{value}' is not a usable SQL migration name").into());
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        Err(jails_support::Failure::Told(
            "a migration needs a description, e.g. `jails g migration create_rewards`".to_string(),
        ))
    } else {
        Ok(out)
    }
}

// ---- cases: a markdown checklist in, a pending JUnit class out. ----

/// Turn a brief's checklist into a `@Disabled` test class -- the todo list you
/// delete one `@Disabled` at a time.
/// What `generate cases` intends, computed without writing anything.
///
/// The same split `plan_recipe` makes for the persistent kinds, and for the
/// same reason: §R6.2 turns this into a one-shot receipt plus one file, and
/// that is only possible if what it intends can be computed apart from being
/// carried out. The brief itself is read here, because its bytes are what the
/// receipt hashes.
///
/// `brief` is **project-relative**, and stays that way: it is read from under
/// the project root and it is the spelling the generated Javadoc names. A
/// path resolved against the process working directory would make the same
/// command produce two different files depending on which subdirectory it was
/// typed in, and the receipt would record whichever one happened to win.
pub fn plan_cases(project: &Project, pkg: &str, brief: &Path) -> Result<(Change, String)> {
    let at = project.root().join(brief);
    let text =
        fs::read_to_string(&at).map_err(|e| format!("failed to read {}: {e}", brief.display()))?;
    let cases = parse_cases(&text);
    if cases.is_empty() {
        return Err(format!(
            "no list items found in {} -- `generate cases` turns markdown bullets into test cases",
            brief.display()
        )
        .into());
    }
    let class = cases_class_name(brief)?;
    let path = test_dir(project.root(), pkg).join(format!("{class}.java"));
    Ok((
        Change {
            files: vec![Artifact {
                kind: "cases",
                path,
                contents: cases_java(pkg, &class, brief, &cases),
            }],
            ..Change::default()
        },
        text,
    ))
}

/// Bullets under an acceptance/criteria/cases/checklist heading if the brief
/// has one, otherwise every bullet in the file.
///
/// Deliberately the whole of the markdown support: a heading and a bullet. The
/// moment this grows a second rule it starts being a markdown parser, and that
/// is not what jails is.
pub(super) fn parse_cases(markdown: &str) -> Vec<String> {
    let scoped = cases_section(markdown);
    let source = scoped.as_deref().unwrap_or(markdown);

    let mut cases = Vec::new();
    let mut in_fence = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(item) = list_item(trimmed) {
            let cleaned = clean_markdown(item);
            if !cleaned.is_empty() {
                cases.push(cleaned);
            }
        }
    }
    cases
}

/// The body under the first heading that looks like a list of expectations,
/// up to the next heading of the same or a higher level.
pub(super) fn cases_section(markdown: &str) -> Option<String> {
    const MARKERS: [&str; 5] = [
        "acceptance",
        "criteria",
        "cases",
        "checklist",
        "requirements",
    ];

    let mut lines = markdown.lines().enumerate();
    let (start, level) = loop {
        let (i, line) = lines.next()?;
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#') {
            continue;
        }
        let level = trimmed.chars().take_while(|c| *c == '#').count();
        let title = trimmed[level..].to_lowercase();
        if MARKERS.iter().any(|m| title.contains(m)) {
            break (i + 1, level);
        }
    };

    let body: Vec<&str> = markdown
        .lines()
        .skip(start)
        .take_while(|line| {
            let trimmed = line.trim_start();
            let depth = trimmed.chars().take_while(|c| *c == '#').count();
            depth == 0 || depth > level
        })
        .collect();
    Some(body.join("\n"))
}

/// The content of a `-`/`*`/`1.` list item, checkbox marker stripped.
pub(super) fn list_item(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
        .or_else(|| {
            let digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
            (!digits.is_empty())
                .then(|| line[digits.len()..].strip_prefix(". "))
                .flatten()
        })?;
    let rest = rest.trim();
    // `- [ ]` / `- [x]` checkboxes: the box is not part of the case.
    let rest = rest
        .strip_prefix("[ ]")
        .or_else(|| rest.strip_prefix("[x]"))
        .or_else(|| rest.strip_prefix("[X]"))
        .unwrap_or(rest);
    Some(rest.trim())
}

/// Strip the inline markup that would otherwise end up inside a `@DisplayName`
/// string: emphasis, code ticks, and link syntax (keeping the link text).
pub(super) fn clean_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '_' | '`' | '#' => {}
            '[' => {}
            ']' => {
                // `](url)` -- drop the target, keep the text already emitted.
                if chars.peek() == Some(&'(') {
                    for skipped in chars.by_ref() {
                        if skipped == ')' {
                            break;
                        }
                    }
                }
            }
            '"' => out.push('\''),
            '\\' => out.push('/'),
            _ => out.push(c),
        }
    }
    out.trim().to_string()
}

/// `01-normalise.md` -> `Workout01NormaliseTest`? No -- `Normalise01Test` would
/// be a guess. The stem is turned into a class name verbatim (minus the
/// separators), with a leading `Case` when it starts with a digit, since a Java
/// identifier cannot.
pub(super) fn cases_class_name(brief: &Path) -> Result<String> {
    let stem = brief.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
        format!(
            "{} has no file name to derive a class from",
            brief.display()
        )
    })?;

    let mut class = String::new();
    let mut capitalize_next = true;
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() {
            if capitalize_next {
                class.extend(c.to_uppercase());
                capitalize_next = false;
            } else {
                class.push(c);
            }
        } else {
            capitalize_next = true;
        }
    }
    if class.is_empty() {
        return Err(format!("cannot derive a class name from {}", brief.display()).into());
    }
    if class.starts_with(|c: char| c.is_ascii_digit()) {
        class.insert_str(0, "Case");
    }
    if !class.ends_with("Test") {
        class.push_str("Test");
    }
    Ok(class)
}

/// A markdown bullet as a Java method name: camelCase, alphanumerics only.
pub(super) fn case_method_name(case: &str) -> String {
    let mut name = String::new();
    let mut capitalize_next = false;
    for c in case.chars() {
        if c.is_ascii_alphanumeric() {
            if capitalize_next && !name.is_empty() {
                name.extend(c.to_uppercase());
            } else if name.is_empty() {
                name.extend(c.to_lowercase());
            } else {
                name.push(c);
            }
            capitalize_next = false;
        } else {
            capitalize_next = true;
        }
    }
    if name.is_empty() {
        name.push_str("case");
    }
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert(0, 'c');
    }
    name
}

pub(super) fn cases_java(pkg: &str, class: &str, brief: &Path, cases: &[String]) -> String {
    let mut out = format!("package {pkg};\n\n");
    out += "import org.junit.jupiter.api.Disabled;\n";
    out += "import org.junit.jupiter.api.DisplayName;\n";
    out += "import org.junit.jupiter.api.Test;\n\n";
    out += "/**\n";
    out += &format!(" * Pending cases generated from {}.\n", brief.display());
    out += " *\n";
    out += " * <p>This is a todo list the build can read: every case fails loudly rather\n";
    out += " * than passing vacuously, and the class-level @Disabled keeps the suite green\n";
    out += " * meanwhile. Delete one @Disabled, make that case pass, move to the next.\n";
    out += " */\n";
    out += &format!(
        "@DisplayName(\"{}\")\n",
        clean_markdown(&brief.file_stem().unwrap_or_default().to_string_lossy())
    );
    out += "@Disabled(\"todo: implement these cases\")\n";
    out += &format!("class {class} {{\n");

    // Two bullets can easily reduce to the same identifier; a suffix keeps the
    // class compiling rather than silently dropping a case.
    let mut seen: Vec<String> = Vec::new();
    for case in cases {
        let base = case_method_name(case);
        let mut method = base.clone();
        let mut n = 2;
        while seen.contains(&method) {
            method = format!("{base}{n}");
            n += 1;
        }
        seen.push(method.clone());

        out += "\n    @Test\n";
        out += &format!("    @DisplayName(\"{case}\")\n");
        out += &format!("    void {method}() {{\n");
        out += "        throw new UnsupportedOperationException(\"todo\");\n";
        out += "    }\n";
    }
    out += "}\n";
    out
}
