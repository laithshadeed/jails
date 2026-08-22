use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN: &[&str] = &[
    "crawl",
    "spider",
    "conversation",
    "workspace",
    "inbox",
    "payment",
    "merchant",
    "settlement",
    "ledger",
    "reconcile",
    "robots",
];

const ALLOWED: &[(&str, &str, &str)] = &[(
    "templates/spring/http_workflow_java.java",
    "robots",
    "RFC 9309 names robots.txt; this is a web standard, not showcase-domain vocabulary",
)];

#[test]
fn core_generation_stays_free_of_showcase_vocabulary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();
    for scope in ["src", "templates"] {
        for path in source_files(&root.join(scope)) {
            let relative = path.strip_prefix(root).unwrap();
            let source = fs::read_to_string(&path).unwrap();
            let visible = without_comments(&source);
            for word in FORBIDDEN {
                for offset in word_offsets(&visible, word) {
                    let allowed = ALLOWED.iter().any(|(file, allowed_word, reason)| {
                        assert!(
                            !reason.trim().is_empty(),
                            "allow-list reasons are load-bearing"
                        );
                        relative == Path::new(file) && word == allowed_word
                    });
                    if !allowed {
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
    let lower = text.to_ascii_lowercase();
    lower
        .match_indices(wanted)
        .filter_map(|(at, _)| {
            let before = lower[..at].bytes().next_back();
            let after = lower[at + wanted.len()..].bytes().next();
            let boundary =
                |byte: Option<u8>| byte.is_none_or(|b| !b.is_ascii_alphanumeric() && b != b'_');
            (boundary(before) && boundary(after)).then_some(at)
        })
        .collect()
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
