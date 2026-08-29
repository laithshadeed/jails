//! A deliberately small Java reader, shared by `routes`, `beans` and
//! `rename`.
//!
//! This is not a parser and must not grow into one. It answers three
//! questions -- which annotations sit on which declaration, what a type
//! declares itself to be, and what its constructor asks for -- by scanning
//! text with comments and string literals blanked out first. That blanking
//! is the whole trick: `// @Service` in a comment and `"@Service"` in a
//! string are the two ways a naive grep gets this wrong, and both stop
//! existing once they are replaced by spaces of the same length (byte
//! offsets stay valid, so slices taken here still index the original).
//!
//! Where it is wrong it is wrong in the direction of reporting less, never
//! of reporting something that is not there: an annotation assembled at
//! runtime, or a mapping path built by string concatenation, is invisible
//! to it. `jails routes` says so in its own output rather than pretending
//! to completeness it cannot have.

use std::fs;
use std::path::{Path, PathBuf};

/// Every `.java` file under `dir`, sorted, so output is stable across runs
/// (`read_dir` order is filesystem-dependent and not otherwise sorted).
pub fn source_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // DirEntry already carries the type on the common filesystems.
            // Calling Path::is_dir here performs another metadata lookup for
            // every source file in a project-wide inspect/rename scan.
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "java") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// The source with every comment and string/char literal replaced by spaces,
/// preserving length so byte offsets into the result also index the original.
pub fn blanked(source: &str) -> String {
    masked(source, true)
}

/// One allocation and one scanner for both public masking modes.
///
/// Starting from a memcpy of the source is substantially cheaper than
/// filling a same-sized buffer with spaces and copying ordinary source code
/// one byte at a time. Generated Java is overwhelmingly ordinary code; only
/// the comparatively small comment/literal ranges need rewriting.
fn masked(source: &str, comments: bool) -> String {
    let bytes = source.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                if comments {
                    blank_range(&mut out, start, i);
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                let start = i;
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
                if comments {
                    blank_range(&mut out, start, i);
                }
            }
            // Text blocks first: `"""` would otherwise read as an empty
            // string literal followed by an unterminated one.
            b'"' if bytes[i..].starts_with(br#"""""#) => {
                let start = i;
                i += 3;
                while i + 2 < bytes.len() && !bytes[i..].starts_with(br#"""""#) {
                    i += 1;
                }
                i = (i + 3).min(bytes.len());
                blank_range(&mut out, start, i);
            }
            quote @ (b'"' | b'\'') => {
                let start = i;
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(bytes.len());
                blank_range(&mut out, start, i);
            }
            _ => i += 1,
        }
    }
    valid_mask(out)
}

/// The source with string and character literals blanked, but comments left
/// intact.
///
/// The mirror image of [`blanked`], and the two exist for opposite reasons.
/// A scan for annotations must ignore comments; a scan for `TODO` markers
/// must read only comments, and must still not be fooled by the word
/// appearing inside a string literal. Length is preserved either way, so line
/// and byte offsets still index the original.
pub fn without_literals(source: &str) -> String {
    masked(source, false)
}

fn valid_mask(out: Vec<u8>) -> String {
    String::from_utf8(out).unwrap_or_else(|error| {
        // The source starts as valid UTF-8 and masking only introduces ASCII,
        // so this is defensive. Reuse the allocation even on malformed input.
        let mut bytes = error.into_bytes();
        for byte in &mut bytes {
            if !byte.is_ascii() {
                *byte = b' ';
            }
        }
        String::from_utf8(bytes).expect("replacing non-ASCII bytes makes valid UTF-8")
    })
}

/// Blank a byte range to spaces, leaving newlines so line numbers survive a
/// multi-line text block.
fn blank_range(out: &mut [u8], start: usize, end: usize) {
    let end = end.min(out.len());
    for byte in &mut out[start..end] {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}

/// One annotation and the declaration it sits on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotated {
    pub name: String,
    /// One-based source line of the annotation.
    pub line: usize,
    /// The text between the annotation's parentheses, or empty when it has
    /// none (`@Service` as opposed to `@GetMapping("/x")`).
    pub args: String,
    /// What the annotation is attached to, once every other annotation and
    /// modifier between them is skipped.
    pub target: Target,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// `class`/`record`/`interface`/`enum` -- carries the type's own name.
    Type(String),
    /// A method -- carries its name and its declared return type.
    Method { name: String, returns: String },
    /// A field, parameter, or anything else this reader declines to classify.
    Other,
}

/// Every annotation in the file, in source order.
pub fn annotations(source: &str) -> Vec<Annotated> {
    let text = blanked(source);
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'@' {
            i += 1;
            continue;
        }
        // `@` also introduces Javadoc tags (already blanked) and
        // `@interface` declarations; the latter is a type, not a use.
        let start = i + 1;
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        if end == start {
            i += 1;
            continue;
        }
        // Names and arguments are sliced out of the *original* source, not
        // the blanked copy -- blanking is what makes the scan safe, but it
        // also erased the very literal `@GetMapping("/x")` is being read for.
        // Offsets are shared between the two by construction.
        let name = source[start..end].to_string();
        let mut after = skip_space(&text, end);
        let mut args = String::new();
        if after < bytes.len() && bytes[after] == b'(' {
            let close = match_paren(&text, after);
            args = source[after + 1..close].trim().to_string();
            after = close + 1;
        }
        found.push(Annotated {
            name,
            line: source[..i].bytes().filter(|byte| *byte == b'\n').count() + 1,
            args,
            target: target_at(&text, after),
        });
        i = after.max(end);
    }
    found
}

/// Walk forward past any further annotations and modifiers to whatever is
/// actually being declared.
fn target_at(text: &str, mut at: usize) -> Target {
    let bytes = text.as_bytes();
    const MODIFIERS: [&str; 10] = [
        "public",
        "protected",
        "private",
        "static",
        "final",
        "abstract",
        "default",
        "synchronized",
        "native",
        "strictfp",
    ];
    // Words seen since the last modifier: for a method the last two are
    // `<return type> <name>`, for a type they are `class <Name>`.
    let mut words: Vec<String> = Vec::new();
    loop {
        at = skip_space(text, at);
        if at >= bytes.len() {
            return Target::Other;
        }
        match bytes[at] {
            // A nested annotation on the same declaration -- skip it whole.
            b'@' => {
                let mut end = at + 1;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                {
                    end += 1;
                }
                let next = skip_space(text, end);
                at = if next < bytes.len() && bytes[next] == b'(' {
                    match_paren(text, next) + 1
                } else {
                    end
                };
            }
            // A parameter list: the two words before it are the return type
            // and the method name.
            b'(' => {
                let name = words.pop().unwrap_or_default();
                let returns = words.pop().unwrap_or_default();
                return if name.is_empty() {
                    Target::Other
                } else {
                    Target::Method { name, returns }
                };
            }
            // A field initialiser or the end of a field declaration.
            b'=' | b';' => return Target::Other,
            b'<' => {
                // A generic return type or type parameter list; fold it into
                // the word already collected rather than treating `<` as a
                // separator.
                let close = match_angle(text, at);
                if let Some(last) = words.last_mut() {
                    last.push_str(&text[at..=close.min(bytes.len() - 1)]);
                }
                at = close + 1;
            }
            b'{' => return Target::Other,
            _ => {
                let mut end = at;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric()
                        || bytes[end] == b'_'
                        || bytes[end] == b'.'
                        || bytes[end] == b'['
                        || bytes[end] == b']')
                {
                    end += 1;
                }
                if end == at {
                    return Target::Other;
                }
                let word = &text[at..end];
                at = end;
                if MODIFIERS.contains(&word) {
                    words.clear();
                    continue;
                }
                if matches!(word, "class" | "record" | "interface" | "enum") {
                    let name_start = skip_space(text, at);
                    let mut name_end = name_start;
                    while name_end < bytes.len()
                        && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'_')
                    {
                        name_end += 1;
                    }
                    return Target::Type(text[name_start..name_end].to_string());
                }
                words.push(word.to_string());
            }
        }
    }
}

/// The single type this file declares: its name, what it extends/implements,
/// and the parameter types of its widest constructor.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypeInfo {
    pub name: String,
    pub package: String,
    pub supertypes: Vec<String>,
    pub constructor_params: Vec<Param>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    /// The type with generics and package qualifiers stripped -- what bean
    /// wiring compares against.
    pub type_name: String,
    /// The type as written, generics intact. `beans` wants `List`; anything
    /// reconstructing the component's real type wants `List<Reward>`, and an
    /// `Optional<String>` flattened to `Optional` loses the only part that
    /// says what is inside it.
    pub raw_type: String,
    pub name: String,
}

/// Read the file's top-level type. Nested types are ignored: the first
/// `class`/`record`/`interface`/`enum` keyword at the outermost level wins,
/// which for jails-generated code is always the file's namesake.
pub fn type_info(source: &str) -> Option<TypeInfo> {
    let text = blanked(source);
    let package = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("package ")?.trim().strip_suffix(';'))
        .map(|p| p.trim().to_string())
        .unwrap_or_default();

    let (keyword_at, keyword) = ["class ", "record ", "interface ", "enum "]
        .iter()
        .filter_map(|kw| top_level_find(&text, kw).map(|at| (at, *kw)))
        .min_by_key(|(at, _)| *at)?;

    let bytes = text.as_bytes();
    let name_start = skip_space(&text, keyword_at + keyword.len());
    let mut name_end = name_start;
    while name_end < bytes.len()
        && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'_')
    {
        name_end += 1;
    }
    let name = text[name_start..name_end].to_string();
    if name.is_empty() {
        return None;
    }

    // Everything between the name and the body opener holds the record's
    // component list plus any extends/implements/permits clauses.
    let body_at = text[name_end..].find('{').map(|o| o + name_end)?;
    let header = &text[name_end..body_at];
    let mut supertypes = Vec::new();
    for clause in ["extends", "implements", "permits"] {
        if let Some(at) = header.find(clause) {
            let rest = &header[at + clause.len()..];
            let end = rest.find('{').unwrap_or(rest.len());
            let end = ["extends", "implements", "permits"]
                .iter()
                .filter_map(|next| rest.find(next))
                .chain(std::iter::once(end))
                .min()
                .unwrap_or(end);
            supertypes.extend(
                rest[..end]
                    .split(',')
                    .map(|t| simple_name(t.trim()))
                    .filter(|t| !t.is_empty()),
            );
        }
    }

    // A record's components are its canonical constructor's parameters.
    let constructor_params = if keyword == "record " {
        header
            .find('(')
            .map(|open| {
                let close = match_paren(header, open);
                params(&header[open + 1..close])
            })
            .unwrap_or_default()
    } else {
        widest_constructor(&text, &name)
    };

    Some(TypeInfo {
        name,
        package,
        supertypes,
        constructor_params,
    })
}

/// The parameter list of the constructor that asks for the most -- which for
/// a Spring component is the injected one.
fn widest_constructor(text: &str, class_name: &str) -> Vec<Param> {
    let mut widest: Vec<Param> = Vec::new();
    let mut from = 0;
    while let Some(rel) = text[from..].find(class_name) {
        let at = from + rel;
        from = at + class_name.len();
        // A constructor is `Name (`, and the character before the name must
        // not be part of a longer identifier (`InMemoryFoo` is not `Foo`).
        let before = text[..at].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
            continue;
        }
        let open = skip_space(text, from);
        if text.as_bytes().get(open) != Some(&b'(') {
            continue;
        }
        // `new Name(...)` and `class Name` are not constructor declarations.
        let preceding = text[..at].trim_end();
        if preceding.ends_with("new")
            || preceding.ends_with("class")
            || preceding.ends_with("record")
        {
            continue;
        }
        let close = match_paren(text, open);
        let found = params(&text[open + 1..close]);
        if found.len() > widest.len() {
            widest = found;
        }
    }
    widest
}

/// Remove each annotation *and its argument list* from a parameter list.
///
/// `bugs.md` B53: dropping annotations by discarding whitespace-separated
/// words that start with `@` works only while the argument has no spaces in
/// it. `@Value("${k:#{env.K ?: \'\'}}")` is three such words, two of which do
/// not start with `@`, so `?:` and `\'\'}}")` survived into the type and
/// `jails beans` reported a dependency called `)` that no bean could supply.
///
/// Structure is read off `blanked()` -- a `(` inside a string literal is not a
/// bracket -- and the spans are cut from the original, which is
/// `annotations()`'s rule one level down.
fn without_annotations(list: &str) -> String {
    let masked = blanked(list);
    let bytes = masked.as_bytes();
    let mut out = String::with_capacity(list.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'@' {
            out.push_str(&list[i..i + 1]);
            i += 1;
            continue;
        }
        // `@` then the annotation's name, then optionally `(...)`.
        let mut at = i + 1;
        while at < bytes.len()
            && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'_' | b'.'))
        {
            at += 1;
        }
        let open = skip_space(&masked, at);
        i = match bytes.get(open) {
            Some(b'(') => match_paren(&masked, open) + 1,
            _ => at,
        };
        // One space so `@Qualifier("x")List<Reward> rs` cannot glue the
        // annotation's neighbours into one word.
        out.push(' ');
    }
    out
}

/// Split a parameter list into (type, name) pairs. Annotations and generic
/// arguments on a parameter are dropped -- `@Qualifier("x") List<Reward> rs`
/// becomes `List` / `rs`.
fn params(list: &str) -> Vec<Param> {
    let list = &without_annotations(list);
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for c in list.chars() {
        match c {
            '<' | '(' => {
                depth += 1;
                current.push(c);
            }
            '>' | ')' => {
                depth = depth.saturating_sub(1);
                current.push(c);
            }
            ',' if depth == 0 => {
                push_param(&mut out, &current);
                current.clear();
            }
            _ => current.push(c),
        }
    }
    push_param(&mut out, &current);
    out
}

fn push_param(out: &mut Vec<Param>, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    // Drop annotations, then take the last two words.
    let cleaned: Vec<&str> = text
        .split_whitespace()
        .filter(|w| !w.starts_with('@'))
        .collect();
    if cleaned.len() < 2 {
        return;
    }
    let name = cleaned[cleaned.len() - 1].to_string();
    let raw_type = cleaned[..cleaned.len() - 1].join(" ");
    out.push(Param {
        type_name: simple_name(raw_type.trim()),
        // Package qualifier dropped, generics kept: `java.util.List<Reward>`
        // becomes `List<Reward>`.
        raw_type: raw_type
            .trim()
            .rsplit('.')
            .next()
            .unwrap_or(raw_type.trim())
            .to_string(),
        name,
    });
}

/// `java.util.List<Reward>` -> `List`. Generic arguments and package
/// qualifiers are noise for every caller here.
pub fn simple_name(t: &str) -> String {
    let t = t.split('<').next().unwrap_or(t).trim();
    let t = t.rsplit('.').next().unwrap_or(t);
    t.trim().trim_end_matches("...").to_string()
}

/// Find `needle` outside any brace nesting, so a keyword inside a method
/// body cannot be mistaken for a top-level declaration.
fn top_level_find(text: &str, needle: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ if depth == 0 && text[i..].starts_with(needle) => {
                let before = text[..i].chars().next_back();
                if !before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

pub(crate) fn skip_space(text: &str, mut at: usize) -> usize {
    let bytes = text.as_bytes();
    while at < bytes.len() && bytes[at].is_ascii_whitespace() {
        at += 1;
    }
    at
}

/// Offset of the `)` closing the `(` at `open`. Returns the last byte when
/// the source is unbalanced, so a truncated file cannot panic the caller.
pub fn match_paren(text: &str, open: usize) -> usize {
    match_delim(text, open, b'(', b')')
}

fn match_angle(text: &str, open: usize) -> usize {
    match_delim(text, open, b'<', b'>')
}

fn match_delim(text: &str, open: usize, opener: u8, closer: u8) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        if bytes[i] == opener {
            depth += 1;
        } else if bytes[i] == closer {
            depth -= 1;
            if depth == 0 {
                return i;
            }
        }
        i += 1;
    }
    bytes.len().saturating_sub(1)
}

/// The first string literal in an annotation's arguments, honouring the
/// `path =` / `value =` forms Spring accepts. Literals were blanked out of
/// the scanning copy, so this reads the original argument text.
pub fn annotation_string(args: &str) -> Option<String> {
    for key in ["path = ", "path=", "value = ", "value="] {
        if let Some(at) = args.find(key)
            && let Some(found) = first_string(&args[at + key.len()..])
        {
            return Some(found);
        }
    }
    // A bare `@GetMapping("/x")` has no key at all -- but `@GetMapping(produces
    // = "application/json")` does, and its literal is not a path.
    if args.contains('=')
        && !args.trim_start().starts_with('"')
        && !args.trim_start().starts_with('{')
    {
        return None;
    }
    first_string(args)
}

/// The first double-quoted literal in `text`, or None.
pub fn first_string(text: &str) -> Option<String> {
    let open = text.find('"')?;
    let rest = &text[open + 1..];
    let close = rest.find('"')?;
    Some(rest[..close].to_string())
}

/// The package a source file declares, if it declares one.
///
/// Read off the `package` line rather than derived from the path, for the
/// same reason `jails src` does it: a checkout's directory layout does not
/// always match its packages.
pub fn package_of(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        line.trim()
            .strip_prefix("package ")?
            .trim()
            .strip_suffix(';')
            .map(|text| text.trim().to_string())
    })
}

/// Every `.java` file under `dir` whose *top-level type* carries `annotation`,
/// with its source, in path order.
///
/// Three callers wanted this and each had written its own walk: `add`'s test
/// wiring, the V2 translation, and `doctor`. Two of the three matched a raw
/// substring, which reads `@SpringBootTest` inside a Javadoc example as a
/// declaration -- and `TestcontainersConfig`'s Javadoc contains exactly that,
/// so `add db` counted its own container config as a test needing the config
/// imported into it.
///
/// The order is the path order, because it decides the order of whatever the
/// caller does next, and a run whose result depended on how the filesystem
/// enumerated a directory is not reproducible.
pub fn types_annotated_with(dir: &std::path::Path, annotation: &str) -> Vec<JavaSource> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "java") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let on_the_type = annotations(&source).into_iter().any(|found| {
                found.name == annotation && found.target == Target::Type(stem.to_string())
            });
            if on_the_type {
                found.push(JavaSource { path, source });
            }
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

/// The same question, asked of sources the caller already holds.
///
/// The directory walk answers about *disk*, and in one transition the file a
/// later row needs may not be written yet. A caller with a projection passes
/// it here instead, so a `@SpringBootTest` an earlier row of the same apply
/// wrote counts.
pub fn types_annotated_among(
    sources: &std::collections::BTreeMap<std::path::PathBuf, String>,
    annotation: &str,
) -> Vec<JavaSource> {
    let mut found: Vec<JavaSource> = sources
        .iter()
        .filter(|(path, _)| path.extension().is_some_and(|value| value == "java"))
        .filter(|(path, source)| {
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                return false;
            };
            annotations(source).into_iter().any(|found| {
                found.name == annotation && found.target == Target::Type(stem.to_string())
            })
        })
        .map(|(path, source)| JavaSource {
            path: path.clone(),
            source: source.clone(),
        })
        .collect();
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

/// One Java file and its text, named rather than a positional pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JavaSource {
    pub path: std::path::PathBuf,
    pub source: String,
}

impl JavaSource {
    /// The top-level type's name, which for Java is always the file stem.
    pub fn type_name(&self) -> Option<&str> {
        self.path.file_stem().and_then(|stem| stem.to_str())
    }
}

/// Whether this source declares a value of any of `types`.
///
/// Read through [`blanked`], so a type named only in a Javadoc example is not
/// mistaken for one the class holds -- which is the difference between "this
/// is the project's database container config" and "this is a class whose
/// comment shows you how to write one".
pub fn declares_any_type(source: &str, types: &[&str]) -> bool {
    let code = blanked(source);
    types.iter().any(|name| code.contains(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blanking_removes_comments_and_literals_without_moving_offsets() {
        let src = r#"class A { // @Service
  String s = "@Component";
  /* @Bean */ int i;
}"#;
        let out = blanked(src);
        assert_eq!(out.len(), src.len(), "offsets must stay valid");
        assert!(!out.contains("@Service"), "{out}");
        assert!(!out.contains("@Component"), "{out}");
        assert!(!out.contains("@Bean"), "{out}");
        assert!(out.contains("class A"), "{out}");
    }

    #[test]
    fn blanking_handles_text_blocks() {
        let src = "class A { String s = \"\"\"\n  @Service \"quoted\"\n  \"\"\"; int i; }";
        let out = blanked(src);
        assert!(!out.contains("@Service"), "{out}");
        assert!(out.contains("int i"), "{out}");
        assert_eq!(out.lines().count(), src.lines().count());
    }

    #[test]
    fn blanking_keeps_unicode_code_and_reuses_byte_offsets() {
        let src = "class Café { /* naïve\ncomment */ String résumé; }";
        let out = blanked(src);
        assert_eq!(out.len(), src.len());
        assert_eq!(out.lines().count(), src.lines().count());
        assert!(out.contains("Café"), "{out}");
        assert!(out.contains("résumé"), "{out}");
        assert!(!out.contains("naïve"), "{out}");
    }

    #[test]
    fn without_literals_keeps_comments_and_drops_strings() {
        let src = "class A { // TODO fix\n  String s = \"TODO not really\";\n}";
        let out = without_literals(src);
        assert_eq!(out.len(), src.len(), "offsets must stay valid");
        assert!(out.contains("// TODO fix"), "{out}");
        assert!(!out.contains("not really"), "{out}");
        // Line count preserved, so a note's line number is the real one.
        assert_eq!(out.lines().count(), src.lines().count());
    }

    #[test]
    fn annotations_attach_to_types_and_methods() {
        let src = r#"
package com.example;
@RestController
@RequestMapping("/rewards")
public final class RewardController {
    @GetMapping("/{id}")
    public Reward byId(String id) { return null; }
    @Autowired private Foo foo;
}"#;
        let found = annotations(src);
        let by = |n: &str| found.iter().find(|a| a.name == n).cloned().unwrap();
        assert_eq!(
            by("RestController").target,
            Target::Type("RewardController".into())
        );
        assert_eq!(by("RequestMapping").args, r#""/rewards""#);
        assert_eq!(
            by("GetMapping").target,
            Target::Method {
                name: "byId".into(),
                returns: "Reward".into()
            }
        );
        assert_eq!(by("Autowired").target, Target::Other);
    }

    #[test]
    fn type_info_reads_supertypes_and_constructor() {
        let src = r#"
package com.example.persistence;
public final class InMemoryRewardRepository implements RewardRepository {
    private final Clock clock;
    public InMemoryRewardRepository(Clock clock, JdbcTemplate jdbc) { this.clock = clock; }
}"#;
        let info = type_info(src).unwrap();
        assert_eq!(info.name, "InMemoryRewardRepository");
        assert_eq!(info.package, "com.example.persistence");
        assert_eq!(info.supertypes, vec!["RewardRepository"]);
        assert_eq!(
            info.constructor_params
                .iter()
                .map(|p| p.type_name.as_str())
                .collect::<Vec<_>>(),
            vec!["Clock", "JdbcTemplate"]
        );
    }

    #[test]
    fn a_generic_component_keeps_its_argument_in_the_raw_type() {
        // `Optional` without its argument says nothing about what is inside,
        // and that is exactly what a DTO has to reconstruct.
        let src = "package p;\npublic record Reward(Optional<String> note, List<Money> lines) {}";
        let info = type_info(src).unwrap();
        assert_eq!(info.constructor_params[0].type_name, "Optional");
        assert_eq!(info.constructor_params[0].raw_type, "Optional<String>");
        assert_eq!(info.constructor_params[1].raw_type, "List<Money>");
    }

    /// An annotation argument with spaces in it is not part of the type.
    ///
    /// `bugs.md` B53: `jails beans` reported `needs )` for a constructor whose
    /// only parameter was a `String` behind a `@Value` with a SpEL default.
    /// Annotations were dropped by discarding words beginning with `@`, and
    /// `@Value("${k:#{env.K ?: ''}}")` is three words, two of which do not.
    #[test]
    fn an_annotation_argument_with_spaces_is_not_read_as_a_type() {
        let source = r#"
            package com.example.service;

            @Service
            public class AiService {
                public AiService(
                        @Value("${openrouter.api.key:#{environment.OPENROUTER_API_KEY ?: ''}}")
                                String openRouterApiKey,
                        @Qualifier("primary") MessageRepository repository) {
                }
            }
        "#;
        let info = type_info(source).expect("a class");
        let params: Vec<(&str, &str)> = info
            .constructor_params
            .iter()
            .map(|param| (param.type_name.as_str(), param.name.as_str()))
            .collect();
        assert_eq!(
            params,
            vec![
                ("String", "openRouterApiKey"),
                ("MessageRepository", "repository")
            ]
        );
    }

    #[test]
    fn type_info_reads_record_components_as_the_constructor() {
        let src = "package p;\npublic record Reward(String id, Money amount) {}";
        let info = type_info(src).unwrap();
        assert_eq!(info.name, "Reward");
        assert_eq!(
            info.constructor_params
                .iter()
                .map(|p| p.type_name.as_str())
                .collect::<Vec<_>>(),
            vec!["String", "Money"]
        );
    }

    #[test]
    fn a_new_expression_is_not_mistaken_for_a_constructor() {
        let src = "package p;\nclass Foo {\n  Foo() {}\n  static Foo make() { return new Foo(1, 2, 3); }\n}";
        let info = type_info(src).unwrap();
        assert!(
            info.constructor_params.is_empty(),
            "{:?}",
            info.constructor_params
        );
    }

    #[test]
    fn annotation_string_prefers_the_path_attribute() {
        assert_eq!(annotation_string(r#""/x""#).as_deref(), Some("/x"));
        assert_eq!(
            annotation_string(r#"path = "/y", produces = "application/json""#).as_deref(),
            Some("/y")
        );
        assert_eq!(annotation_string(r#"produces = "application/json""#), None);
        assert_eq!(annotation_string(""), None);
    }
}
