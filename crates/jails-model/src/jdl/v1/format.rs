use super::parse_cst;
use crate::Diagnostics;

/// Format a valid JDL v1 document without changing declaration ordering.
///
/// This first formatter layer canonicalizes encoding-level layout: LF line
/// endings, spaces instead of tabs, no trailing horizontal whitespace, and
/// exactly one final newline. Structural wrapping is a later rule.
pub fn format(input: &str) -> Result<String, Diagnostics> {
    parse_cst(input)?;
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = normalized
        .lines()
        .map(|line| line.replace('\t', "    ").trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    while output.ends_with('\n') {
        output.pop();
    }
    output.push('\n');
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatter_is_idempotent_and_keeps_comments() {
        let input = "jdl 1\r\n\r\napp Demo {\r\n\tpkg com.example.demo  \r\n java 26\r\n platform plain\r\n build gradle\r\n storage none\r\n}\r\n// keep me\r\n";
        let once = format(input).unwrap();
        let twice = format(&once).unwrap();
        assert_eq!(once, twice);
        assert!(once.contains("// keep me\n"));
        assert!(!once.contains('\r'));
        assert!(!once.contains('\t'));
        assert!(once.ends_with('\n'));
    }
}
