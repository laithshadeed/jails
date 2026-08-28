use super::parse_cst;
use crate::Diagnostics;

/// Format a valid JDL v1 document without changing declaration ordering.
///
/// The formatter owns whole-document layout, unlike ordinary CST edits. It
/// canonicalizes line endings, indentation, blank-line runs, horizontal
/// trailing space, and the final newline while retaining declaration order and
/// comment text. Token wrapping and member reordering remain deliberately
/// separate rules.
pub fn format(input: &str) -> Result<String, Diagnostics> {
    parse_cst(input)?;
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut brace_depth = 0_u32;
    let mut delimiter_depth = 0_u32;
    let mut lines = Vec::new();
    let mut previous_blank = false;
    for line in normalized.lines() {
        let line = line.replace('\t', "    ");
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !previous_blank && !lines.is_empty() {
                lines.push(String::new());
            }
            previous_blank = true;
            continue;
        }
        previous_blank = false;
        let shape = line_shape(trimmed);
        let line_brace_depth = brace_depth.saturating_sub(shape.leading_close_braces);
        let line_delimiter_depth = delimiter_depth.saturating_sub(shape.leading_close_delimiters);
        let continuation = u32::from(line_delimiter_depth > 0);
        lines.push(format!(
            "{}{}",
            "  ".repeat((line_brace_depth + continuation) as usize),
            trimmed
        ));
        brace_depth = brace_depth
            .saturating_add(shape.open_braces)
            .saturating_sub(shape.close_braces);
        delimiter_depth = delimiter_depth
            .saturating_add(shape.open_delimiters)
            .saturating_sub(shape.close_delimiters);
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    let mut output = lines.join("\n");
    output.push('\n');
    Ok(output)
}

#[derive(Default)]
struct LineShape {
    open_braces: u32,
    close_braces: u32,
    open_delimiters: u32,
    close_delimiters: u32,
    leading_close_braces: u32,
    leading_close_delimiters: u32,
}

fn line_shape(line: &str) -> LineShape {
    let mut shape = LineShape::default();
    let mut string = false;
    let mut escaped = false;
    let mut leading = true;
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        if string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                string = false;
            }
            continue;
        }
        if character == '/' && characters.peek() == Some(&'/') {
            break;
        }
        match character {
            '"' => {
                string = true;
                leading = false;
            }
            '{' => {
                shape.open_braces += 1;
                leading = false;
            }
            '}' => {
                shape.close_braces += 1;
                if leading {
                    shape.leading_close_braces += 1;
                }
            }
            '(' | '[' => {
                shape.open_delimiters += 1;
                leading = false;
            }
            ')' | ']' => {
                shape.close_delimiters += 1;
                if leading {
                    shape.leading_close_delimiters += 1;
                }
            }
            character if character.is_whitespace() => {}
            _ => leading = false,
        }
    }
    shape
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
        assert!(once.contains("  pkg com.example.demo\n"));
        assert!(once.ends_with('\n'));
    }

    #[test]
    fn formatter_uses_two_space_structure_and_one_blank_line() {
        let input = "jdl 1\n\n\napp Demo {\n pkg com.example.demo\n java 26\n platform spring\n build maven\n storage none\n}\n\n\nentity Task {\n id: uuid @pk\n command Create(id) {\n route POST \"/tasks/{id}\"\n }\n}\n";
        let formatted = format(input).unwrap();
        assert!(formatted.contains("\n\napp Demo {\n  pkg"), "{formatted}");
        assert!(formatted.contains("\n  command Create(id) {\n    route"));
        assert!(!formatted.contains("\n\n\n"));
        assert_eq!(format(&formatted).unwrap(), formatted);
        assert_eq!(
            crate::parse_jdl(input).unwrap(),
            crate::parse_jdl(&formatted).unwrap()
        );
    }
}
