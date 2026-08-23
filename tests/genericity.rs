use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN: &[&str] = &[
    "crawl",
    "spider",
    "inbox",
    "payment",
    "merchant",
    "settlement",
    "ledger",
    "robots",
    // App D's vocabulary, and the half that carries no `ledger`-style
    // collision with jails' own machinery -- so these stay unallowed
    // everywhere and are what make the `ledger` allowance below narrow rather
    // than a hole. `transaction` is deliberately absent: `@Transactional` is
    // Spring's, and banning it would forbid generated code jails must emit.
    "debit",
    "posting",
];

struct AllowedConcept {
    word: &'static str,
    files: &'static [&'static str],
    reason: &'static str,
}

const ALLOWED: &[AllowedConcept] = &[
    AllowedConcept {
        word: "robots",
        files: &[
            // Followed the http-workflow generator out of `spring.rs` when rung 11
            // split it; the concept is unchanged, only its address.
            "crates/jails-generate/src/spring/http.rs",
            "templates/spring/http_workflow_java.java",
            "templates/spring/http_workflow_it_java.java",
        ],
        reason: "RFC 9309 names robots.txt; this is a web standard, not showcase-domain vocabulary",
    },
    AllowedConcept {
        word: "inbox",
        files: &["templates/spring/mailer_it_java.java"],
        reason: "POP3's own folder name -- `store.getFolder(\"inbox\")` is the protocol, not \
                 App B's domain, and Spring Boot's own mail integration test spells it the \
                 same way. The variable holding it is called `folder`, so this is the one \
                 occurrence: the string the protocol requires",
    },
    AllowedConcept {
        word: "ledger",
        files: &[
            "crates/jails-project/src/ledger.rs",
            "crates/jails-project/src/generated_files.rs",
            // The read-only machine-state facade classifies the ledger file
            // and the legacy sources a migration will retire.
            "crates/jails-project/src/compat.rs",
            // The crate root has to declare `pub mod ledger;` and say what it
            // is for. Same word, same reason, one level up.
            "crates/jails-project/src/lib.rs",
            // `ProjectPath` must refuse `.jails/ledger.toml` by name, and the
            // test that proves it has to spell the path it is refusing.
            "crates/jails-protocol/src/identity.rs",
            // The schema-2 envelope is the ledger file format; its constants
            // and messages name the thing they describe.
            "crates/jails-protocol/src/envelope.rs",
            // Bootstrap order is *defined* by reading the ledger first, so it
            // cannot be described without naming it.
            "crates/jails-protocol/src/bootstrap.rs",
            // `LedgerIntent` is what a plan says the store should hold
            // afterwards, and the guard it carries is the ledger generation.
            "crates/jails-protocol/src/plan.rs",
            // A transition is chosen against the ledger's pending-conflict
            // state, and its tests construct one to prove the pairing.
            "crates/jails-protocol/src/transition.rs",
            // A frozen conflict carries the complete ledger state a
            // resolution will promote; that is what it is.
            "crates/jails-protocol/src/pending.rs",
            // A prepared transaction guards the ledger file it will replace,
            // and its semantics carry the intent for it.
            "crates/jails-prepare/src/prepare.rs",
            "crates/jails-prepare/src/operation.rs",
            // Preparation guards the ledger generation the plan was computed
            // against, and renders the image the commit will write.
            "crates/jails-prepare/src/pipeline.rs",
            // A report, a receipt and a command envelope each say what
            // happened to the ledger file, which is the name of the thing.
            "crates/jails-prepare/src/report.rs",
            "crates/jails-prepare/src/receipt.rs",
            "crates/jails-prepare/src/command.rs",
            "crates/jails-prepare/src/serialize.rs",
            // A journal names the ledger-committed phase, which is the point
            // after which recovery must roll forward rather than back.
            "crates/jails-commit/src/journal.rs",
            // The commit point *is* the ledger write; the executor cannot
            // describe its own protocol without naming it.
            "crates/jails-commit/src/execute.rs",
            // Recovery classifies the ledger first: it is what says whether
            // the commit point was crossed.
            "crates/jails-commit/src/recover.rs",
            // The failpoint names are the commit protocol's own steps, and
            // three of them are the ledger transition.
            "crates/jails-commit/src/fault.rs",
            "src/app.rs",
            // The typed shadow reads the schema-1 store's rows to build its
            // observed side, so it names the module they live in.
            "src/app/shadow.rs",
            "crates/jails-generate/src/generate/remove.rs",
            "src/main.rs",
        ],
        reason: "jails' own bookkeeping file (`abstract.md` §6.3 names it), which collides \
                 with App D's domain by accident -- the word is the storage's, not the \
                 accounting concept's. `debit` and `posting` stay forbidden in these same \
                 files, so the allowance is one word wide rather than a way in for the domain",
    },
];

#[test]
fn core_generation_stays_free_of_showcase_vocabulary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();
    let mut used_allowances = vec![false; ALLOWED.len()];
    // Per *file*, not just per concept: an allowance whose word still appears
    // somewhere keeps the whole entry alive, so a path that has gone stale --
    // the code moved to a sibling module -- would sit there unnoticed,
    // permitting a word in a file that no longer says it.
    let mut used_files: Vec<Vec<bool>> = ALLOWED
        .iter()
        .map(|allowed| vec![false; allowed.files.len()])
        .collect();
    for allowed in ALLOWED {
        assert!(
            !allowed.reason.trim().is_empty(),
            "allow-list reasons are load-bearing"
        );
        assert!(
            !allowed.files.is_empty(),
            "an allowed concept must name its exact implementation files"
        );
    }
    // Every crate, not only the binary's own `src/`. A scanner that walked one
    // package would have reported a clean sweep over five crates it never read,
    // which is the same failure as a skipped tier-3 test: green, and meaningless.
    let mut scopes = vec![root.join("src"), root.join("templates")];
    let crates = root.join("crates");
    if crates.is_dir() {
        let mut members: Vec<PathBuf> = fs::read_dir(&crates)
            .expect("failed to read crates/")
            .map(|entry| entry.expect("failed to read a crates/ entry").path())
            .collect();
        members.sort();
        scopes.extend(members.into_iter().map(|member| member.join("src")));
    }
    let mut scanned = 0;
    for scope in scopes {
        for path in source_files(&scope) {
            scanned += 1;
            let relative = path.strip_prefix(root).unwrap();
            let source = fs::read_to_string(&path).unwrap();
            let visible = without_comments(&source);
            for word in FORBIDDEN {
                for offset in word_offsets(&visible, word) {
                    let allowance = ALLOWED.iter().enumerate().find_map(|(index, allowed)| {
                        if allowed.word != *word {
                            return None;
                        }
                        allowed
                            .files
                            .iter()
                            .position(|file| relative == Path::new(file))
                            .map(|file| (index, file))
                    });
                    if let Some((index, file)) = allowance {
                        used_allowances[index] = true;
                        used_files[index][file] = true;
                    } else {
                        let line = visible[..offset]
                            .bytes()
                            .filter(|byte| *byte == b'\n')
                            .count()
                            + 1;
                        failures.push(format!("{}:{line}: {word}", relative.display()));
                    }
                }
            }
        }
    }
    assert!(
        scanned > 60,
        "the genericity sweep read only {scanned} files -- it has lost track of \
         where the code lives, and a clean result would mean nothing"
    );
    for ((allowed, used), files) in ALLOWED.iter().zip(used_allowances).zip(&used_files) {
        assert!(
            used,
            "stale genericity allowance for `{}` ({})",
            allowed.word, allowed.reason
        );
        let stale: Vec<&str> = allowed
            .files
            .iter()
            .zip(files)
            .filter(|(_, used)| !**used)
            .map(|(file, _)| *file)
            .collect();
        assert!(
            stale.is_empty(),
            "genericity allowance for `{}` names {} file(s) that no longer contain it: {stale:?}. \
             Take them out -- an allow-list nobody prunes is one that permits more than it says.",
            allowed.word,
            stale.len()
        );
    }
    assert!(
        failures.is_empty(),
        "showcase vocabulary leaked into generic code/templates:\n{}\n\n\
         Replace it with a generic primitive. Do not grow the allow-list unless the word is a \
         named external standard and the reason says which one.",
        failures.join("\n")
    );
}

fn source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("rs" | "java")
            ) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn word_offsets(text: &str, wanted: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut offsets = Vec::new();
    let mut token_start = None;
    for at in 0..=bytes.len() {
        let current = bytes.get(at).copied();
        let previous = at
            .checked_sub(1)
            .and_then(|index| bytes.get(index))
            .copied();
        let next = bytes.get(at + 1).copied();
        let separator = current.is_none_or(|value| !value.is_ascii_alphanumeric());
        let camel_boundary = current.is_some_and(|value| {
            value.is_ascii_uppercase()
                && (previous.is_some_and(|prior| prior.is_ascii_lowercase())
                    || (previous.is_some_and(|prior| prior.is_ascii_uppercase())
                        && next.is_some_and(|following| following.is_ascii_lowercase())))
        });
        if separator || camel_boundary {
            if let Some(start) = token_start.take() {
                let token = text[start..at].to_ascii_lowercase();
                if token.starts_with(wanted) {
                    offsets.push(start);
                }
            }
            if camel_boundary {
                token_start = Some(at);
            }
        } else if token_start.is_none() {
            token_start = Some(at);
        }
    }
    offsets
}

/// Mask Rust/Java line and block comments while preserving byte offsets.
/// String literals remain visible because generated vocabulary inside a
/// template is still generated vocabulary; comment markers inside a quoted
/// string are not mistaken for source comments.
fn without_comments(source: &str) -> String {
    #[derive(Clone, Copy)]
    enum State {
        Code,
        Line,
        Block(usize),
        Quoted(u8, bool),
        Raw(usize),
    }

    let bytes = source.as_bytes();
    let mut out = bytes.to_vec();
    let mut state = State::Code;
    let mut at = 0;
    while at < bytes.len() {
        match state {
            State::Code if bytes[at..].starts_with(b"//") => {
                out[at] = b' ';
                if at + 1 < out.len() {
                    out[at + 1] = b' ';
                }
                at += 2;
                state = State::Line;
            }
            State::Code if bytes[at..].starts_with(b"/*") => {
                out[at] = b' ';
                out[at + 1] = b' ';
                at += 2;
                state = State::Block(1);
            }
            State::Code if bytes[at] == b'r' => {
                let hashes = bytes[at + 1..]
                    .iter()
                    .take_while(|byte| **byte == b'#')
                    .count();
                if bytes.get(at + 1 + hashes) == Some(&b'"') {
                    at += 2 + hashes;
                    state = State::Raw(hashes);
                } else {
                    at += 1;
                }
            }
            // Only double-quoted strings need protecting from `//` and
            // `/*`. Treating every apostrophe as a character literal would
            // mistake Rust lifetimes (`&'static str`) for strings and hide
            // the source until some later apostrophe.
            State::Code if bytes[at] == b'"' => {
                state = State::Quoted(bytes[at], false);
                at += 1;
            }
            State::Code => at += 1,
            State::Line if bytes[at] == b'\n' => {
                state = State::Code;
                at += 1;
            }
            State::Line => {
                out[at] = b' ';
                at += 1;
            }
            State::Block(depth) if bytes[at..].starts_with(b"/*") => {
                out[at] = b' ';
                out[at + 1] = b' ';
                at += 2;
                state = State::Block(depth + 1);
            }
            State::Block(depth) if bytes[at..].starts_with(b"*/") => {
                out[at] = b' ';
                out[at + 1] = b' ';
                at += 2;
                state = if depth == 1 {
                    State::Code
                } else {
                    State::Block(depth - 1)
                };
            }
            State::Block(_) => {
                if bytes[at] != b'\n' {
                    out[at] = b' ';
                }
                at += 1;
            }
            State::Quoted(quote, escaped) => {
                let byte = bytes[at];
                state = if escaped {
                    State::Quoted(quote, false)
                } else if byte == b'\\' {
                    State::Quoted(quote, true)
                } else if byte == quote {
                    State::Code
                } else {
                    State::Quoted(quote, false)
                };
                at += 1;
            }
            State::Raw(hashes) => {
                if bytes[at] == b'"'
                    && bytes
                        .get(at + 1..at + 1 + hashes)
                        .is_some_and(|tail| tail.iter().all(|byte| *byte == b'#'))
                {
                    at += 1 + hashes;
                    state = State::Code;
                } else {
                    at += 1;
                }
            }
        }
    }
    String::from_utf8(out).unwrap()
}
