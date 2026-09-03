//! `jails model fmt` — whole-document layout, and the only pass allowed to do
//! it.
//!
//! Ordinary CST edits are surgical by contract: they touch one span and leave
//! every other byte alone. The formatter is the deliberate exception, so it is
//! the one place that may canonicalize lexical spellings, line endings,
//! indentation, blank-line runs, trailing space and the final newline.
//!
//! **Comment text is retained and declaration ordering is not changed.** Both
//! are the reader's, and a formatter that reorders declarations makes every
//! subsequent diff unreadable — which is how a formatter stops being run.
//! Token wrapping and member reordering are held out for the same reason:
//! each would be a judgement about the reader's document rather than about its
//! whitespace.

use super::cst::top_level_order;
use super::{TokenKind, parse_cst};
use crate::Diagnostics;
use std::collections::BTreeSet;

/// Format a valid JDL v1 document without changing declaration ordering.
pub fn format(input: &str) -> Result<String, Diagnostics> {
    let reordered = reorder_entity_members(input)?;
    let grouped = separate_top_level_groups(&reordered)?;
    let normalized = canonicalize_tokens(&grouped)?
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
        let rendered = format!(
            "{}{}",
            "  ".repeat((line_brace_depth + continuation) as usize),
            trimmed
        );
        lines.extend(wrap_line(rendered, line_delimiter_depth));
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
    align_member_columns(&mut lines);
    let mut output = lines.join("\n");
    output.push('\n');
    Ok(output)
}

/// Line up the type column of a run of `name: type` members.
///
/// **The one place a column is decided.** A scaffold renders its fields, an
/// appended field is spliced in on its own, and a reader writes theirs by
/// hand; three writers each choosing a column is three answers, and the
/// visible cost was a `resource field add` line sitting one space off every
/// line above it. Alignment is layout, so it belongs to the formatter -- and
/// because every model mutation is formatted before it becomes the plan's
/// after-image, the appended line comes out aligned without the splice
/// knowing anything about its neighbours.
///
/// A run is consecutive lines at one indent that all carry a `name:` head; a
/// blank line, a different indent or a line with no head ends it. A comment
/// rides along without ending a run, because a comment between two fields is
/// about the field below it.
fn align_member_columns(lines: &mut [String]) {
    let mut start = 0;
    while start < lines.len() {
        let Some((indent, _)) = member_head(&lines[start]) else {
            start += 1;
            continue;
        };
        let mut end = start;
        let mut width = 0;
        while end < lines.len() {
            match member_head(&lines[end]) {
                Some((line_indent, head)) if line_indent == indent => {
                    width = width.max(head);
                    end += 1;
                }
                _ if end > start && is_comment_at(&lines[end], indent) => end += 1,
                _ => break,
            }
        }
        for line in lines.iter_mut().take(end).skip(start) {
            let Some((line_indent, head)) = member_head(line) else {
                continue;
            };
            let rest = line[line_indent + head..].trim_start().to_string();
            if rest.is_empty() {
                continue;
            }
            let padding = " ".repeat(width - head + 1);
            let head_text = line[..line_indent + head].to_string();
            *line = format!("{head_text}{padding}{rest}");
        }
        start = end.max(start + 1);
    }
}

/// The indent and the width of a `name:` head, when the line has one.
///
/// Only a member has one: `pkg com.example.demo` and `use scaffold` carry no
/// colon, an operation's `route POST "/tasks"` carries one only inside a
/// string, and a top-level declaration is at indent zero.
fn member_head(line: &str) -> Option<(usize, usize)> {
    let indent = line.len() - line.trim_start().len();
    if indent == 0 {
        return None;
    }
    let rest = &line[indent..];
    let colon = rest.find(':')?;
    let name = &rest[..colon];
    if name.is_empty()
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return None;
    }
    Some((indent, colon + 1))
}

/// A comment sitting inside a run of members, at the run's own indent.
fn is_comment_at(line: &str, indent: usize) -> bool {
    line.len() - line.trim_start().len() == indent && line.trim_start().starts_with("//")
}

fn separate_top_level_groups(input: &str) -> Result<String, Diagnostics> {
    let cst = parse_cst(input)?;
    let entities = cst
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == "entity")
        .collect::<Vec<_>>();
    let mut declarations = cst
        .declarations
        .iter()
        .filter(|declaration| {
            declaration.kind == "entity"
                || !entities.iter().any(|entity| {
                    declaration.span.start > entity.span.start
                        && declaration.span.end <= entity.span.end
                })
        })
        .collect::<Vec<_>>();
    declarations.sort_by_key(|declaration| declaration.span.start);
    let mut insertions = Vec::new();
    for pair in declarations.windows(2) {
        if top_level_order(&pair[0].kind).0 != top_level_order(&pair[1].kind).0 {
            insertions.push(pair[0].span.end);
        }
    }
    let mut output = input.to_string();
    for position in insertions.into_iter().rev() {
        output.insert(position, '\n');
    }
    Ok(output)
}

fn reorder_entity_members(input: &str) -> Result<String, Diagnostics> {
    let cst = parse_cst(input)?;
    let mut edits = Vec::new();
    for entity in cst
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == "entity")
    {
        let mut members = cst
            .members
            .iter()
            .filter(|member| {
                member.span.start >= entity.span.start && member.span.end <= entity.span.end
            })
            .collect::<Vec<_>>();
        members.sort_by_key(|member| member.span.start);
        if members.len() < 2 {
            continue;
        }
        let body_start = cst
            .tokens
            .iter()
            .find(|token| {
                token.span.start >= entity.span.start
                    && token.span.end <= entity.span.end
                    && token.text(cst.source()) == "{"
            })
            .map(|token| token.span.end);
        let body_end = cst
            .tokens
            .iter()
            .filter(|token| {
                token.span.start >= entity.span.start
                    && token.span.end <= entity.span.end
                    && token.text(cst.source()) == "}"
            })
            .map(|token| token.span.start)
            .max();
        let (Some(body_start), Some(body_end)) = (body_start, body_end) else {
            continue;
        };
        let prefix_end = members[0].span.start;
        let prefix = &cst.source()[body_start..prefix_end];
        let mut previous_end = prefix_end;
        let mut chunks = Vec::new();
        for (position, member) in members.iter().enumerate() {
            let chunk = &cst.source()[previous_end..member.span.end];
            chunks.push((
                member_rank(&member.kind),
                position,
                member.kind.as_str(),
                member_syntax(cst.member_text(member)),
                chunk.to_string(),
            ));
            previous_end = member.span.end;
        }
        chunks.sort_by_key(|(rank, position, ..)| (*rank, *position));
        let mut seen_uses = BTreeSet::new();
        let mut replacement = prefix.to_string();
        let mut previous_rank = None;
        for (rank, _, kind, syntax, chunk) in chunks {
            if kind == "use" && !chunk.contains("//") && !seen_uses.insert(syntax) {
                continue;
            }
            if previous_rank.is_some_and(|previous| previous != rank) {
                replacement.push('\n');
            }
            replacement.push_str(&chunk);
            previous_rank = Some(rank);
        }
        replacement.push_str(&cst.source()[previous_end..body_end]);
        if replacement != cst.source()[body_start..body_end] {
            edits.push((body_start, body_end, replacement));
        }
    }
    let mut output = input.to_string();
    edits.sort_by_key(|(start, ..)| *start);
    for (start, end, replacement) in edits.into_iter().rev() {
        output.replace_range(start..end, &replacement);
    }
    Ok(output)
}

fn member_rank(kind: &str) -> u8 {
    match kind {
        "use" => 0,
        "table" => 1,
        "field" => 2,
        "pk" | "unique" | "index" => 3,
        "relation" => 4,
        "command" | "query" | "transition" | "event" => 5,
        _ => 6,
    }
}

fn member_syntax(source: &str) -> String {
    super::token::lex(source).map_or_else(
        |_| source.split_whitespace().collect(),
        |tokens| {
            tokens
                .iter()
                .filter(|token| {
                    matches!(
                        token.kind,
                        TokenKind::Word
                            | TokenKind::Integer
                            | TokenKind::String
                            | TokenKind::Symbol
                    )
                })
                .map(|token| token.text(source))
                .collect()
        },
    )
}

fn wrap_line(line: String, initial_delimiter_depth: u32) -> Vec<String> {
    if line.chars().count() <= 100 {
        return vec![line];
    }
    let leading = line.bytes().take_while(|byte| *byte == b' ').count();
    let continuation_indent = if initial_delimiter_depth > 0 {
        leading
    } else {
        leading + 2
    };
    let mut depth = initial_delimiter_depth;
    let mut current = line;
    let mut wrapped = Vec::new();
    loop {
        let candidates = comma_candidates(&current, depth);
        let position = candidates
            .iter()
            .rev()
            .find(|(position, _)| current[..*position].chars().count() <= 100)
            .copied()
            .or_else(|| candidates.first().copied());
        let Some((position, next_depth)) = position else {
            wrapped.push(current);
            break;
        };
        let remainder = current[position..].trim_start();
        if remainder.is_empty() {
            wrapped.push(current);
            break;
        }
        wrapped.push(current[..position].trim_end().to_string());
        current = format!("{}{remainder}", " ".repeat(continuation_indent));
        depth = next_depth;
        if current.chars().count() <= 100 {
            wrapped.push(current);
            break;
        }
    }
    wrapped
}

fn comma_candidates(line: &str, initial_depth: u32) -> Vec<(usize, u32)> {
    let mut candidates = Vec::new();
    let mut depth = initial_depth;
    let mut string = false;
    let mut escaped = false;
    let mut characters = line.char_indices().peekable();
    while let Some((offset, character)) = characters.next() {
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
        if character == '/' && characters.peek().is_some_and(|(_, next)| *next == '/') {
            break;
        }
        match character {
            '"' => string = true,
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth > 0 => candidates.push((offset + character.len_utf8(), depth)),
            _ => {}
        }
    }
    candidates
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
        "default" | "pk" | "version" => 1,
        "length" | "nonnegative" | "notBlank" | "positive" => 2,
        "index" | "internal" | "scope" | "target" | "unique" | "updated" => 3,
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
cap fake
cap security
dep org.example:demo @scope(test) @version("1.0") @id(dep_demo)
entity Task {
 title: string @map("ti\u0074le") @unique @length(1..100) @id(fld_task_title)
 tenantId: uuid @map("tenant_id") @scope(claim: "tenant") @id(fld_task_tenant)
 version: long @nonnegative @version
 updatedAt: instant @updated @default(now())
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
                "title:     string @id(fld_task_title) @length(1..100) @unique @map(\"title\")"
            ),
            "{formatted}"
        );
        assert!(formatted.contains(
            "tenantId:  uuid @id(fld_task_tenant) @scope(claim: \"tenant\") @map(\"tenant_id\")"
        ));
        assert!(formatted.contains("version:   long @version @nonnegative"));
        assert!(formatted.contains("updatedAt: instant @default(now()) @updated"));
        assert!(
            formatted.contains("dep org.example:demo @id(dep_demo) @version(\"1.0\") @scope(test)"),
            "{formatted}"
        );
        assert!(formatted.contains("}\n\ncap fake\ncap security\ndep org.example:demo"));
        assert!(formatted.contains("@scope(test)\n\nentity Task"));
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

    #[test]
    fn formatter_wraps_only_at_delimited_commas_before_the_target_width() {
        let input = r#"jdl 1
app Demo {
 pkg com.example.demo
 java 26
 platform spring
 build maven
 storage postgres
}
entity Composite {
 firstIdentifier: uuid
 secondIdentifier: uuid
 thirdIdentifier: uuid
 fourthIdentifier: uuid
 fifthIdentifier: uuid
 sixthIdentifier: uuid
 pk [firstIdentifier, secondIdentifier, thirdIdentifier, fourthIdentifier, fifthIdentifier, sixthIdentifier] @id(pk_composite)
}
"#;
        let formatted = format(input).unwrap();
        assert!(formatted.contains("pk [firstIdentifier, secondIdentifier,"));
        assert!(formatted.contains("\n    sixthIdentifier"), "{formatted}");
        assert!(
            formatted.lines().all(|line| line.chars().count() <= 100),
            "{formatted}"
        );
        assert_eq!(format(&formatted).unwrap(), formatted);
        assert_eq!(
            crate::parse_jdl(input).unwrap(),
            crate::parse_jdl(&formatted).unwrap()
        );
    }

    #[test]
    fn formatter_orders_entity_member_classes_without_detaching_comments() {
        let input = r#"jdl 1
app Demo {
 pkg com.example.demo
 java 26
 platform spring
 build maven
 storage postgres
}
entity Task {
 query Open()
 // title contract
 title: string
 use repo
 table "tasks"
 id: uuid @pk
 index [title]
}
"#;
        let formatted = format(input).unwrap();
        let use_at = formatted.find("  use repo").unwrap();
        let table_at = formatted.find("  table \"tasks\"").unwrap();
        let id_at = formatted.find("  id:").unwrap();
        let comment_at = formatted.find("  // title contract").unwrap();
        let title_at = formatted.find("  title:").unwrap();
        let index_at = formatted.find("  index [title]").unwrap();
        let query_at = formatted.find("  query Open()").unwrap();
        assert!(use_at < table_at);
        assert!(table_at < id_at);
        assert!(comment_at < title_at && title_at < id_at);
        assert!(id_at < index_at && index_at < query_at);
        assert!(formatted.contains("  use repo\n\n  table \"tasks\""));
        assert!(formatted.contains("@pk\n\n  index [title]"), "{formatted}");
        assert!(formatted.contains("  index [title]\n\n  query Open()"));
        assert_eq!(format(&formatted).unwrap(), formatted);
        assert_eq!(
            crate::parse_jdl(input).unwrap(),
            crate::parse_jdl(&formatted).unwrap()
        );
    }

    #[test]
    fn formatter_removes_only_comment_free_identical_use_members() {
        let input = r#"jdl 1
app Demo {
 pkg com.example.demo
 java 26
 platform spring
 build maven
 storage postgres
}
entity Task {
 use repo
 use repo
 // keep this selection explanation
 use repo
 id: uuid @pk
}
"#;
        let formatted = format(input).unwrap();
        assert_eq!(formatted.matches("use repo").count(), 2, "{formatted}");
        assert!(formatted.contains("// keep this selection explanation"));
        assert_eq!(format(&formatted).unwrap(), formatted);
    }
}
