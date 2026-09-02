//! **How the counting works**, and nothing about what the numbers mean.
//!
//! [`Source`] is a production file with its comments, string literals and
//! `#[cfg(test)]` bodies blanked to spaces of the same length -- so byte
//! offsets and line numbers still index the original, which is what lets a gate
//! locate something in the blanked text and then read it from the raw file.
//!
//! Everything else here is one counting function per row of [`crate::board`],
//! plus the small Rust parser they share. Its unit tests are at the bottom,
//! colocated, because a parser whose tests live in another file is a parser
//! whose tests stop being run against it.

use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Measurement
// ---------------------------------------------------------------------------

pub(crate) struct Source {
    pub(crate) path: PathBuf,
    /// Comments and string literals replaced by spaces of the same length --
    /// so byte offsets and line counts still index the original -- with every
    /// `#[cfg(test)] mod … { … }` body also blanked.
    ///
    /// Every gate excludes `mod tests`: a fixture that writes a scratch
    /// `pom.xml` is not a production `fs::write`, and a test helper taking
    /// `root: &Path` is not the primitive being propagated. Counting them
    /// makes the ladder punish the tests that prove a rung did not change
    /// behaviour.
    pub(crate) production: String,
    /// The same file with comments and `#[cfg(test)]` bodies removed, but
    /// **string literals intact** -- for the gates whose subject only ever
    /// appears inside one. See [`keeping_literals`].
    pub(crate) literals: String,
}

/// Every production Rust file in the workspace, not only the binary's own.
///
/// A scanner that walks `src/` alone keeps reporting green while the code it
/// gates lives in `crates/*/src`.
///
/// Read and blanked **once per process**, and blanked in parallel. Every gate
/// shares this scan, and the memo is sound because a [`Source`] is immutable,
/// the files are not written while the binary runs, and every gate treats the
/// scan as a snapshot -- two gates disagreeing about the contents of the tree
/// would be a bug whichever way the scan was cached.
///
/// The blanking itself is a byte-at-a-time state machine per file with no
/// shared state, so it is spread across the same scheduler the table-driven
/// binaries use. The `assert` stays: a scanner that has lost the code reports
/// precisely what a clean one does, and caching a wrong answer once is worse
/// than recomputing it.
pub(crate) fn sources() -> &'static [Source] {
    static SOURCES: std::sync::OnceLock<Vec<Source>> = std::sync::OnceLock::new();
    SOURCES.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut paths = Vec::new();
        collect_paths(root.join("src").as_path(), &mut paths);
        let crates = root.join("crates");
        if crates.is_dir() {
            let mut members: Vec<PathBuf> = fs::read_dir(&crates)
                .expect("failed to read crates/")
                .map(|entry| entry.expect("failed to read a crates/ entry").path())
                .collect();
            members.sort();
            for member in members {
                let src = member.join("src");
                if src.is_dir() {
                    collect_paths(&src, &mut paths);
                }
            }
        }
        assert!(
            paths.len() > 30,
            "the workspace scanner found only {} files -- it has lost track of where \
             the code lives, and every gate below would report green over code it \
             never read",
            paths.len()
        );
        paths.sort();
        // Largest first: blanking is linear in file size and these differ by
        // two orders of magnitude, so starting the biggest file last would
        // leave every other worker waiting on it.
        crate::parallel::map_by_cost(
            &paths,
            |path| fs::metadata(path).map_or(0, |data| data.len()),
            |path| read_source(path),
        )
    })
}

/// One file, read and blanked. The unit the scan above is parallel over.
pub(crate) fn read_source(path: &Path) -> Source {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let production = without_test_modules(&blank(&source));
    let literals = without_test_modules_at(
        &keeping_literals(&source),
        &test_module_spans(&blank(&source)),
    );
    Source {
        path: path.to_path_buf(),
        production,
        literals,
    }
}

/// Every `.rs` file under `dir`, without reading any of them.
///
/// Separated from the read so the walk -- which is serial by nature, one
/// directory at a time -- stays off the parallel path and the reads, which
/// are not, go on it.
pub(crate) fn collect_paths(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("failed to read a directory entry").path();
        if path.is_dir() {
            collect_paths(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Replace comments and string literals with spaces of the same length.
///
/// The same trick as `java::blanked`, and for the same reason: a scan must
/// not be fooled by `// root: &Path` or by the word `fn` inside an inline Java
/// body, while offsets and line numbers still line up with the file on disk.
///
/// **Delimiters are matched on bytes, not on `&source[i..]`.** Inside a
/// comment or a string this walks one byte at a time, so `i` can sit in the
/// middle of a multi-byte character -- and `&source[i..]` panics there rather
/// than returning false. Every delimiter here is ASCII, so a byte comparison
/// is exactly equivalent and cannot panic.
pub(crate) fn blank(source: &str) -> String {
    blank_with(source, false)
}

/// The same walk, keeping string and character literals as they are.
///
/// Some gates count text that only ever appears *inside* a literal -- the
/// `# jails:` markers, the `"version"` JSON key, inline `r#"package ` bodies
/// -- and [`blank`] replaces every literal with spaces before they run, so a
/// gate reading blanked source reports zero whatever the code says: a scanner
/// that read nothing reports exactly what a clean one does.
///
/// Comments and `#[cfg(test)]` bodies are still removed, because a marker in
/// a doc comment is prose and one in a test is a fixture.
pub(crate) fn keeping_literals(source: &str) -> String {
    blank_with(source, true)
}

/// Replace comments -- and, unless `keep_literals`, string and character
/// literals -- with spaces of the same length.
fn blank_with(source: &str, keep_literals: bool) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    // A literal span is emitted verbatim or blanked, never half of each, so
    // offsets line up with the file on disk under both modes.
    let emit = |out: &mut String, span: &str| {
        if keep_literals {
            out.push_str(span);
        } else {
            // One space per *byte*, not per char: these gates assert that
            // blanking preserves length, and a `§` inside a raw string is two
            // bytes and one char. Getting that wrong is what
            // `blanked to a different length` exists to catch.
            for byte in span.bytes() {
                out.push(if byte == b'\n' { '\n' } else { ' ' });
            }
        }
    };
    while i < bytes.len() {
        let rest = &source[i..];
        if rest.starts_with("//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                out.push(' ');
                i += 1;
            }
        } else if rest.starts_with("/*") {
            let mut depth = 0;
            while i < bytes.len() {
                if bytes[i..].starts_with(b"/*") {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if bytes[i..].starts_with(b"*/") {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
        } else if rest.starts_with('r') && rest[1..].starts_with(['#', '"']) {
            // A raw string: r"..", r#".."#, r##".."##, and so on.
            let hashes = rest[1..].bytes().take_while(|b| *b == b'#').count();
            if rest[1 + hashes..].starts_with('"') {
                let close = format!("\"{}", "#".repeat(hashes));
                let open = 1 + hashes + 1;
                let mut end = i + open;
                while end < bytes.len() && !bytes[end..].starts_with(close.as_bytes()) {
                    end += 1;
                }
                let end = (end + close.len()).min(bytes.len());
                emit(&mut out, &source[i..end]);
                i = end;
            } else {
                out.push('r');
                i += 1;
            }
        } else if bytes[i] == b'"' {
            let mut end = i + 1;
            while end < bytes.len() && bytes[end] != b'"' {
                // A trailing backslash is Rust's line continuation, and eating
                // its newline would make every line count here read low --
                // which is the whole measurement for two gates.
                end += if bytes[end] == b'\\' && end + 1 < bytes.len() {
                    2
                } else {
                    1
                };
            }
            let end = (end + 1).min(bytes.len());
            emit(&mut out, &source[i..end]);
            i = end;
        } else if bytes[i] == b'\'' && char_literal_len(&source[i..]).is_some() {
            let len = char_literal_len(&source[i..]).expect("checked above");
            emit(&mut out, &source[i..i + len]);
            i += len;
        } else {
            let ch = source[i..].chars().next().expect("in bounds");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Blank the body of every `#[cfg(test)]` module, preserving offsets.
pub(crate) fn without_test_modules(blanked: &str) -> String {
    without_test_modules_at(blanked, &test_module_spans(blanked))
}

/// Blank the given spans, preserving offsets.
///
/// Split from the span search so the literal-preserving view can blank
/// *exactly* the regions the fully blanked one did. Finding them again in text
/// that still has its literals would not work: a `{` inside one of these test
/// fixtures' Java bodies is not a block, and the brace walk would stop in the
/// wrong place.
pub(crate) fn without_test_modules_at(text: &str, spans: &[(usize, usize)]) -> String {
    let mut out = text.to_string();
    for (at, close) in spans.iter().rev() {
        let blanked_body: String = text[*at..=*close]
            .chars()
            .map(|c| if c == '\n' { '\n' } else { ' ' })
            .collect();
        out.replace_range(at..=close, &blanked_body);
    }
    out
}

/// Where each `#[cfg(test)]` item's body starts and ends.
pub(crate) fn test_module_spans(blanked: &str) -> Vec<(usize, usize)> {
    let bytes = blanked.as_bytes();
    let mut spans = Vec::new();
    let mut search = 0;
    while let Some(offset) = blanked[search..].find("#[cfg(test)]") {
        let at = search + offset;
        search = at + "#[cfg(test)]".len();
        // Only a module has a body worth erasing; `#[cfg(test)]` on a single
        // helper fn is erased by the same brace walk, which is also correct.
        let Some(open) = blanked[search..].find('{').map(|i| search + i) else {
            break;
        };
        let mut depth = 0usize;
        let mut close = open;
        for (index, byte) in bytes.iter().enumerate().skip(open) {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = index;
                        break;
                    }
                }
                _ => {}
            }
        }
        if close <= open {
            break;
        }
        spans.push((at, close));
        search = close;
    }
    spans
}

/// The length of a `'a'`-style character literal, or `None` for a lifetime.
pub(crate) fn char_literal_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if bytes.first() != Some(&b'\'') {
        return None;
    }
    if bytes.get(1) == Some(&b'\\') {
        return rest[1..].find('\'').map(|end| end + 2);
    }
    let ch = rest[1..].chars().next()?;
    let end = 1 + ch.len_utf8();
    (bytes.get(end) == Some(&b'\'')).then_some(end + 1)
}

/// `root: &Path` in a **parameter** position, in either spelling.
///
/// `&std::path::Path` counts too. Measuring one spelling means a conversion
/// that removes the other reads as no progress at all, which is how a gate
/// quietly stops describing the thing it is named after.
///
/// Deliberately not a plain substring count: a recipe mid-migration binds
/// `let root: &Path = slice.project().root();` inside its body, which is the
/// primitive being *contained* rather than propagated. Counting those makes
/// the gate read flat while the parameter it exists to remove disappears -- a
/// measurement that cannot see the improvement it is asking for.
pub(crate) fn root_path_parameters(src: &[Source]) -> usize {
    src.iter()
        // The canonical workspace crate is the capture/apply boundary. Its
        // subject is deliberately a filesystem root; the pure compiler above
        // it receives a WorkspaceSnapshot and cannot name one. The ratchet
        // remains useful for code that should take `Project` instead.
        .filter(|file| !is_canonical_workspace(&file.path))
        .map(|file| {
            ["root: &Path", "root: &std::path::Path"]
                .iter()
                .flat_map(|spelling| file.production.match_indices(spelling))
                .filter(|(at, _)| !file.production[..*at].trim_end().ends_with("let"))
                // `module_root: &Path` and `workspace_root: &Path` are not
                // this parameter. Counting them makes `project.rs` -- which
                // walks a reactor and *must* read each pom along the way --
                // look like the disease.
                .filter(|(at, _)| {
                    file.production[..*at]
                        .chars()
                        .next_back()
                        .is_none_or(|before| !before.is_alphanumeric() && before != '_')
                })
                .count()
        })
        .sum()
}

/// Root-taking pom readers where reading again is the *correct* behaviour, and
/// passing the caller's `Project` would be the bug.
///
/// The distinction the rung is about: envy is asking the disk for a fact
/// somebody already resolved. These ask the disk because the resolved answer
/// is **absent** -- there is no project yet. Declared rather than counted,
/// because a number nobody can reach is a gate nobody reads, and a stale
/// entry here fails the test below.
pub(crate) const A_FRESH_READ_IS_CORRECT: &[(&str, &str)] = &[
    (
        "read_build_file",
        "it is the read a `Project` is *constructed from*, not a second one taken beside \
         it. Both `load` and `inspect` go through it precisely so there is one answer: \
         `inspect` reading `pom.xml` unconditionally while `load` had learned about \
         `build.gradle` is what made `doctor` report a Gradle project as having no build \
         file at all",
    ),
    (
        "read_source",
        "the model it is reading does not exist yet: a project with none reads as the \
         seed `model init` would write, and that seed is *derived from* the project -- \
         its package, its release, its build tool. There is no resolved `Project` to be \
         handed, which is the same absence `read_build_file` records",
    ),
    (
        "load_model",
        "same absence as `read_source`, one layer up: it parses what that returns, \
         and on a project with no model the source is the derived seed rather than a \
         file. Both halves are the construction, not a second read beside it",
    ),
];

/// Functions that exist *to* turn a path into project facts, so re-deriving is
/// their whole job rather than envy of someone else's.
///
/// `new` is here in full: it runs before a project exists to resolve, which is
/// the one situation where there is no `Project` to have been passed.
pub(crate) const DERIVATION_IS_THE_JOB: &[&str] = &[
    "load",
    "inspect",
    "base_package",
    "project_with_pom",
    "verify_requested_deps",
    "write_agents",
    "ensure_enforcer",
];

/// `(file, function)` for every `root: &Path` function that goes back to disk
/// for something a resolved `Project` already holds.
pub(crate) fn rederivers(src: &[Source]) -> Vec<(String, String)> {
    // Applied to `root` specifically. Without the argument it would count a
    // function that loads a `Project` for each of two *scratch* copies of the
    // tree -- the opposite of envy, since there is no resolved project for
    // those roots to have been passed.
    const FACTS: &[&str] = &[
        "pom::read(root)",
        "base_package(root)",
        "Project::load(root)",
        "Project::inspect(root)",
        "Config::load(root)",
    ];
    let mut found = Vec::new();
    for file in src {
        let lines: Vec<&str> = file.production.lines().collect();
        let mut index = 0;
        while index < lines.len() {
            let trimmed = lines[index].trim_start();
            if !trimmed.contains("fn ") {
                index += 1;
                continue;
            }
            let indent = lines[index].len() - trimmed.len();
            let close = format!("{}}}", " ".repeat(indent));
            let Some(end) = (index..lines.len()).find(|at| lines[*at] == close) else {
                index += 1;
                continue;
            };
            let body = lines[index..=end].join("\n");
            let signature = body.split('{').next().unwrap_or_default();
            let name = signature
                .split("fn ")
                .nth(1)
                .and_then(|rest| rest.split(['(', '<']).next())
                .unwrap_or_default()
                .to_string();
            let takes_root = signature.match_indices("root: &Path").any(|(at, _)| {
                signature[..at]
                    .chars()
                    .next_back()
                    .is_none_or(|before| !before.is_alphanumeric() && before != '_')
            });
            if takes_root
                && FACTS.iter().any(|fact| body.contains(fact))
                && !DERIVATION_IS_THE_JOB.contains(&name.as_str())
            {
                found.push((file.path.display().to_string(), name));
            }
            index = end + 1;
        }
    }
    found
}

/// Lines of a file that are neither blank nor inside a `#[cfg(test)]` module.
pub(crate) fn production_lines(file: &Source) -> usize {
    file.production
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
}

pub(crate) fn count_matches(src: &[Source], needle: &str) -> usize {
    src.iter()
        .map(|file| file.production.matches(needle).count())
        .sum()
}

/// Every `fn` in blanked Rust, with the number of top-level parameters.
pub(crate) fn fn_param_counts(blanked: &str) -> Vec<(String, usize)> {
    let bytes = blanked.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(offset) = blanked[i..].find("fn ") {
        let at = i + offset;
        let boundary = at == 0 || !is_ident(bytes[at - 1]);
        i = at + 3;
        if !boundary {
            continue;
        }
        let after = &blanked[i..];
        let name: String = after
            .trim_start()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        // Skip any generic parameter list, then find the argument list.
        let Some(open) = find_param_list(&blanked[i..]) else {
            continue;
        };
        let start = i + open;
        let Some(end) = matching_paren(blanked, start) else {
            continue;
        };
        out.push((name, top_level_commas(&blanked[start + 1..end])));
        i = end;
    }
    out
}

pub(crate) fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The offset of the `(` opening a function's parameter list, skipping any
/// generic parameters, which may themselves contain parentheses in a bound.
pub(crate) fn find_param_list(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut i = 0;
    let mut angle = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => angle += 1,
            b'>' => angle = angle.saturating_sub(1),
            b'(' if angle == 0 => return Some(i),
            b'{' | b';' if angle == 0 => return None,
            _ => {}
        }
        i += 1;
    }
    None
}

pub(crate) fn matching_paren(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    for (offset, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// The number of parameters in a blanked argument list.
///
/// Counted as non-empty top-level segments, not as commas plus one: Rust
/// permits a trailing comma, and every multi-line signature in this codebase
/// has one, so counting commas overstates every wrapped signature by exactly
/// one while leaving single-line ones correct. That is the worst shape of
/// measurement bug -- consistent enough to look right.
pub(crate) fn top_level_commas(inner: &str) -> usize {
    let mut depth = 0i32;
    let mut params = 0;
    let mut segment_has_content = false;
    for ch in inner.chars() {
        match ch {
            '(' | '[' | '<' | '{' => depth += 1,
            ')' | ']' | '>' | '}' => depth -= 1,
            ',' if depth == 0 => {
                if segment_has_content {
                    params += 1;
                }
                segment_has_content = false;
                continue;
            }
            _ => {}
        }
        if !ch.is_whitespace() {
            segment_has_content = true;
        }
    }
    params + usize::from(segment_has_content)
}

/// A declaration of `keyword`, at any visibility.
///
/// A scanner matching only some visibilities reports zero over code it cannot
/// see -- an improvement that has not happened -- the moment a
/// `pub(crate) struct` becomes a `pub struct`.
pub(crate) fn is_item(line: &str, keyword: &str) -> bool {
    let rest = line
        .strip_prefix("pub(crate) ")
        .or_else(|| line.strip_prefix("pub(super) "))
        .or_else(|| line.strip_prefix("pub "))
        .unwrap_or(line);
    rest.starts_with(keyword) && rest[keyword.len()..].starts_with(' ')
}

pub(crate) fn body_carrying_structs(src: &[Source]) -> usize {
    let mut found = 0;
    for file in src {
        let mut in_struct = false;
        for line in file.production.lines() {
            let trimmed = line.trim();
            if is_item(trimmed, "struct") {
                in_struct = trimmed.ends_with('{');
                continue;
            }
            if in_struct {
                if trimmed == "}" {
                    in_struct = false;
                } else if trimmed.contains("contents: String") || trimmed.contains("body: String") {
                    found += 1;
                    in_struct = false;
                }
            }
        }
    }
    found
}

/// Positional `(PathBuf, String, ..)` tuples standing in for `model::Artifact`.
pub(crate) fn file_tuple_types(src: &[Source]) -> usize {
    src.iter()
        .map(|file| {
            file.production.matches("(PathBuf, String").count()
                + file
                    .production
                    .matches("(std::path::PathBuf, String")
                    .count()
        })
        .sum()
}

/// `type X = Change;`-style aliases for the one shared shape.
pub(crate) fn type_aliases(src: &[Source]) -> usize {
    src.iter()
        .map(|file| {
            file.production
                .lines()
                .filter(|line| {
                    let line = line.trim();
                    is_item(line, "type")
                })
                .filter(|line| line.contains("= Change;") || line.contains("= Artifact;"))
                .count()
                + file.production.matches("Artifact as NewFile").count()
                + file.production.matches("Change as Plan").count()
        })
        .sum()
}

/// The bypass count reads a literal, so the literal must be the only spelling.
///
/// `use jails_support::apply::put;` would let a call be written as bare `put(`,
/// which `executor_bypasses` cannot see -- and a gate that can be stepped around
/// by an import is the failure mode this whole file exists to prevent, arriving
/// through the door it was built to watch.
#[test]
pub(crate) fn no_bare_apply_verb_imports() {
    let offenders: Vec<_> = sources()
        .iter()
        .filter(|file| {
            file.production.lines().any(|line| {
                let line = line.trim();
                line.starts_with("use ") && (line.contains("apply::{") || line.contains("apply::*"))
            })
        })
        .map(|file| file.path.display().to_string())
        .collect();
    assert!(
        offenders.is_empty(),
        "these files import `apply`'s verbs by name, so their writes are invisible to the \
         `mutations that bypass the executor` gate. Spell the call `apply::put(..)` in full:\n  {}",
        offenders.join("\n  ")
    );
}

/// Production files that parse Maven's XML with a scanner of their own.
///
/// **Parsers, not emitters.** A file that mentions `<dependency>` while only
/// ever *building* one is not claiming to understand a build file -- the bar
/// is "answer exactly or refuse, never guess", and that is a bar on reading.
/// Counting the emitters would put files in a row about parsing and make the
/// number stop meaning what its name says.
///
/// The count is deliberately **not** at one yet. Two of these are the
/// strangler migration itself -- `jails-project/src/pom.rs` is the path being
/// replaced, `jails-workspace/src/documents.rs` the backend replacing it --
/// so both exist on purpose until the cutover. What this row is for is that a
/// *third* answer cannot appear while that is happening, which is the failure
/// a migration invites: during one, every file has a reason to be special.
pub(crate) fn maven_xml_parsers(src: &[Source]) -> usize {
    const ELEMENTS: [&str; 7] = [
        "<dependency>",
        "<artifactId>",
        "<groupId>",
        "<plugin>",
        "<dependencyManagement>",
        "<build>",
        "<plugins>",
    ];
    const READS: [&str; 9] = [
        ".contains(",
        ".find(",
        ".rfind(",
        ".match_indices(",
        ".splitn(",
        ".starts_with(",
        ".ends_with(",
        ".strip_prefix(",
        ".strip_suffix(",
    ];
    src.iter()
        .filter(|file| {
            let text = &file.literals;
            ELEMENTS
                .iter()
                .filter(|element| text.contains(*element))
                .count()
                >= 2
                && READS.iter().any(|read| {
                    text.match_indices(read).any(|(at, _)| {
                        // The argument is a string literal naming an element.
                        // `<` need not be its first byte: the scanners match
                        // on indented fragments (`"    <dependency>"`) and on
                        // closing tags, so requiring it at position zero would
                        // find none of them.
                        let rest = &text[at + read.len()..];
                        let argument = rest.trim_start();
                        argument.starts_with('"')
                            && argument[1..]
                                .split('"')
                                .next()
                                .is_some_and(|literal| literal.contains('<'))
                    })
                })
        })
        .count()
}

/// The biggest table of per-builtin knowledge written anywhere but the row.
///
/// Every builtin type has one semantics row. A builtin described by several
/// separate matches -- the token, its aliases, the Java type, the import, the
/// sample, the Postgres column, which literals may default it -- is described
/// by matches that are each exhaustive over the enum and silent about the
/// others, so adding a builtin compiles in every one of them and is wrong in
/// most.
///
/// Counted as the largest number of *distinct* variants named inside one
/// function, because that is what a table is. A handful of scattered
/// references to two or three builtins is ordinary code -- `derive_default`
/// asking whether a key is a `Uuid` is not a second registry -- so the number
/// is a ceiling on table-shaped knowledge rather than on mentioning the enum.
pub(crate) fn largest_builtin_table(src: &[Source]) -> usize {
    src.iter()
        .filter(|file| !file.path.ends_with(BUILTIN_RS))
        .map(|file| {
            let text = &file.production;
            let mut worst = 0;
            for (start, _) in text.match_indices("fn ") {
                let Some(open) = text[start..].find('{').map(|at| start + at) else {
                    continue;
                };
                let mut depth = 0usize;
                let mut end = open;
                for (offset, byte) in text[open..].bytes().enumerate() {
                    match byte {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                end = open + offset;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let mut variants = std::collections::BTreeSet::new();
                for (at, _) in text[open..end].match_indices("BuiltinType::") {
                    let rest = &text[open + at + "BuiltinType::".len()..];
                    let name: String = rest
                        .chars()
                        .take_while(|character| character.is_alphanumeric() || *character == '_')
                        .collect();
                    if name.starts_with(|character: char| character.is_ascii_uppercase()) {
                        variants.insert(name);
                    }
                }
                worst = worst.max(variants.len());
            }
            worst
        })
        .max()
        .unwrap_or(0)
}

/// The row itself, excluded from the count above for the reason `codemod.rs`
/// is excluded from the marked-block row: the one legitimate owner of a format
/// is not a violation of having one owner.
pub(crate) const BUILTIN_RS: &str = "jails-model/src/builtin.rs";

/// One: `jails-workspace/src/documents/pom.rs`, the surviving backend.
///
/// `documents.rs`, `documents/build_feature.rs` and `capture/observe.rs` each
/// answered Maven questions of their own and now ask it, and
/// `jails-project/src/pom.rs` -- the reader it replaced -- is gone. The row
/// stays a ratchet because during the migration every file had a reason to be
/// special, and that is exactly when a fifth answer appears.
pub(crate) const MAVEN_XML_PARSERS: usize = 1;

/// `# jails:` marker literals outside the crate that owns the format.
///
/// Counted over `Source::literals`: in `Source::production` every string
/// literal is blanked to spaces before a gate sees it, and a marker only ever
/// appears inside one, so a gate reading blanked source claims zero whether
/// or not the line holds. Knowledge of the format outside `jails-codemod`
/// goes through it rather than being spelled: `Marked::present_in` instead of
/// a substring probe (`contains("# jails:db")` reads `# jails:dbx` as this
/// block), `compose::declares` instead of a copy of the two-space indent,
/// `Marked::OPEN_PREFIX` quoted in a refusal rather than retyped, so a message
/// about the format cannot outlive the format. `jails setup`'s note in
/// `~/.testcontainers.properties` says `# jails --`, because `# jails:` is how
/// a block opens and that file has none.
///
/// The gate can fail: adding a `# jails:` literal to any production file
/// outside `jails-codemod` moves it off zero.
pub(crate) const MARKED_BLOCK_LITERALS: usize = 0;

/// Where the count stands: `context_value`'s four-arm lowering, which is a
/// rendering rather than a registry.
pub(crate) const LARGEST_BUILTIN_TABLE: usize = 4;

/// The crates that must stay pure once the workspace has been captured.
///
/// After capture, no compiler module may access `std::fs`, a project root or
/// a process runner. The whole argument for a capture pass is that planning
/// becomes a function -- the same
/// snapshot and the same request yield the same plan, which is what makes a
/// plan diffable, cacheable and safe to show before it is applied. One
/// `fs::read` inside a pass is enough to break that, and nothing about the
/// resulting bug says so: it just means the plan depended on a file nobody
/// recorded reading.
pub(crate) const PURE_COMPILER_CRATES: [&str; 3] =
    ["jails-compiler", "jails-model", "jails-contracts"];

/// A pure crate reaching for the world outside the snapshot it was handed.
///
/// Gated at zero because a purity property is cheap to keep and expensive to
/// recover.
///
/// The `root: &Path` half is counted here too, through the same function the
/// workspace-wide row uses: a path a pass could resolve against is a project
/// root whether or not it reads one yet, and the workspace row's ceiling
/// would absorb a rise inside these three crates without noticing.
pub(crate) fn compiler_reaches_outside_the_snapshot(src: &[Source]) -> usize {
    let pure: Vec<Source> = src
        .iter()
        .filter(|file| {
            PURE_COMPILER_CRATES.iter().any(|name| {
                file.path
                    .to_string_lossy()
                    .contains(&format!("{name}/src/"))
            })
        })
        .map(|file| Source {
            path: file.path.clone(),
            production: file.production.clone(),
            literals: file.literals.clone(),
        })
        .collect();
    let reaches: usize = pure
        .iter()
        .map(|file| {
            [
                "std::fs",
                "fs::read",
                "fs::write",
                "Command::new",
                "std::process",
                "current_dir",
            ]
            .iter()
            .map(|needle| file.production.matches(needle).count())
            .sum::<usize>()
        })
        .sum();
    reaches + root_path_parameters(&pure)
}

/// The other files a gate here names. Paths for the same reason: a second
/// `doctor.rs` or `codemod.rs` anywhere in the workspace would silently join or
/// leave the set its gate measures, and the gate would report a number about a
/// different file without saying so.
pub(crate) const CODEMOD_RS: &str = "jails-codemod/src/marked.rs";

/// The one module allowed to know that `git merge-file` takes a diff
/// algorithm. See the board row that counts the literal elsewhere.
pub(crate) const GIT_RS: &str = "jails-support/src/git.rs";
pub(crate) const DOCTOR_RS: &str = "jails-report/src/doctor.rs";
pub(crate) const SCRATCH_RS: &str = "jails-support/src/scratch.rs";

/// Where the count stands. Lowered per message, never by a sweep: a refusal
/// gains a `fix:` line when it has a real next step -- the exact valid
/// spelling to retry, the upgrade path, "this is a bug in jails, not something
/// a project can cause" -- and a duplicate refusal is deleted with the
/// duplicate code that builds it.
///
/// 110 -> 88: `docs/51-kernel.md` S51.4 deleted the codec, and most of what it
/// took with it were exactly the refusals with no next step to name -- a
/// corrupt tag, a length over its cap, a set whose keys did not arrive sorted.
pub(crate) const REFUSALS_WITHOUT_A_FIX: usize = 88;

/// A refusal that builds a message and does not say what to do next.
///
/// Located on the blanked production text -- so `#[cfg(test)]` bodies are out
/// and parentheses inside string literals cannot confuse the scan -- and then
/// *read* from the raw file at the same byte offsets, because the message is
/// exactly what blanking erases.
///
/// Only calls whose argument contains a string literal count. `Err(error)`,
/// `Err(Failure::Reported)` and `Err(CommitError::Io(..))` are forwarding a
/// refusal somebody else worded, and a `fix:` is that somebody's job.
pub(crate) fn refusals_without_a_fix(src: &[Source]) -> usize {
    let mut count = 0;
    for file in src {
        // This free-text heuristic cannot see structured diagnostics: every
        // semantic diagnostic in jails-model has a mandatory fix, while
        // internal compiler/executor errors are intentionally not decorated as
        // user advice. The ratchet holds the free-text vocabulary; `rules.rs`
        // enforces the structured contract.
        if is_canonical_new_world(&file.path) {
            continue;
        }
        let raw = fs::read_to_string(&file.path).expect("this file was read once already");
        if raw.len() != file.production.len() {
            // Blanking preserves length; if it ever stops, this gate is reading
            // the wrong bytes and should say so rather than report a number.
            panic!("{} blanked to a different length", file.path.display());
        }
        let bytes = file.production.as_bytes();
        for (at, _) in file.production.match_indices("Err(") {
            // `.map_err(` and similar are not refusal sites of their own.
            if at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_') {
                continue;
            }
            let Some(end) = matching_paren(&file.production, at + 3) else {
                continue;
            };
            let argument = &raw[at + 4..end];
            if !argument.contains('"') {
                continue;
            }
            if !argument.contains("fix:") {
                count += 1;
            }
        }
    }
    count
}

fn is_canonical_workspace(path: &Path) -> bool {
    path.to_string_lossy()
        .replace('\\', "/")
        .contains("/crates/jails-workspace/src/")
}

fn is_canonical_new_world(path: &Path) -> bool {
    let path = path.to_string_lossy().replace('\\', "/");
    path.contains("/crates/jails-model/src/")
        || path.contains("/crates/jails-contracts/src/")
        || path.contains("/crates/jails-compiler/src/")
        || path.contains("/crates/jails-workspace/src/")
        || path.ends_with("/src/model_command.rs")
        || path.ends_with("/src/model_eject.rs")
        // The module *and its children*: `model_generate/render.rs` is the
        // same module, and matching only the basename would count its
        // refusals under a different row.
        || path.ends_with("/src/model_generate.rs")
        || path.contains("/src/model_generate/")
        || path.ends_with("/src/model_import.rs")
        || path.ends_with("/src/model_setting.rs")
}

pub(crate) fn write_sites_outside_apply(src: &[Source]) -> usize {
    mutation_sites(src, &["fs::write"])
}

/// Where the count stands: zero. Each write is decided rather than moved:
/// `jails new`'s writers take an `apply::Tree` instead of a `&Path`, so a
/// write outside the staging tree is refused; the `pom.xml` splice for
/// `test --fast` is a transition; and build output and machine state go
/// through `remove_derived`, `ensure_derived_directory` and
/// `ensure_directory_outside_project`, the first two of which **refuse** a
/// path outside `target/` or `build/` -- so the exemption below is checked
/// rather than promised.
pub(crate) const MUTATION_CEILING: usize = 0;

/// Every API that changes the filesystem, wherever it is spelled.
///
/// A gate that bans only literal `fs::write` reads green while `write`,
/// `OpenOptions` write modes, `remove_file`/`remove_dir`, `copy`, `rename`,
/// hard links, directory creation and permissions mutate the project under
/// other names.
pub(crate) const MUTATION_APIS: &[&str] = &[
    "fs::write",
    "fs::remove_file",
    "fs::remove_dir",
    "fs::remove_dir_all",
    "fs::copy",
    "fs::rename",
    "fs::hard_link",
    "fs::create_dir",
    "fs::create_dir_all",
    "fs::set_permissions",
    "set_len(",
    "create_new(true)",
];

/// Writes that reach the filesystem outside a transaction.
///
/// `apply::*` is the *write layer*, not the executor. It is one owner for the
/// bytes -- which is what the `fs::write` gate above buys -- but a call to it
/// from a generator happens immediately: nothing journals it, `--pretend`
/// cannot report it, and a failure half way through a capability leaves the
/// project in a state no `continue` or `abort` can reach. The executor is
/// what supplies those three, and the rule is that every mutation goes
/// through it.
///
/// So this counts the calls that do not. `apply::` is spelled in full at every
/// call site -- there is no `use apply::put` anywhere in the workspace, which
/// `no_bare_apply_verb_imports` holds -- so the literal is the count.
pub(crate) fn executor_bypasses(src: &[Source]) -> usize {
    src.iter()
        .filter(|file| !owns_writing(&file.path))
        .map(|file| {
            let all = file.production.matches("apply::").count();
            // Five verbs say in their own names that they are not writing
            // into a project, which is what this row is about. Counting them
            // would make the gate ask for something wrong to do: there is no
            // transaction to put a write outside every project into, and none
            // that owns `target/`.
            //
            // - `put_outside_project` / `ensure_directory_outside_project`:
            //   `jails setup`'s `~/.testcontainers.properties`, `testd`'s
            //   daemon source and its cache directory. Deliberately long, so
            //   nothing editing a project reaches one by accident.
            // - `put_in_scratch`: a tree jails created empty moments earlier
            //   and removes when the run ends.
            // - `remove_derived` / `ensure_derived_directory`: build output.
            //   `target/` is Maven's and `build/` is Gradle's; no plan claims
            //   a byte of either, and both verbs *refuse* a path outside one
            //   -- so the exemption is checked rather than promised.
            //
            // Order matters in one place: `apply::remove_derived` contains
            // `apply::remove` as a prefix, so it would be counted by a shorter
            // literal. `remove` is not in this list, so there is nothing to
            // subtract twice -- but a future exemption of `apply::remove`
            // would have to account for it.
            let exempt: usize = [
                // Not a verb at all: `apply::Tree` is the *type* a staging
                // write goes through, and a `use` of it is the opposite of a
                // bypass -- a function taking one cannot reach a published
                // project.
                "apply::Tree",
                "apply::put_outside_project",
                "apply::ensure_directory_outside_project",
                "apply::put_in_scratch",
                "apply::remove_derived",
                "apply::ensure_derived_directory",
                // Authenticated sockets and metadata under `.jails/run` are
                // disposable process state, not project authority. These
                // verbs check the exact directory and refuse all other paths.
                // Rendered once from an authority that is not the model --
                // `jails.toml`'s `[layout]`, a modernized build file, a
                // contract document, a named query's adapters. The verb says
                // so in its own name and its doc lists all four; there is no
                // accepted state for a transaction to protect.
                "apply::put_one_shot",
                "apply::remove_one_shot",
                "apply::ensure_runtime_directory",
                "apply::put_runtime_state",
                "apply::remove_runtime_state",
            ]
            .iter()
            .map(|verb| file.production.matches(verb).count())
            .sum();
            all - exempt
        })
        .sum()
}

pub(crate) fn mutation_sites(src: &[Source], apis: &[&str]) -> usize {
    src.iter()
        .filter(|file| !owns_writing(&file.path))
        .map(|file| {
            apis.iter()
                .map(|api| whole_calls(&file.production, api))
                .sum::<usize>()
        })
        .sum()
}

/// Count a call name, not a prefix of one.
///
/// `fs::create_dir_all` contains `fs::create_dir`, and `fs::remove_dir_all`
/// contains `fs::remove_dir`, so a substring count reports every such call
/// twice. A gate that inflates its own number is a gate whose progress cannot
/// be read.
pub(crate) fn whole_calls(source: &str, name: &str) -> usize {
    source
        .match_indices(name)
        .filter(|(at, _)| {
            source[at + name.len()..]
                .chars()
                .next()
                .map(|next| !next.is_alphanumeric() && next != '_')
                .unwrap_or(true)
        })
        .count()
}

/// The modules whose *subject* is changing the filesystem.
///
/// `apply` is the project's write layer. `store`, `journal` and `execute` are
/// the executor's: a commit publishes bytes through a protocol, and that
/// protocol is made of exactly these calls. `scratch` and `sandbox` own trees
/// jails creates and destroys within one run.
pub(crate) fn owns_writing(path: &Path) -> bool {
    let owns = [
        "apply",
        "store.rs",
        "journal.rs",
        "execute.rs",
        // The half of the executor that actually moves bytes, split from
        // `execute.rs` by size; splitting a file does not change what the
        // project's write layer *is*.
        "activate.rs",
        "scratch.rs",
        "sandbox.rs",
        "recover.rs",
        "gc.rs",
        "lock.rs",
        // `jails new` has no project to lock and no ledger to journal, so the
        // guarantee the executor gives is bought a different way: everything
        // lands in a reserved scratch that is published by one `rename` or
        // discarded entire. This module owns that, and `Tree` is what makes
        // it checkable -- a `Tree` comes from a `Publication` and nowhere
        // else, and its absolute-path verbs refuse a write outside the tree.
        "publish.rs",
    ];
    path.components().any(|part| part.as_os_str() == "apply")
        || owns
            .iter()
            .any(|name| path.file_name().map(|file| file == *name).unwrap_or(false))
}

/// Modules whose file does not open with a `//!` doc comment.
///
/// **Read off the raw file, because `blank` erases the very thing being
/// counted.** A gate measured on blanked source cannot see comments at all,
/// and would report a clean zero whatever the tree said.
///
/// The first line rather than anywhere in the file: a module doc has to be the
/// first item, so a `//!` further down is either inside a nested `mod` block
/// or a compile error, and neither is this module having one.
pub(crate) fn modules_without_a_module_doc(src: &[Source]) -> usize {
    src.iter()
        .filter(|file| {
            let text = fs::read_to_string(&file.path).expect("the file was read once already");
            !text.trim_start_matches(['\u{feff}']).starts_with("//!")
        })
        .count()
}

/// How much of the Java jails writes is comment, as a percentage of the
/// non-blank lines in `templates/`.
///
/// Measured over the templates rather than over `crates/*/src` because this is
/// the prose a reader of a *generated project* meets, and it is the prose
/// nothing checks: a claim like "keyed on the `email` component" is asserted
/// by a template that has no way to confirm it, and it is believed.
///
/// The load-bearing comments stay -- the `@ServiceConnection` explanation, the
/// Failsafe note, why `NullPointerException` is deliberately not classified
/// fatal. What this gate stops is the next template adding another paragraph
/// of unverifiable prose beside them. A template that cannot check its own
/// claim should say less.
pub(crate) fn template_comment_density() -> usize {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let mut files = Vec::new();
    collect_java(&root, &mut files);
    assert!(
        files.len() > 100,
        "the template scanner found only {} files -- it has lost track of where \
         the generated Java lives, and would report a clean density over prose \
         it never read",
        files.len()
    );
    let (mut comment, mut total) = (0usize, 0usize);
    for source in &files {
        let mut in_block = false;
        for line in source
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            total += 1;
            if in_block || line.starts_with("/*") {
                comment += 1;
                in_block = !line.contains("*/");
            } else if line.starts_with("//") {
                comment += 1;
            }
        }
    }
    comment * 100 / total.max(1)
}

/// Every `.java` under a directory, read whole -- templates are not Rust, so
/// [`sources`] does not see them.
pub(crate) fn collect_java(dir: &Path, out: &mut Vec<String>) {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("failed to read a directory entry").path();
        if path.is_dir() {
            collect_java(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "java") {
            out.push(
                fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display())),
            );
        }
    }
}

#[cfg(test)]
mod blanking_tests {
    use super::*;

    /// Which module is currently the largest, printed for the ladder board.
    #[test]
    fn report_the_largest_modules() {
        let src = sources();
        let mut rows: Vec<(usize, String)> = src
            .iter()
            .map(|file| (production_lines(file), file.path.display().to_string()))
            .collect();
        rows.sort_by_key(|row| std::cmp::Reverse(row.0));
        println!("\nlargest production modules");
        for (lines, path) in rows.iter().take(8) {
            println!("  {lines:5}  {path}");
        }
    }

    #[test]
    fn blanking_erases_comments_and_literals_but_preserves_offsets() {
        let source = "let a = \"root: &Path\"; // root: &Path\nlet b = 1;\n";
        let blanked = blank(source);
        assert_eq!(blanked.len(), source.len(), "offsets must still line up");
        assert_eq!(blanked.lines().count(), source.lines().count());
        assert!(
            !blanked.contains("root: &Path"),
            "a literal and a comment both hid a countable token: {blanked:?}"
        );
    }

    #[test]
    fn blanking_erases_raw_strings_holding_java() {
        let source = "let java = r#\"package a; class B { void fn(int x) {} }\"#;\n";
        let blanked = blank(source);
        assert_eq!(blanked.len(), source.len());
        assert!(!blanked.contains("package"), "{blanked:?}");
        assert!(
            fn_param_counts(&blanked).is_empty(),
            "a Java method must not be counted as a Rust fn"
        );
    }

    #[test]
    fn parameters_are_counted_at_the_top_level_only() {
        let counts = fn_param_counts(
            "fn a(x: Result<A, B>, y: (u8, u8)) {}\nfn b() {}\nfn c<T: Into<X>>(t: T) {}\n",
        );
        assert_eq!(counts[0], ("a".to_string(), 2), "{counts:?}");
        assert_eq!(counts[1], ("b".to_string(), 0), "{counts:?}");
        assert_eq!(counts[2], ("c".to_string(), 1), "{counts:?}");
    }

    #[test]
    fn a_trailing_comma_does_not_invent_a_parameter() {
        let wrapped = fn_param_counts("fn a(\n    x: u8,\n    y: u8,\n) {}\n");
        let inline = fn_param_counts("fn a(x: u8, y: u8) {}\n");
        assert_eq!(wrapped[0].1, 2, "{wrapped:?}");
        assert_eq!(inline[0].1, 2, "{inline:?}");
    }

    #[test]
    fn a_lifetime_is_not_read_as_a_character_literal() {
        let source = "fn a<'a>(x: &'a str, y: char) {}\n";
        let blanked = blank(source);
        assert_eq!(blanked, source, "nothing here should be blanked");
        assert_eq!(fn_param_counts(&blanked)[0].1, 2);
    }
}
