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
            "src/spring/http.rs",
            "src/explain.rs",
            "templates/spring/http_workflow_java.java",
            "templates/spring/http_workflow_it_java.java",
        ],
        reason: "RFC 9309 names robots.txt; this is a web standard, not showcase-domain vocabulary",
    },
    AllowedConcept {
        word: "ledger",
        files: &[
            "src/ledger.rs",
            "src/generated_files.rs",
            "src/app.rs",
            "src/generate.rs",
            "src/apply/mod.rs",
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
    for scope in ["src", "templates"] {
        for path in source_files(&root.join(scope)) {
            let relative = path.strip_prefix(root).unwrap();
            let source = fs::read_to_string(&path).unwrap();
            let visible = without_comments(&source);
            for word in FORBIDDEN {
                for offset in word_offsets(&visible, word) {
                    let allowance = ALLOWED.iter().position(|allowed| {
                        allowed.word == *word
                            && allowed.files.iter().any(|file| relative == Path::new(file))
                    });
                    if let Some(index) = allowance {
                        used_allowances[index] = true;
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
    for (allowed, used) in ALLOWED.iter().zip(used_allowances) {
        assert!(
            used,
            "stale genericity allowance for `{}` ({})",
            allowed.word, allowed.reason
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
