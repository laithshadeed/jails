//! The eleven gates of `abstract.md` §8, as ratchets rather than as a table.
//!
//! `abstract.md` prices every rung of its refactor ladder with a falsifiable
//! gate and says to revert the rung when the gate is missed. Nothing measured
//! them, and §8.1 recorded the consequence: `root: &Path` rose 21% across four
//! commits while `src/model/mod.rs` was being added *beside* the primitive
//! rather than instead of it, and nothing said so. Prose did not move the
//! number; `tests/genericity.rs` moved the vocabulary problem only once it put
//! a failure in the build.
//!
//! So each row below is a **ratchet**, and it bites in both directions:
//!
//! - **Rising above the ceiling fails.** That is the regression guard.
//! - **Falling below the ceiling also fails**, telling you to record the new
//!   number. That is what makes progress permanent: an improvement that is not
//!   written down here is an improvement the next change may silently undo.
//!
//! `target` is the number `abstract.md` §8 actually asks for. A ceiling equal
//! to its target is a finished rung; the test prints the gap for every row, so
//! `cargo test --test architecture -- --nocapture` is the ladder's progress
//! report.
//!
//! Measurement is over **blanked** Rust: comments and string literals are
//! replaced by spaces of the same length, so a `fn` inside one of `spring.rs`'s
//! inline Java bodies cannot be counted as a function, and a `root: &Path`
//! written in a doc comment cannot be counted as a parameter. That is
//! `src/java.rs`'s own trick, applied to Rust for the same reason.

use std::fs;
use std::path::{Path, PathBuf};

/// One measurable gate from `abstract.md` §8.
struct Ratchet {
    /// What is being counted, phrased as the thing that should shrink.
    name: &'static str,
    /// Which rung of `abstract.md` §7 closes it.
    rung: &'static str,
    /// Today's recorded number. May only ever be lowered.
    ceiling: usize,
    /// What `abstract.md` §8 asks for. `ceiling == target` is a closed gate.
    target: usize,
    /// Why the number matters, printed when the row fails.
    why: &'static str,
}

fn gates() -> Vec<(Ratchet, usize)> {
    let src = sources();
    vec![
        (
            Ratchet {
                name: "`root: &Path` parameters (target withdrawn — §8.0)",
                rung: "1 — Introduce Parameter Object (`Project`)",
                // 142 -> 144 was a measurement correction, not a regression:
                // the `&std::path::Path` spelling was never being counted.
                // 137 -> 140 for `apply_in`, `project_at` and `seed_manifest`,
                // which are entry points that *resolve* a `Project` from a
                // root -- the cure rather than the disease, and what the
                // target of 40 rather than 0 leaves room for. The disease is
                // a `root` threaded through a call graph so each level can
                // re-derive facts; these hand one to `Project::load` and stop.
                // 139 -> 143 for plan.md §12's reach work: `build::detect`,
                // `build::require_maven_at`, `config::record_layout` and
                // `adopt::report`. All four ask "what is at this path" before
                // any `Project` exists -- `detect` is what `Project::load`
                // itself calls first -- so a `Project` parameter is not
                // available to them, and this is the containment boundary
                // rather than the disease. `run.rs`'s eight call sites fold
                // into one `maven_root`, which is why it is four and not five.
                ceiling: 140,
                // Withdrawn, not reached. abstract.md §8.0: the count includes
                // modules whose subject *is* a path, so 40 read as a demand to
                // stop writing modules. The row below is rung 1's condition;
                // this one stays a ratchet against growth, which is why the
                // target tracks the ceiling rather than sitting under it.
                target: 140,
                why: "Every one is a fact re-derived from a primitive instead of read off \
                      the resolved `Project`. This is the count abstract.md §8.1 watched \
                      rise from 161 to 195 with nothing to say so.",
            },
            root_path_parameters(&src),
        ),
        (
            Ratchet {
                name: "undeclared root-taking readers of the pom",
                rung: "1 — Introduce Parameter Object (`Project`)",
                // The row above counts every `root: &Path`, and by now that
                // includes modules whose whole subject *is* a path --
                // `build.rs` asks what builds a directory, `ledger.rs` and
                // `launcher.rs` read files under one. Those are the
                // containment, not the disease, and the raw count rises every
                // time one is added, which makes its target of 40 read as a
                // demand to stop writing modules.
                //
                // So this is the disease itself, in abstract.md §2's words: a
                // function handed a primitive that goes back to disk for a
                // fact the resolved `Project` already holds. It measures the
                // *undeclared* ones, because the four that survive are each a
                // case where reading again is correct and a `Project` would be
                // wrong -- see `A_FRESH_READ_IS_CORRECT`. Nought means every
                // one of them is a decision somebody wrote down.
                ceiling: 0,
                target: 0,
                why: "Feature Envy on `Project`: a second read of the pom for a fact the \
                      caller already resolved, which is how two answers to one question \
                      appear in one run.",
            },
            rederivers(&src)
                .into_iter()
                .filter(|(_, name)| {
                    !A_FRESH_READ_IS_CORRECT
                        .iter()
                        .any(|(declared, _)| declared == name)
                })
                .count(),
        ),
        (
            Ratchet {
                name: "functions in `spring.rs` taking over 5 parameters",
                rung: "1 — Introduce Parameter Object (`Layers`)",
                ceiling: 0,
                target: 0,
                why: "The layer packages travel one at a time because `Layers` is not a \
                      value: a Data Clump producing connascence of position at degree 12, \
                      which is the highest-cost coupling in Page-Jones's ranking.",
            },
            over_five_params(&src, "spring.rs"),
        ),
        (
            Ratchet {
                name: "structs in `src/` with a `contents`/`body` field",
                rung: "2 — Extract Class (one `Change`)",
                ceiling: 1,
                target: 1,
                why: "abstract.md §4.1 found four shapes for `a file to write`, two of them \
                      in the same file. Exactly one struct may carry a body, and it is \
                      `model::Artifact`.",
            },
            body_carrying_structs(&src),
        ),
        (
            Ratchet {
                name: "ad-hoc `(path, body, label)` file tuples",
                rung: "2 — Extract Class (one `Change`)",
                ceiling: 0,
                target: 0,
                why: "The fourth of abstract.md §4.1's four shapes, and the one still \
                      standing: 14 `*_files` functions in `spring.rs` returning a positional \
                      tuple where `model::Artifact` says the same thing by name. Swap two \
                      fields and it compiles and emits wrong Java.",
            },
            file_tuple_types(&src),
        ),
        (
            Ratchet {
                name: "aliases hiding the one `Change`/`Artifact` type",
                rung: "2 — Extract Class (one `Change`)",
                ceiling: 0,
                target: 0,
                why: "`NewFile` and `SpringSlice` now point at the one shape, which is the \
                      migration working. They are scaffolding, not a destination: two names \
                      for one type is how the four shapes got there in the first place.",
            },
            type_aliases(&src),
        ),
        (
            Ratchet {
                name: "`dry_run || pretend` sites",
                rung: "3 — Command with undo (one `describe`)",
                ceiling: 0,
                target: 0,
                why: "Two names for one boolean, OR'd at dispatch because the global flag \
                      and the per-command flag reach two different implementations. \
                      Connascence of meaning crossing a module boundary.",
            },
            count_matches(&src, "dry_run || pretend") + count_matches(&src, "pretend || dry_run"),
        ),
        (
            Ratchet {
                name: "`KIND_FILES`/`NO_FILE_TABLE` references",
                rung: "4–5 — Separate Query from Modifier; derive `destroy`",
                ceiling: 0,
                target: 0,
                why: "A second transcription of the file list the generator right next door \
                      already computes. `tests/agreement.rs` polices it; abstract.md §9 says \
                      a test that polices duplication is a receipt for a decision not made.",
            },
            count_matches(&src, "KIND_FILES") + count_matches(&src, "NO_FILE_TABLE"),
        ),
        (
            Ratchet {
                name: "`# jails:` block literals outside `codemod.rs`",
                rung: "16 — collect the splice primitives (plan.md §11)",
                // `compose.rs`, `add.rs`, `add/database.rs`,
                // `add/test_wiring.rs` and `doctor.rs` each built and parsed
                // the marked-block format with their own `format!`. Five
                // owners of one format is `process.rs` before it was
                // extracted, with the same consequence waiting: a change to
                // the markers, or to the rule about the trailing newline, has
                // to be made in five places and will be made in four.
                ceiling: 0,
                target: 0,
                why: "The marked block is how jails edits a file the reader owns, and it is \
                      what makes `remove` the exact inverse of `add`. A second implementation \
                      of it is a second answer to what `remove db` deletes.",
            },
            src.iter()
                .filter(|file| !file.path.ends_with("codemod.rs"))
                .map(|file| {
                    file.production.matches("# jails:").count()
                        + file.production.matches("# /jails:").count()
                })
                .sum(),
        ),
        (
            Ratchet {
                name: "`fs::write` sites outside the apply layer",
                rung: "6 — `Edit` + `apply/codemod.rs`",
                ceiling: 0,
                target: 0,
                why: "Writing is the one thing that must have a single owner, or `--pretend` \
                      cannot be trusted and a ledger hung off `write_new_file` has a hole \
                      exactly where a capability updates a file it previously wrote.",
            },
            write_sites_outside_apply(&src),
        ),
        (
            Ratchet {
                name: "`doctor` module lines (target withdrawn — §8.0.1)",
                rung: "9 — Move Method (`doctor` derives from `plan`)",
                // Rose 1123 -> 1140 while rung 1 ran (every check that took
                // `(root, pom_text)` now takes one `Project`), then 1140 ->
                // 1253 for `capability_drift_checks`, which is rung 9's
                // *additive* half: `doctor` finally re-plans each recorded
                // capability through `add::plan_for` and reports the delta,
                // catching a drift class nothing caught before.
                //
                // The subtractive half is what reaches 700, and it is not a
                // line-deletion exercise: the hand-written checks still cover
                // projects with no `jails.toml` capability list at all, where
                // a derived check has nothing to derive from. Removing them
                // wholesale would trade a real coverage class for a number.
                // 1253 -> 1289 for `--json`, which renders the *same*
                // `Vec<Check>` the human report prints rather than re-deriving
                // it -- the one shape that cannot describe a different run.
                // 1289 -> 1312 for `template_override_checks`: plan.md §6.6
                // states template overrides' cost as "an overridden template is
                // not golden-tested", and names this check as the mitigation.
                // Additive coverage again, not re-derivation.
                //
                // 1312 -> 1328 for the split into `doctor/environment.rs` (asks
                // the machine) and `doctor/wiring.rs` (asks the project): two
                // module headers and their imports. This gate sums the whole
                // module on purpose, so a split costs a little rather than
                // reading as a 700-line win -- which is exactly what it would
                // have done had it kept matching one filename.
                //
                // 1328 -> 1340 for plan.md §12: `doctor` names the real build
                // tool and stops reporting on a pom that is not there. Not
                // optional -- fifteen greens over a build jails cannot see is
                // §8.9's failure in a new disguise.
                ceiling: 1340,
                // Withdrawn, not reached. abstract.md §8.0.1 audits all ten
                // checks: none is a re-encoded dependency fact, so the 700
                // measured a saving that is not there. Ratchet against growth.
                target: 1340,
                why: "Feature Envy at module scale: doctor re-derives by reading the project \
                      back off disk the facts `add/*` already own, and the drift between them \
                      is a class nothing catches.",
            },
            // The whole module, not one file: splitting `doctor.rs` into
            // `doctor/mod.rs` + submodules would take a gate that reads one
            // filename to zero, which is gaming rather than closing it. Rung 9
            // is about how much `doctor` re-derives, and that does not change
            // when the lines move to a sibling file.
            src.iter()
                .filter(|file| {
                    file.path.ends_with("doctor.rs")
                        || file.path.to_string_lossy().contains("doctor/")
                })
                .map(production_lines)
                .sum(),
        ),
        (
            Ratchet {
                name: "`spring.rs` lines",
                rung: "10–11 — Extract Class; split by secret",
                // Re-baselined once, from 3449, when the row below reached 0.
                // While Java still lived inline, `blank` erased those bodies and
                // this number could not see them -- so it flattered the file.
                // With every body in `templates/spring/*.java`, what remains is
                // genuinely Rust: the `render` call and its key list. The raw
                // file fell 6,624 -> 5,517 in the same change and 4,596 lines
                // became real Java an editor can check. This number is now
                // honest, and only rung 11's split moves it.
                // Closed by rung 11's split: `workflow`, `durable`, `http` and
                // `schema` are now their own modules under `src/spring/`, and
                // what is left here is the shared precondition plus the
                // capability slices. The row below guards against the obvious
                // way to cheat this one -- moving a monolith rather than
                // decomposing it.
                // 1,268 -> 688 when rung 11 finished: `dto.rs`, `messaging.rs`
                // and `security.rs` left, and what remains is the shared
                // preconditions plus the three small capabilities that have no
                // second reader.
                ceiling: 673,
                target: 2500,
                why: "Logical cohesion: one file for everything sharing the `require_spring` \
                      precondition. abstract.md §6.2 says turning that precondition into data \
                      dissolves the file along real seams. Counted as lines of *decisions*: \
                      test modules are blanked, which is the number plan.md §6.2 C sets its \
                      2,500 target against.",
            },
            src.iter()
                .find(|file| file.path.ends_with("spring.rs"))
                .map_or(0, production_lines),
        ),
        (
            Ratchet {
                name: "production lines in the largest module",
                rung: "11 — Move Module (split by secret)",
                // 2390, and it is `generate.rs` rather than anything under
                // `src/spring/` -- so the split decomposed rather than
                // relocated, and the next monolith was already the one
                // abstract.md §3.2 calls Ousterhout's named anti-pattern
                // verbatim: parse -> dispatch -> write -> side effects.
                ceiling: 673,
                target: 700,
                why: "The row above can be satisfied by *moving* a monolith rather than \
                      decomposing one, so this asks the question the split is actually for: \
                      is there still a file nobody can hold in their head? It is measured \
                      across every module, so cutting one in half only helps if the halves \
                      are genuinely smaller.",
            },
            src.iter().map(production_lines).max().unwrap_or(0),
        ),
        (
            Ratchet {
                name: "inline Java bodies in `spring.rs` (`r#\"package `)",
                rung: "10 — templates out of `spring.rs`",
                ceiling: 0,
                target: 0,
                why: "Every brace doubled, and no editor or compiler can check it. This is \
                      the exact tax `src/template.rs` exists to remove.",
            },
            inline_java_bodies(&src),
        ),
    ]
}

#[test]
fn the_abstract_md_ladder_gates_are_ratchets_that_only_move_down() {
    let rows = gates();
    let mut rose = Vec::new();
    let mut fell = Vec::new();

    println!("\nabstract.md §7 ladder — gate status\n");
    println!(
        "{:<52} {:>7} {:>9} {:>8}  rung",
        "gate", "now", "ceiling", "target"
    );
    for (gate, actual) in &rows {
        let mark = if *actual > gate.ceiling {
            "ROSE"
        } else if *actual < gate.ceiling {
            "FELL"
        } else if gate.ceiling <= gate.target {
            "done"
        } else {
            "held"
        };
        println!(
            "{:<52} {:>7} {:>9} {:>8}  {:<6} {}",
            gate.name, actual, gate.ceiling, gate.target, mark, gate.rung
        );
        if *actual > gate.ceiling {
            rose.push((gate, *actual));
        } else if *actual < gate.ceiling {
            fell.push((gate, *actual));
        }
    }
    println!();

    let mut report = String::new();
    for (gate, actual) in &rose {
        report.push_str(&format!(
            "\nREGRESSED: {} is {actual}, above the recorded ceiling of {}.\n  rung {}\n  {}\n  \
             Either bring the number back down, or -- if the rise is deliberate and \
             justified -- say why in the commit and raise the ceiling in the same change.\n",
            gate.name, gate.ceiling, gate.rung, gate.why
        ));
    }
    for (gate, actual) in &fell {
        report.push_str(&format!(
            "\nIMPROVED, RECORD IT: {} is {actual}, below the recorded ceiling of {}.\n  \
             rung {}\n  Lower this row's `ceiling` to {actual} in tests/architecture.rs. An \
             improvement that is not recorded here is one the next change may silently \
             undo, which is exactly the failure abstract.md §8.1 documented.\n",
            gate.name, gate.ceiling, gate.rung
        ));
    }
    assert!(report.is_empty(), "{report}");
}

/// Every gate that has reached its target should stay at it.
/// The declared list must name exactly the readers that are really there.
///
/// Both directions. A name that has gone means the function was fixed or
/// renamed and the reason is now permission for nothing; a reader that is not
/// named is one nobody decided about.
#[test]
fn every_fresh_read_of_the_pom_is_a_decision_somebody_wrote_down() {
    let src = sources();
    let found: std::collections::BTreeSet<String> =
        rederivers(&src).into_iter().map(|(_, name)| name).collect();
    let declared: std::collections::BTreeSet<String> = A_FRESH_READ_IS_CORRECT
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();

    assert_eq!(
        found, declared,
        "\n\nA root-taking function reads the pom for a fact a resolved `Project` holds.\n\
         Either pass it the `Project` -- which is rung 1 -- or, if reading again is \
         genuinely correct, add it to A_FRESH_READ_IS_CORRECT with the reason.\n\
         A name in the list that is no longer found has to come out: a reason nobody \
         needs is permission nobody asked for."
    );
    assert!(
        A_FRESH_READ_IS_CORRECT
            .iter()
            .all(|(_, why)| why.len() > 40),
        "every reason has to say why a `Project` would be wrong, not merely that it is allowed"
    );
}

#[test]
fn a_gate_that_reached_its_target_is_never_reopened() {
    for (gate, actual) in gates() {
        if gate.ceiling <= gate.target {
            assert!(
                actual <= gate.target,
                "{} closed at {} and is now {actual}. A closed gate reopening means the \
                 rung was reverted without the ladder being updated.\n  {}",
                gate.name,
                gate.target,
                gate.why
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Measurement
// ---------------------------------------------------------------------------

struct Source {
    path: PathBuf,
    /// Comments and string literals replaced by spaces of the same length --
    /// so byte offsets and line counts still index the original -- with every
    /// `#[cfg(test)] mod … { … }` body also blanked.
    ///
    /// abstract.md §3 states its line counts exclude `mod tests`, and every
    /// gate here means the same thing: a fixture that writes a scratch `pom.xml`
    /// is not a production `fs::write`, and a test helper taking `root: &Path`
    /// is not the primitive being propagated. Counting them makes the ladder
    /// punish the tests that prove a rung did not change behaviour.
    production: String,
}

fn sources() -> Vec<Source> {
    let mut files = Vec::new();
    collect(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path(),
        &mut files,
    );
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

fn collect(dir: &Path, out: &mut Vec<Source>) {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("failed to read a directory entry").path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            let production = without_test_modules(&blank(&source));
            out.push(Source { path, production });
        }
    }
}

/// Replace comments and string literals with spaces of the same length.
///
/// The same trick as `src/java.rs::blanked`, and for the same reason: a scan
/// must not be fooled by `// root: &Path` or by the word `fn` inside one of
/// `spring.rs`'s inline Java bodies, while offsets and line numbers still line
/// up with the file on disk.
fn blank(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
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
                if source[i..].starts_with("/*") {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if source[i..].starts_with("*/") {
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
                out.push_str(&" ".repeat(open));
                i += open;
                while i < bytes.len() && !source[i..].starts_with(&close) {
                    out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
                let closing = close.len().min(bytes.len().saturating_sub(i));
                out.push_str(&" ".repeat(closing));
                i += closing;
            } else {
                out.push('r');
                i += 1;
            }
        } else if bytes[i] == b'"' {
            out.push(' ');
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    // A trailing backslash is Rust's line continuation, and
                    // eating its newline would make every line count here read
                    // low -- which is the whole measurement for two gates.
                    out.push(' ');
                    out.push(if bytes[i + 1] == b'\n' { '\n' } else { ' ' });
                    i += 2;
                    continue;
                }
                out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                i += 1;
            }
            if i < bytes.len() {
                out.push(' ');
                i += 1;
            }
        } else if bytes[i] == b'\'' && char_literal_len(&source[i..]).is_some() {
            let len = char_literal_len(&source[i..]).expect("checked above");
            out.push_str(&" ".repeat(len));
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
fn without_test_modules(blanked: &str) -> String {
    let bytes = blanked.as_bytes();
    let mut out = blanked.to_string();
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
        let blanked_body: String = blanked[at..=close]
            .chars()
            .map(|c| if c == '\n' { '\n' } else { ' ' })
            .collect();
        out.replace_range(at..=close, &blanked_body);
        search = close;
    }
    out
}

/// The length of a `'a'`-style character literal, or `None` for a lifetime.
fn char_literal_len(rest: &str) -> Option<usize> {
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
fn root_path_parameters(src: &[Source]) -> usize {
    src.iter()
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
/// somebody already resolved. These four ask the disk because the resolved
/// answer is **stale or absent** -- jails has been splicing the pom in the same
/// run, or there is no project yet. Declared rather than counted, because a
/// number nobody can reach is a gate nobody reads; the pattern is
/// `SILENT_WITHOUT_A_RECORD`'s, and a stale entry here fails the test below the
/// same way.
const A_FRESH_READ_IS_CORRECT: &[(&str, &str)] = &[
    (
        "project_at",
        "its own Javadoc forbids caching: `app apply` splices the pom and rewrites \
         jails.toml between steps, so step N+1 planning against step N's snapshot is \
         exactly the staleness bug this avoids",
    ),
    (
        "ensure_dependency",
        "it splices into the pom, so it must hold the current bytes -- an earlier \
         dependency added in the same run is not in any copy taken before it",
    ),
    (
        "ensure_console_launcher",
        "the same: it checks for and splices `junit-platform-console`",
    ),
    (
        "ensure_package_info",
        "reached from `write_new_file`, whose nine callers include `new` -- which is \
         creating the very pom this asks about, so there is no project to have resolved",
    ),
];

/// Functions that exist *to* turn a path into project facts, so re-deriving is
/// their whole job rather than envy of someone else's.
///
/// `new` is here in full: it runs before a project exists to resolve, which is
/// the one situation where there is no `Project` to have been passed.
const DERIVATION_IS_THE_JOB: &[&str] = &[
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
fn rederivers(src: &[Source]) -> Vec<(String, String)> {
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
fn production_lines(file: &Source) -> usize {
    file.production
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
}

fn count_matches(src: &[Source], needle: &str) -> usize {
    src.iter()
        .map(|file| file.production.matches(needle).count())
        .sum()
}

/// Count `fn` declarations in one file whose parameter list exceeds five.
fn over_five_params(src: &[Source], file_suffix: &str) -> usize {
    src.iter()
        .filter(|file| file.path.ends_with(file_suffix))
        .map(|file| {
            fn_param_counts(&file.production)
                .into_iter()
                .filter(|(_, count)| *count > 5)
                .count()
        })
        .sum()
}

/// Every `fn` in blanked Rust, with the number of top-level parameters.
fn fn_param_counts(blanked: &str) -> Vec<(String, usize)> {
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

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The offset of the `(` opening a function's parameter list, skipping any
/// generic parameters, which may themselves contain parentheses in a bound.
fn find_param_list(rest: &str) -> Option<usize> {
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

fn matching_paren(source: &str, open: usize) -> Option<usize> {
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
fn top_level_commas(inner: &str) -> usize {
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

fn body_carrying_structs(src: &[Source]) -> usize {
    let mut found = 0;
    for file in src {
        let mut in_struct = false;
        for line in file.production.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("struct ") || trimmed.starts_with("pub(crate) struct ") {
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
fn file_tuple_types(src: &[Source]) -> usize {
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
fn type_aliases(src: &[Source]) -> usize {
    src.iter()
        .map(|file| {
            file.production
                .lines()
                .filter(|line| {
                    let line = line.trim();
                    line.starts_with("type ") || line.starts_with("pub(crate) type ")
                })
                .filter(|line| line.contains("= Change;") || line.contains("= Artifact;"))
                .count()
                + file.production.matches("Artifact as NewFile").count()
                + file.production.matches("Change as Plan").count()
        })
        .sum()
}

fn write_sites_outside_apply(src: &[Source]) -> usize {
    src.iter()
        .filter(|file| {
            !file
                .path
                .components()
                .any(|part| part.as_os_str() == "apply")
        })
        .map(|file| file.production.matches("fs::write").count())
        .sum()
}

fn inline_java_bodies(src: &[Source]) -> usize {
    // Counted on the *raw* source: `blank` deliberately erases these bodies,
    // which is what makes them invisible to every other measurement here.
    src.iter()
        .filter(|file| file.path.ends_with("spring.rs"))
        .map(|file| {
            fs::read_to_string(&file.path)
                .expect("spring.rs was read once already")
                .matches("r#\"package ")
                .count()
        })
        .sum()
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
