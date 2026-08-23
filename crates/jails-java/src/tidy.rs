//! The shape rules a generated Java file gets at write time.
//!
//! Import order and blank lines: the two things palantir-java-format decides
//! that a template would otherwise have to get right by hand. CLAUDE.md states
//! both as write-time rules rather than template rules, and the reason is
//! decay: the next template gets it wrong and nobody notices until `jails add
//! format` makes `mvn verify` fail on a freshly generated project.
//!
//! They live here, below every producer, because there are two write paths now
//! -- the direct one and the projected one -- and a rule that only one of them
//! applies is a rule that produces two different files from one recipe. Both
//! were found that way: the projected path emitted a template's own import
//! order, and then its doubled blank lines.

/// Rewrite a generated file's import block into the order
/// palantir-java-format produces: static imports first, a blank line, then
/// everything else sorted.
///
/// Done here, once, rather than by hand in each of the twenty-odd templates.
/// Hand-ordering is a rule that decays -- the next template gets it wrong and
/// nobody notices until `jails add format` makes `mvn verify` fail on a
/// freshly generated project, which is a bad first impression for a scaffold
/// to make.
pub fn normalize_imports(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();

    let Some(package_at) = lines
        .iter()
        .position(|l| l.trim_start().starts_with("package "))
    else {
        return source.to_string();
    };

    // Imports are only ever between the package declaration and the first
    // other construct, so scanning stops at the first line that is neither an
    // import nor blank -- a Javadoc block, an annotation, the type itself.
    let mut statics: Vec<&str> = Vec::new();
    let mut plain: Vec<&str> = Vec::new();
    let mut end = package_at + 1;
    for (offset, line) in lines[package_at + 1..].iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("import ") {
            if rest.starts_with("static ") {
                statics.push(trimmed);
            } else {
                plain.push(trimmed);
            }
            end = package_at + 1 + offset + 1;
            continue;
        }
        break;
    }

    if statics.is_empty() && plain.is_empty() {
        return source.to_string();
    }

    statics.sort_unstable();
    statics.dedup();
    plain.sort_unstable();
    plain.dedup();

    let mut out = String::with_capacity(source.len() + 32);
    for line in &lines[..=package_at] {
        out.push_str(line);
        out.push('\n');
    }
    out.push('\n');
    for group in [&statics, &plain] {
        if group.is_empty() {
            continue;
        }
        for line in group.iter() {
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }
    // Whatever followed the imports, with any blank lines it was padded with
    // already consumed above.
    for line in lines[end..].iter().skip_while(|l| l.trim().is_empty()) {
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Collapse the blank lines a template leaves behind when an optional section
/// renders empty, and end the file with exactly one newline.
///
/// Here for the same reason `normalize_imports` is: **palantir-java-format
/// removes both**, so leaving them in means `add format` -- which jails
/// installs itself -- fails `jails check` on a project whose every line jails
/// wrote. That is not hypothetical. It is what App D (`examples/ledger-cli`)
/// hit on its first gate run, in four files, because it is the first proof
/// application to ask for `format` at all: `class NoteTest {` followed by two
/// blank lines wherever the sample block was omitted, and a
/// `package-info.java` ending on a blank line after its import.
///
/// Fixing it in each template is the rule-twenty-templates-must-remember that
/// this write path exists to avoid.
///
/// **Text blocks are left alone.** A `"""` block is the one Java literal that
/// can span lines, so a blank line inside one is data -- SQL, JSON, an
/// expected message -- and collapsing it would change what the program says.
/// Counting the delimiters is enough to know which side of one a line is on.
pub fn tidy_blank_lines(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut in_text_block = false;
    let mut previous_blank = false;
    for line in source.lines() {
        let blank = line.trim().is_empty();
        if !in_text_block && blank && previous_blank {
            continue;
        }
        out.push_str(line);
        out.push('\n');
        if line.matches("\"\"\"").count() % 2 == 1 {
            in_text_block = !in_text_block;
        }
        previous_blank = blank && !in_text_block;
    }
    // A file that ends on a blank line is the same violation at the bottom.
    let trimmed = out.trim_end_matches('\n');
    if trimmed.is_empty() {
        return String::new();
    }
    format!("{trimmed}\n")
}
