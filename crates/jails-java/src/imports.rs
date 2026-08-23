//! The one rule about how a generated Java file's imports are ordered.
//!
//! CLAUDE.md states it as a write-time rule rather than a template rule, and
//! the reason is decay: hand-ordering imports in twenty templates means the
//! next template gets it wrong and nobody notices until `jails add format`
//! makes `mvn verify` fail on a freshly generated project.
//!
//! It lives here, below every producer, because there are two write paths now
//! -- the direct one and the projected one -- and a rule that only one of them
//! applies is a rule that produces two different files from one recipe.

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
