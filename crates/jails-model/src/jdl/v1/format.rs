use super::{TokenKind, parse_cst};
use crate::Diagnostics;

/// Format a valid JDL v1 document without changing declaration ordering.
///
/// The formatter owns whole-document layout, unlike ordinary CST edits. It
/// canonicalizes lexical spellings, line endings, indentation, blank-line
/// runs, horizontal trailing space, and the final newline while retaining
/// comment text. Token wrapping and member reordering remain deliberately
/// separate rules.
pub fn format(input: &str) -> Result<String, Diagnostics> {
    let normalized = canonicalize_tokens(input)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
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
        let ordered = order_attributes(trimmed);
        let trimmed = ordered.as_str();
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

fn canonicalize_tokens(input: &str) -> Result<String, Diagnostics> {
    let cst = parse_cst(input)?;
    let remove_asc = explicit_asc_tokens(&cst);
    let mut output = String::with_capacity(input.len());
    let mut previous_syntax = None::<String>;
    for (index, token) in cst.tokens.iter().enumerate() {
        if token.kind == TokenKind::Eof || remove_asc[index] {
            continue;
        }
        let text = token.text(cst.source());
        if token.kind == TokenKind::String {
            let decoded = serde_json::from_str::<String>(text)
                .expect("the validated JDL lexer accepts only JSON string literals");
            output.push_str(
                &serde_json::to_string(&decoded)
                    .expect("a Rust string always has a JSON representation"),
            );
        } else if token.kind == TokenKind::Word && previous_syntax.as_deref() == Some("route") {
            output.push_str(&text.to_ascii_uppercase());
        } else {
            output.push_str(text);
        }
        match token.kind {
            TokenKind::Word | TokenKind::Integer | TokenKind::String | TokenKind::Symbol => {
                previous_syntax = Some(text.to_string());
            }
            TokenKind::Newline => previous_syntax = None,
            _ => {}
        }
    }
    Ok(output)
}

fn explicit_asc_tokens(cst: &super::DocumentCst) -> Vec<bool> {
    let mut remove = vec![false; cst.tokens.len()];
    let syntax = cst
        .tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| {
            matches!(
                token.kind,
                TokenKind::Word | TokenKind::Integer | TokenKind::String | TokenKind::Symbol
            )
        })
        .collect::<Vec<_>>();
    let mut cursor = 0;
    while cursor + 2 < syntax.len() {
        if syntax[cursor].1.text(cst.source()) != "order"
            || syntax[cursor + 1].1.text(cst.source()) != "by"
            || syntax[cursor + 2].1.text(cst.source()) != "["
        {
            cursor += 1;
            continue;
        }
        cursor += 3;
        let mut expect_field = true;
        while cursor < syntax.len() {
            let (token_index, token) = syntax[cursor];
            let text = token.text(cst.source());
            if text == "]" {
                break;
            }
            if text == "," {
                expect_field = true;
            } else if expect_field {
                expect_field = false;
            } else if text == "asc" {
                remove[token_index] = true;
                if let Some((whitespace_index, whitespace)) = cst
                    .tokens
                    .get(..token_index)
                    .and_then(|tokens| tokens.iter().enumerate().next_back())
                    && whitespace.kind == TokenKind::Whitespace
                {
                    remove[whitespace_index] = true;
                }
            }
            cursor += 1;
        }
    }
    remove
}

fn order_attributes(line: &str) -> String {
    let Ok(tokens) = super::token::lex(line) else {
        return line.to_string();
    };
    let mut edits = Vec::new();
    let mut cursor = 0;
    while cursor < tokens.len() {
        if tokens[cursor].text(line) != "@" {
            cursor += 1;
            continue;
        }
        let run_start = tokens[cursor].span.start;
        let mut attributes = Vec::new();
        let mut next = cursor;
        let mut run_end = run_start;
        while let Some((end, name)) = attribute_end(line, &tokens, next) {
            run_end = tokens[end - 1].span.end;
            attributes.push((
                attribute_rank(name),
                line[tokens[next].span.start..run_end].to_string(),
            ));
            next = end;
            while tokens
                .get(next)
                .is_some_and(|token| token.kind == TokenKind::Whitespace)
            {
                next += 1;
            }
            if tokens.get(next).is_none_or(|token| token.text(line) != "@") {
                break;
            }
        }
        if attributes.len() > 1 {
            attributes.sort_by(|left, right| left.0.cmp(&right.0));
            edits.push((
                run_start,
                run_end,
                attributes
                    .into_iter()
                    .map(|(_, text)| text)
                    .collect::<Vec<_>>()
                    .join(" "),
            ));
        }
        cursor = next.max(cursor + 1);
    }
    let mut output = line.to_string();
    for (start, end, replacement) in edits.into_iter().rev() {
        output.replace_range(start..end, &replacement);
    }
    output
}

fn attribute_end<'a>(
    source: &'a str,
    tokens: &[super::Token],
    start: usize,
) -> Option<(usize, &'a str)> {
    if tokens.get(start)?.text(source) != "@" {
        return None;
    }
    let name = tokens.get(start + 1)?;
    if name.kind != TokenKind::Word {
        return None;
    }
    let mut cursor = start + 2;
    if tokens
        .get(cursor)
        .is_none_or(|token| token.text(source) != "(")
    {
        return Some((cursor, name.text(source)));
    }
    let mut depth = 0_u32;
    while let Some(token) = tokens.get(cursor) {
        match token.text(source) {
            "(" => depth += 1,
            ")" => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some((cursor + 1, name.text(source)));
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

fn attribute_rank(name: &str) -> (u8, &str) {
    let group = match name {
        "id" => 0,
        "default" | "pk" => 1,
        "length" | "nonnegative" | "notBlank" | "positive" => 2,
        "index" | "internal" | "scope" | "target" | "unique" | "version" => 3,
        "map" => 4,
        "retired" => 5,
        _ => 6,
    };
    (group, name)
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

    #[test]
    fn formatter_canonicalizes_strings_routes_ordering_and_attribute_rank() {
        let input = r#"jdl 1
app Demo {
 pkg com.example.demo
 java 26
 platform spring
 build maven
 storage postgres
}
dep org.example:demo @scope(test) @version("1.0") @id(dep_demo)
entity Task {
 title: string @map("ti\u0074le") @unique @length(1..100) @id(fld_task_title)
 createdAt: instant
 query Open() @id(op_open) {
 order by [title asc, createdAt desc]
 route get "\u002ftasks"
 }
}
"#;
        let formatted = format(input).unwrap();
        assert!(
            formatted.contains(
                "title: string @id(fld_task_title) @length(1..100) @unique @map(\"title\")"
            ),
            "{formatted}"
        );
        assert!(
            formatted.contains("dep org.example:demo @id(dep_demo) @scope(test) @version(\"1.0\")"),
            "{formatted}"
        );
        assert!(
            formatted.contains("order by [title, createdAt desc]"),
            "{formatted}"
        );
        assert!(formatted.contains("route GET \"/tasks\""), "{formatted}");
        assert_eq!(format(&formatted).unwrap(), formatted);
        assert_eq!(
            crate::parse_jdl(input).unwrap(),
            crate::parse_jdl(&formatted).unwrap()
        );
    }
}
