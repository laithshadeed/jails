//! **How the counting works**, and nothing about what the numbers mean.
//!
//! [`Source`] is a production file with its comments, string literals and
//! `#[cfg(test)]` bodies blanked to spaces of the same length -- so byte
//! offsets and line numbers still index the original, which is what lets a gate
//! locate something in the blanked text and then read it from the raw file.
//! That is `java.rs`'s own trick, applied to Rust for the same reason.
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
    /// abstract.md §3 states its line counts exclude `mod tests`, and every
    /// gate here means the same thing: a fixture that writes a scratch `pom.xml`
    /// is not a production `fs::write`, and a test helper taking `root: &Path`
    /// is not the primitive being propagated. Counting them makes the ladder
    /// punish the tests that prove a rung did not change behaviour.
    pub(crate) production: String,
    /// The same file with comments and `#[cfg(test)]` bodies removed, but
    /// **string literals intact** -- for the gates whose subject only ever
    /// appears inside one. See [`keeping_literals`].
    pub(crate) literals: String,
}

/// Every production Rust file in the workspace, not only the binary's own.
///
/// The binary is one crate of seven. A scanner that walked `src/` alone would
/// keep reporting green while the code it gates moved into `crates/*/src` --
/// the same failure as a skipped tier-3 test, which the suite also reports as
/// passing unless something insists otherwise.
/// Read and blanked **once per process**, and blanked in parallel.
///
/// Nineteen gates share this scan and each used to run it again: the walk,
/// the read and two blanking passes over 414 files and 5.9 MB, eleven times
/// over. That is what made this binary the third most expensive target in the
/// suite at 9.4 seconds while doing no I/O worth the name and starting no
/// process at all.
///
/// The memo is the whole fix and it is sound for one reason worth stating: a
/// [`Source`] is immutable, the files are not written while the binary runs,
/// and every gate already treats the scan as a snapshot -- two gates
/// disagreeing about the contents of the tree would be a bug whichever way
/// the scan was cached.
///
/// The blanking itself is a byte-at-a-time state machine per file with no
/// shared state, so it is spread across the same scheduler the table-driven
/// binaries use. The `assert` stays exactly where it was: a scanner that has
/// lost the code reports precisely what a clean one does, and caching a wrong
/// answer once is worse than recomputing it.
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
/// The same trick as `src/java.rs::blanked`, and for the same reason: a scan
/// must not be fooled by `// root: &Path` or by the word `fn` inside one of
/// `spring.rs`'s inline Java bodies, while offsets and line numbers still line
/// up with the file on disk.
/// **Delimiters are matched on bytes, not on `&source[i..]`.** Inside a
/// comment or a string this walks one byte at a time, so `i` can sit in the
/// middle of a multi-byte character — and `&source[i..]` panics there rather
/// than returning false. It did: a `§` inside an `r#"..."#` template body
/// took eight of these gates down at once, having been latent for as long as
/// no raw string held a character outside ASCII. Every delimiter here is
/// ASCII, so a byte comparison is exactly equivalent and cannot panic.
pub(crate) fn blank(source: &str) -> String {
    blank_with(source, false)
}

/// The same walk, keeping string and character literals as they are.
///
/// Three gates count text that only ever appears *inside* a literal -- the
/// `# jails:` markers, the `"version"` JSON key, `spring.rs`'s inline
/// `r#"package ` bodies -- and [`blank`] replaces every literal with spaces
/// before they run. All three therefore counted zero whatever the code said,
/// and all three record a ceiling of zero, so nothing distinguished a gate
/// holding the line from one that had lost the text it was about. That is the
/// failure `sources()` already guards against at the file level and the same
/// one `CLAUDE.md` records for skipped tier-3 tests: a scanner that read
/// nothing reports exactly what a clean one does.
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
/// primitive being *contained* rather than propagated. Counting those made the
/// gate read flat while the parameter it exists to remove was disappearing --
/// a measurement that cannot see the improvement it is asking for.
pub(crate) fn root_path_parameters(src: &[Source]) -> usize {
    src.iter()
        // The canonical workspace crate is the capture/apply boundary. Its
        // subject is deliberately a filesystem root; the pure compiler above
        // it receives a WorkspaceSnapshot and cannot name one. The legacy
        // ratchet remains useful for code that should take Project instead.
        .filter(|file| !is_canonical_workspace(&file.path))
        .map(|file| {
            ["root: &Path", "root: &std::path::Path"]
                .iter()
                .flat_map(|spelling| file.production.match_indices(spelling))
                .filter(|(at, _)| !file.production[..*at].trim_end().ends_with("let"))
                // `module_root: &Path` and `workspace_root: &Path` are not
                // this parameter. Counting them inflated the number by six and
                // made `project.rs` -- which walks a reactor and *must* read
                // each pom along the way -- look like the disease.
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
/// The distinction rung 1 is actually about: envy is asking the disk for a fact
/// somebody already resolved. These two ask the disk because the resolved
/// answer is **absent** -- there is no project yet.
///
/// It was four. `ensure_console_launcher` went with `pending.md` §7.7's move
/// of the `--fast` splice into a transition, and `ensure_package_info` stopped
/// taking a `root` at all when `write_new_file` started taking an
/// `apply::Tree` -- it reads the pom out of the staging tree it was handed,
/// which is not a second read of anything. Declared rather than counted, because a
/// number nobody can reach is a gate nobody reads; the pattern is
/// `SILENT_WITHOUT_A_RECORD`'s, and a stale entry here fails the test below the
/// same way.
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
        "read_source_at",
        "the model it is reading does not exist yet: a project with none reads as the \
         seed `model init` would write, and that seed is *derived from* the project -- \
         its package, its release, its build tool. There is no resolved `Project` to be \
         handed, which is the same absence `read_build_file` records",
    ),
    (
        "load_model_at",
        "same absence as `read_source_at`, one layer up: it parses what that returns, \
         and on a project with no model the source is the derived seed rather than a \
         file. Both halves are the construction, not a second read beside it",
    ),
    (
        "sync_at",
        "`jails.toml` is not a fact a `Project` holds for this purpose. Its `[layout]` \
         is, and the snapshot carries that; what is read here is the *capability list*, \
         which the canonical path does not act on -- so the read exists to refuse a name \
         jails does not know rather than to answer a question. A capability nothing \
         recognises sitting in that file looks applied and never will be, which is the \
         failure a manifest exists to remove",
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
    "add_jspecify",
    "write_agents",
    "ensure_enforcer",
];

/// `(file, function)` for every `root: &Path` function that goes back to disk
/// for something a resolved `Project` already holds.
pub(crate) fn rederivers(src: &[Source]) -> Vec<(String, String)> {
    // Applied to `root` specifically. Without the argument this counted
    // `reconcile_intent`, which loads a `Project` for each of two *scratch*
    // copies of the tree -- the opposite of envy, since there is no resolved
    // project for those roots to have been passed.
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
/// Spelling the visibilities out cost a gate its sight: the workspace split
/// turned `pub(crate) struct` into `pub struct` inside a moved crate, and a
/// scanner matching only the first two spellings reported zero — an improvement
/// that had not happened, over code it could no longer see.
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

/// An `encode`/`decode` half that is not a `Codec` method.
///
/// The signatures are what identify one: `encode(&self, encoder: &mut Encoder)`
/// and `decode(decoder: &mut Decoder<'_>)`. A method with either shape sitting
/// in an inherent `impl` is a value on the wire that `Encoder::seq`,
/// `Encoder::set` and `Encoder::map` cannot be used on, so its collection
/// handling has to be written out again by hand.
pub(crate) fn inherent_codec_halves(src: &[Source]) -> usize {
    let mut count = 0;
    for file in src {
        let mut in_codec_impl = false;
        for line in file.production.lines() {
            // `trim_start`, because `digest_newtype!` and `logical_id!`
            // expand to `impl Codec for $name` indented inside a
            // `macro_rules!` body -- and a scanner that read column zero
            // would report six perfectly good trait impls as violations.
            let head = line.trim_start();
            if head.starts_with("impl ") {
                in_codec_impl = head.starts_with("impl Codec for ");
            }
            // A declaration, not a definition: the trait's own two lines.
            let is_half = !head.ends_with(';')
                && (head.contains("fn encode(&self, encoder: &mut Encoder)")
                    || head.contains("fn decode(decoder: &mut Decoder<'_>)"));
            if is_half && !in_codec_impl {
                count += 1;
            }
        }
    }
    count
}

/// Production files that parse Maven's XML with a scanner of their own.
///
/// `simplify-sol.md`'s deletion map: *duplicate Maven XML scanners -> one
/// document backend*, deleting "second scanners and field-name lies".
///
/// **Parsers, not emitters.** Five more files mention `<dependency>` while
/// only ever *building* one, and rendering a block jails owns is not a claim
/// to understand a build file -- `CLAUDE.md`'s bar is "answer exactly or
/// refuse, never guess", and that is a bar on reading. Counting the emitters
/// would put five files in a row about parsing and make the number stop
/// meaning what its name says.
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
                        // closing tags, and requiring it at position zero
                        // found none of them.
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
/// `simplify-sol.md`'s fitness rule: *every builtin type has one semantics
/// row*. Sixteen builtins were described by seven separate matches -- the
/// token, its aliases, the Java type, the import, the sample, the Postgres
/// column, which literals may default it -- and `primitive` was written out
/// twice, identically, in two emitters. Each match was exhaustive over the
/// enum and silent about the others, so adding a builtin compiled in every
/// one of them and was wrong in most.
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

/// Where the count stands today: `jails-project/src/pom.rs`,
/// `jails-protocol/src/vocabulary/coordinate.rs`, and the three of the new
/// tree -- `jails-workspace/src/{capture,documents}.rs` and
/// `documents/build_feature.rs`.
///
/// `jails-workspace/src/capture/observe.rs`'s `junit_version` is deliberately
/// *below* the bar: it matches on one element to read one artifact's version,
/// which is a lookup rather than a scanner. Two distinct elements is what
/// separates "asks the pom a question" from "has an opinion about its
/// structure", and it is the second that duplicates.
///
/// It used to be `jails-project/src/junit.rs`, which was a second copy of that
/// rule with no callers left and is deleted. The exemption belongs to the
/// question, not to the file that happened to ask it.
pub(crate) const MAVEN_XML_PARSERS: usize = 5;

/// `# jails:` marker literals outside the crate that owns the format.
///
/// Recorded for the first time on 2026-08-29, and closed the same day. The row
/// had claimed zero since it was written -- not because it held, but because it
/// counted `Source::production`, where every string literal is blanked to
/// spaces before a gate sees it. Ceiling and target were both `0`, so a vacuous
/// gate and a held line printed the same word. It reads `Source::literals` now.
///
/// The true count was **13**, and the three that mattered were second
/// implementations of the block itself -- all in the new tree, none careless:
/// `codemod` lived in `jails-project`, which neither `jails-compiler` nor
/// `jails-workspace` depends on, so there was nothing to reuse. It is
/// `crates/jails-codemod` now, with no dependencies at all, so there is
/// nowhere left that cannot reach it.
///
/// 13 -> 8 -> 0. The remaining ten were knowledge of the format rather than
/// copies of it, and each went a different way: two substring probes became
/// `Marked::present_in`, which also fixed the prefix collision `exact_line`
/// exists for -- `contains("# jails:db")` reads `# jails:dbx` as this block.
/// `doctor` asked `compose::declares` instead of spelling this file's
/// two-space indent itself. Three refusal messages quote
/// `Marked::OPEN_PREFIX` rather than retyping it, so a message about the
/// format cannot outlive the format. And `jails setup`'s note in
/// `~/.testcontainers.properties` says `# jails --`, because `# jails:` is how
/// a block opens and that file has none.
///
/// **The gate can fail now**, which is the part worth keeping: adding a
/// `# jails:` literal to any production file outside `jails-codemod` moves it
/// off zero. That was verified by doing it.
pub(crate) const MARKED_BLOCK_LITERALS: usize = 0;

/// Where the count stands today. The remaining four are `context_value`'s
/// four-arm lowering, which is a rendering rather than a registry.
pub(crate) const LARGEST_BUILTIN_TABLE: usize = 4;

/// The crates that must stay pure once the workspace has been captured.
///
/// `simplify-sol.md`'s first fitness rule: *after capture, no compiler module
/// can access `std::fs`, a project root or a process runner*. The whole
/// argument for a capture pass is that planning becomes a function -- the same
/// snapshot and the same request yield the same plan, which is what makes a
/// plan diffable, cacheable and safe to show before it is applied. One
/// `fs::read` inside a pass is enough to break that, and nothing about the
/// resulting bug says so: it just means the plan depended on a file nobody
/// recorded reading.
pub(crate) const PURE_COMPILER_CRATES: [&str; 3] =
    ["jails-compiler", "jails-model", "jails-contracts"];

/// A pure crate reaching for the world outside the snapshot it was handed.
///
/// Counted at **zero**, and it is zero today -- which is the point. This is a
/// property that is cheap to keep and expensive to recover, so it is gated
/// while it still holds rather than after the first pass reads a file.
///
/// The `root: &Path` half is counted here too, through the same function the
/// workspace-wide row uses: a path a pass could resolve against is a project
/// root whether or not it reads one yet, and the workspace row's ceiling of
/// 145 would absorb a rise inside these three crates without noticing.
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

/// Types whose wire format is written out by hand rather than derived.
///
/// One `impl Codec for X` is three statements of the same format -- the field
/// list in the struct, again in `encode`, again in `decode` -- so a field
/// added to the type and forgotten in the codec is a silent change of format
/// rather than a compile error. `#[derive(Codec)]` makes the declaration the
/// only owner of the encoding, which is what `simplify-sol.md`'s fitness rule
/// asks for: *every persisted union tag and field number is generated and
/// golden-tested*.
///
/// `trim_start` for the same reason [`inherent_codec_halves`] needs it:
/// `digest_newtype!` and `logical_id!` expand to `impl Codec for $name`
/// indented inside a `macro_rules!` body. Those are counted, and should be --
/// a macro is a hand-written codec shared by six types, not a derived one --
/// but they count once each, where they are written.
pub(crate) fn hand_written_codecs(src: &[Source]) -> usize {
    src.iter()
        .filter(|file| !file.path.ends_with(WIRE_RS))
        .map(|file| {
            file.production
                .lines()
                .filter(|line| {
                    let head = line.trim_start();
                    head.starts_with("impl ") && head.contains(" Codec for ")
                })
                .count()
        })
        .sum()
}

/// The primitives the derive is built out of, excluded from the row above.
///
/// `bool`, `u32`, `u64`, `String`, `Option<T>`, `Vec<T>`, `BTreeSet<T>`,
/// `BTreeMap<K, V>` and `Box<T>` are where the recursion stops: a derive can
/// only delegate to [`Codec`], so something has to state what a `bool` is on
/// the wire. Counting them would put a floor in the row that no work could
/// ever remove, which is the same reason `codemod.rs` is excluded from the
/// `# jails:` row -- the one legitimate owner is not a violation.
pub(crate) const WIRE_RS: &str = "jails-support/src/codec/wire.rs";

/// Where the count stands today. See the row in [`crate::board`] for why the
/// target is withdrawn rather than zero.
///
/// 210 -> 147: `#[derive(Codec)]`, `codec/wire.rs` and `jails-codec-derive`.
///
/// 147 -> 146 for `RendererStamp`, and the one matters more than the number:
/// it disproves "the mechanical seam is exhausted", which is what the first
/// sweep concluded because it treated `encoder.count(..)` as a hard blocker.
/// It is not one. `Encoder::seq` *is* a count followed by a loop of `encode`,
/// `set` is that plus the `ordered` check, and `map` the same for pairs -- so
/// a codec framing its own collection is byte-identical to `Vec<T>`,
/// `BTreeSet<T>` or `BTreeMap<K, V>` doing it, ordering guarantee included.
/// 29 codecs frame a collection by hand; the ones whose field is already one
/// of those three convert with no wire change. `plan.md` P13.4.
///
/// 146 -> 144 for `DesiredAppliedEntity` and `OutputRecord`, both framing a
/// `BTreeSet<OwnerId>` -- which needed `OwnerId` to have a codec at all. It
/// had a public `tag()`/`from_tag()` pair and no impl, because its containers
/// wrote the tag inline; the derive reproduces that byte and `label = "owner"`
/// keeps the refusal the test pins.
///
/// 133 -> 105, and the largest class in it is the one `OwnerId` was the first
/// of: **an enum with a hand-written `tag()`/`from_tag()` pair is a second
/// encoding of that type**, and its containers called the pair rather than the
/// trait. Seven such pairs are gone -- `ReferenceRole`, `ResumeState`,
/// `FormatOwner`, `OneShotKind`, `MaintenanceAttribution`, `EffectFailureCode`
/// and `OwnerId`'s leftover -- and `ReferenceRole` is the one to read: it
/// already derived `Codec` *and* still carried the pair, so one enum had two
/// encodings and only one of them was the wire's.
///
/// Four more enums had the pair and no codec at all (`MavenScope`,
/// `Optionality`, `JavaTypeKind`, `ToolFeature`); deriving each one is what
/// made its containers derivable in turn, which is where the rest of the fall
/// comes from. Every conversion was read against the declaration and its
/// existing tags before it was made, and the two byte oracles -- the protocol
/// goldens and `-p jails-prepare -p jails-commit -p jails-protocol` -- ran
/// after each batch, because `PreparedIdentityV1` is the standing proof that
/// the golden trees alone do not see a moved wire.
///
/// `AppliedEntity` is the shape that stays hand-written and shows where the
/// line is: it refuses an empty owner set inside `encode`, so its codec
/// enforces an invariant rather than describing a layout. Only its `tag()`
/// calls moved onto `OwnerId`'s codec.
///
/// 105 -> 90, and this is where the mechanical seam ends for a reason rather
/// than for lack of looking. **49 of the 90 validate**: `decode` calls a
/// validating constructor, or `encode` enforces an invariant before writing a
/// byte -- `ByteSpan` refusing a span that starts after it ends,
/// `PropertySetting` refusing a comment carrying its own `#`,
/// `CanonicalRequestSyntaxV1` rejecting dashes, `EffectState` refusing a zero
/// attempt. That is R1.1 working, not a sweep left unfinished: the constructor
/// is the only place a value is validated, so a derive that skipped it would
/// let a value rejected at the CLI arrive through a recovered journal instead.
///
/// The rest of the 90 are as deliberate and smaller in number: six encode a
/// label string rather than a discriminant (`RendererId`, `IntentId`,
/// `SqlDialect` and their kin -- §R1.4 records the *name* so reordering an
/// enum cannot change a recorded value), four are the primitive impls the
/// derive is built on, three frame a length-capped blob through
/// `encoder.object`, two write a raw digest array, and one carries a depth
/// counter for a recursive type.
///
/// So the next move on this row is not more reading. It is either a
/// `#[codec(validate)]` that calls the constructor after decoding -- which
/// would reach most of the 49 -- or accepting the number, which is what
/// "target withdrawn" already says.
pub(crate) const HAND_WRITTEN_CODECS: usize = 90;

/// The other three files a gate here names. Paths for the same reason: a
/// second `doctor.rs` or `codemod.rs` anywhere in the workspace would silently
/// join or leave the set its gate measures, and the gate would report a number
/// about a different file without saying so.
pub(crate) const CODEMOD_RS: &str = "jails-codemod/src/marked.rs";

/// The one module allowed to know that `git merge-file` takes a diff
/// algorithm. See the board row that counts the literal elsewhere.
pub(crate) const GIT_RS: &str = "jails-support/src/git.rs";
pub(crate) const DOCTOR_RS: &str = "jails-report/src/doctor.rs";
pub(crate) const SCRATCH_RS: &str = "jails-support/src/scratch.rs";

/// Where the count stands today. Lowered per message, never by a sweep.
///
/// 443 → 439 on 2026-08-25, and not by writing four `fix:` lines: `pending.md`
/// §6.3 deleted the second field-spec parser, and four of its refusals went
/// with it. A duplicate parser is four duplicate refusals.
///
/// 439 → 437 the same day. §3's build-feature key added four internal-invariant
/// refusals and deleted one user-facing one, and the four were given the fix
/// line the two `publish::Tree` refusals already use — "this is a bug in jails,
/// not something a project can cause". That is a real next step rather than a
/// filler: it tells the reader the one thing they can do.
///
/// 437 → 436 when truthful parent-directory capture made a file/directory
/// collision actionable: move or rename the colliding file, then retry.
/// 436 → 432 when every compatibility-version refusal gained an explicit
/// upgrade path instead of only describing the unsupported bytes.
/// 432 → 430 when planned-subject decoding gained the same recovery advice.
/// 430 → 426 when the extracted rename-source boundary made each invalid-name
/// refusal carry the exact valid spelling to retry.
/// 426 → 425 when the durable-job identity refusal named the satisfiable
/// use-case change instead of asking for a payload the command rejects.
/// 425 → 424 when colliding generated field names gained an actionable rename
/// instruction.
/// 424 → 423 when `FieldName` collapsed the two spellings of one field into
/// one value (plan.md P3.1): the column-collision refusal and the
/// duplicate-name refusal were two branches of one condition, and the branch
/// that survives is the one that already carried a `fix:` line.
// 423 -> 421: the two "takes only a name" refusals `fetcher` and
// `idempotency` wrote inline are the one helper in `recipes/flags.rs` now,
// and it carries a `fix:` line. plan.md P8.8.
// 421 -> 419: `auth` and `webhook` wrote their "takes only a name" refusal
// inline with no next step, and both are `recipes/flags.rs`'s helper now --
// the same move `fetcher` and `idempotency` made, and the same `fix:` line.
// 419 -> 418: `g query`'s "optional or a collection" refusal was two
// refusals in one sentence and named no next step. Optional filters are
// generated now (plan.md P10.5), and what is left refuses a collection with
// the two things a reader can do about it.
// 418 -> 396: 22 hand-written codecs became `#[derive(Codec)]`, and each had
// carried its own `Err(format!("unknown <thing> tag {other}"))`. They were the
// same refusal 22 times, so collapsing them is not 22 messages improved -- it
// is 21 copies deleted. What the derive added is that the wording can no
// longer drift per type, and that a refusal needing a next step says so on the
// type (`#[codec(unknown_fix = "...")]`) rather than in the one place the
// message is built. `PlannedSubject` is the first to use it; the other ten
// `fix:`-carrying decoders are still hand-written.
// 391 -> 375 as a consequence of the codec row above, not as work of its own:
// sixteen more unknown-tag refusals that named no next step were deleted with
// the codecs that built them. The five carrying one -- the rename, column,
// type-change and repair policies -- kept it on the type through
// `#[codec(unknown_fix = ...)]`, which is what stops the wording drifting per
// type the way it had.
// 375 -> 369, the same consequence: six more unknown-tag refusals that named
// no next step went with the codecs that built them.
// 369 -> 325: the legacy generator stack went, and forty-four of its refusals
// with it -- a `g scaffold` that cannot map a field type, an `add` that cannot
// plan a capability, a recipe refusing a flag combination. None of them can be
// reached now that every kind and capability compiles.
pub(crate) const REFUSALS_WITHOUT_A_FIX: usize = 325;

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
        // This legacy free-text heuristic cannot see structured diagnostics:
        // every semantic diagnostic in jails-model has a mandatory fix, while
        // internal compiler/executor errors are intentionally not decorated as
        // user advice. Keep the old ratchet on the old error vocabulary and
        // enforce the new contract structurally in rules.rs.
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
        // The module *and its children*. This gate names files by path and
        // `model_generate` grew a `render.rs`, which is the same module in the
        // same world -- matching only the basename dragged its refusals into a
        // legacy count. `CLAUDE.md` records the same trap costing two rows
        // when `src/new/spring.rs` appeared beside `spring.rs`.
        || path.ends_with("/src/model_generate.rs")
        || path.contains("/src/model_generate/")
        || path.ends_with("/src/model_import.rs")
        || path.ends_with("/src/model_setting.rs")
}

pub(crate) fn write_sites_outside_apply(src: &[Source]) -> usize {
    mutation_sites(src, &["fs::write"])
}

/// Every API that changes the filesystem, wherever it is spelled.
///
/// plan.md §R6.4: the gate "currently bans only literal `fs::write`; it must
/// expand to `write`, `OpenOptions` write modes, `remove_file/remove_dir`,
/// `copy`, `rename`, hard links, directory creation, permissions and mutating
/// subprocesses." The reason the narrow version was not enough is visible in
/// the count: `fs::write` was at zero while a dozen other calls mutated the
/// project through other names, so the gate read green over exactly the
/// surface R6 has to migrate.
/// Where the count stands today. Lowered by each migrated surface.
///
/// **Zero, 2026-08-25 — the rung is reached.** 56 → 46 → 11 → 6 → 0, and the
/// last six were the ones `pending.md` §7.7 called "a real decision rather
/// than a migration nobody got to". Each was decided rather than moved:
///
/// - `generate/write.rs`'s two took an `apply::Tree` instead of a `&Path`.
///   Every caller of `write_new_file` is on the `jails new` path, so the
///   signature now says what a comment used to, and a write outside the
///   staging tree is refused.
/// - `run.rs`'s `pom.xml` splice for `test --fast` became the transition that
///   was already written and unwired: `route::install_fast_test`, with
///   `jails remove fast-test` as the other half.
/// - `add/database.rs`, `console.rs` and `testd.rs` write build output and
///   machine state, not a project. They go through `remove_derived`,
///   `ensure_derived_directory` and `ensure_directory_outside_project`, and
///   the first two **refuse** a path outside `target/` or `build/` — so the
///   exemption below is checked rather than promised.
pub(crate) const MUTATION_CEILING: usize = 0;

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
/// project in a state no `continue` or `abort` can reach. The executor
/// (`execute.rs` + `activate.rs`, driven from `jails-commit`) is what supplies
/// those three, and R6.4's rung is that every mutation goes through it.
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
            // made the gate ask for something that would be wrong to do:
            // there is no transaction to put a write outside every project
            // into, and none that owns `target/`.
            //
            // - `put_outside_project` / `ensure_directory_outside_project`:
            //   `jails setup`'s `~/.testcontainers.properties`, `testd`'s
            //   daemon source and its cache directory. Deliberately long, so
            //   nothing editing a project reaches one by accident.
            // - `put_in_scratch`: a tree jails created empty moments earlier
            //   and removes when the run ends.
            // - `remove_derived` / `ensure_derived_directory`: build output.
            //   `target/` is Maven's and `build/` is Gradle's; nothing in the
            //   ledger claims a byte of either, and both verbs *refuse* a path
            //   outside one -- so the exemption is checked rather than
            //   promised. `pending.md` §7.7.
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
/// the executor's: R4's whole point is that a commit publishes bytes through
/// a protocol, and that protocol is made of exactly these calls. `scratch`
/// and `sandbox` own trees jails creates and destroys within one run.
pub(crate) fn owns_writing(path: &Path) -> bool {
    let owns = [
        "apply",
        "store.rs",
        "journal.rs",
        "execute.rs",
        // The half of the executor that actually moves bytes. It was inside
        // `execute.rs` until that module outgrew the size ceiling; splitting a
        // file must not change what the project's write layer *is*.
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
        // `pending.md` §5.
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
/// counted.** Same trap `inline_java_bodies` documents one row down: a gate
/// measured on blanked source cannot see comments at all, and would report a
/// clean zero whatever the tree said.
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
/// nothing checks. `modern.md` §11.2 is the finding: "keyed on the `email`
/// component" (it was not), "ordering per entity" (it was not), "scoped
/// matches cannot mutate another tenant's row" (there was no scope in the
/// SQL), "this type has no `id` component" (it had one). Each was asserted by
/// a template that had no way to confirm it, and each was believed.
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
