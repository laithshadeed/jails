//! Registering a generated command in the dispatcher that runs it.
//!
//! Text in, text out; nothing here opens a file.
//!
//! Every match is scoped to the registry body rather than the whole file. A
//! dispatcher's own Javadoc carries an example `commands.put(...)` line, and a
//! whole-file match reads that as a registration -- skipping a command named
//! like the example, and deleting the documentation instead of the
//! registration on removal.

/// What makes a file a jails command dispatcher: the registry type it
/// dispatches over, and the line `register_command` splices above. Both are
/// checked, because either alone shows up in files that are not dispatchers.
pub fn is_dispatcher(source: &str) -> bool {
    source.contains("SequencedMap<String, Command>") && source.contains("return commands;")
}

/// Insert the registration immediately above `return commands;`, matching that
/// line's indentation, and add `import` if the command lives elsewhere.
/// Returns `None` when the anchor is missing, so the caller can say so rather
/// than write a mangled file.
pub fn splice_registration(source: &str, command_class: &str, import: &str) -> Option<String> {
    let anchor = source.find("return commands;")?;
    let line_start = source[..anchor].rfind('\n').map(|i| i + 1)?;
    let indent: String = source[line_start..anchor].to_string();

    let mut out = String::with_capacity(source.len() + import.len() + 96);
    out.push_str(&source[..line_start]);
    out.push_str(&format!(
        "{indent}commands.put({command_class}.NAME, {command_class}::run);\n"
    ));
    out.push_str(&source[line_start..]);

    if import.is_empty() {
        return Some(out);
    }
    // Imports go after the package line; ordering is the normaliser's problem,
    // but this file already exists, so re-sort it here too.
    let package_end = out.find(";\n").map(|i| i + 2)?;
    let mut with_import = String::with_capacity(out.len() + import.len());
    with_import.push_str(&out[..package_end]);
    with_import.push('\n');
    with_import.push_str(import);
    with_import.push_str(&out[package_end..]);
    Some(crate::tidy::normalize_imports(&with_import))
}
