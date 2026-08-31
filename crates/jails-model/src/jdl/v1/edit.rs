//! CST-owned, byte-local edits for JDL v1 authoring source.

use super::{DocumentCst, MemberCst, parse_cst};
use crate::{Diagnostic, Diagnostics};

/// Append one top-level declaration without re-rendering existing source.
pub fn append_declaration(source: &str, declaration: &str) -> Result<String, Diagnostics> {
    parse_cst(source)?;
    let newline = newline_style(source);
    let declaration = normalize_newlines(declaration.trim_end_matches(['\r', '\n']), newline);
    let mut edited = source.to_string();
    if !edited.is_empty() && !edited.ends_with('\n') && !edited.ends_with('\r') {
        edited.push_str(newline);
    }
    let separator = format!("{newline}{newline}");
    if !edited.is_empty() && !edited.ends_with(&separator) {
        edited.push_str(newline);
    }
    edited.push_str(&declaration);
    edited.push_str(newline);
    Ok(edited)
}

/// Set one `app { }` property, replacing its line or adding it.
///
/// **This is how `add db` reaches a JDL v1 project.** v1 has no `cap db`: the
/// closed capability registry deliberately excludes the storage kinds, because
/// `storage postgres` is the axis and the `db` capability is what the linker
/// materializes *from* it. Appending `cap db` therefore wrote a model that no
/// longer parsed -- which is what `jails add db` did on every v1 project, and
/// it failed closed, so the command simply did not work.
///
/// The edit is scoped to the `app` declaration's own span and rewrites one
/// line, so every other byte -- comments, ordering, unrelated properties --
/// survives, which is the rule every edit in this module follows.
pub fn set_app_property(source: &str, key: &str, value: &str) -> Result<String, Diagnostics> {
    let cst = parse_cst(source)?;
    let app = cst
        .declarations
        .iter()
        .find(|declaration| declaration.kind == "app")
        .ok_or_else(|| {
            edit_problem(
                "this JDL v1 source has no `app` declaration",
                "add an `app` block before setting one of its properties",
            )
        })?;
    let text = cst.source();
    let block = &text[app.span.start..app.span.end];
    let indent = block
        .lines()
        .nth(1)
        .map(|line| &line[..line.len() - line.trim_start().len()])
        .filter(|indent| !indent.is_empty())
        .unwrap_or("  ")
        .to_string();
    let mut offset = app.span.start;
    for line in block.split_inclusive('\n') {
        if line.trim_start().starts_with(&format!("{key} ")) {
            let start = offset + (line.len() - line.trim_start().len());
            let end = offset + line.trim_end_matches(['\r', '\n']).len();
            return cst.replace_span(super::Span::new(start, end), &format!("{key} {value}"));
        }
        offset += line.len();
    }
    let brace = closing_brace(&cst, app.span).ok_or_else(|| {
        edit_problem(
            "the `app` block has no unambiguous closing brace",
            "repair the app block, then retry the command",
        )
    })?;
    let newline = newline_style(source);
    cst.replace_span(
        super::Span::new(brace, brace),
        &format!("{indent}{key} {value}{newline}"),
    )
}

/// Remove one complete top-level declaration selected by parsed kind and name.
pub fn remove_declaration(source: &str, kinds: &[&str], name: &str) -> Result<String, Diagnostics> {
    let cst = parse_cst(source)?;
    let matches = cst
        .declarations
        .iter()
        .filter(|candidate| kinds.contains(&candidate.kind.as_str()))
        .filter(|candidate| candidate.name.as_deref() == Some(name))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [declaration] => {
            let start = cst.source()[..declaration.span.start]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
            cst.replace_span(super::Span::new(start, declaration.span.end), "")
        }
        [] => Err(edit_problem(
            format!(
                "could not find top-level {} declaration `{name}`",
                kinds.join("/")
            ),
            "name a declaration in the parsed JDL v1 source",
        )),
        _ => Err(edit_problem(
            format!(
                "top-level {} declaration `{name}` is ambiguous",
                kinds.join("/")
            ),
            "give each declaration a unique name before retrying",
        )),
    }
}

/// Rename one declaration and materialize its previously effective ID when it
/// was convention-derived.
pub fn rename_declaration(
    source: &str,
    kinds: &[&str],
    current_name: &str,
    next_name: &str,
    effective_id: &str,
) -> Result<String, Diagnostics> {
    let cst = parse_cst(source)?;
    let matches = cst
        .declarations
        .iter()
        .filter(|candidate| kinds.contains(&candidate.kind.as_str()))
        .filter(|candidate| candidate.name.as_deref() == Some(current_name))
        .collect::<Vec<_>>();
    let declaration = match matches.as_slice() {
        [declaration] => *declaration,
        [] => {
            return Err(edit_problem(
                format!(
                    "could not find {} declaration `{current_name}`",
                    kinds.join("/")
                ),
                "name a declaration in the parsed JDL v1 source",
            ));
        }
        _ => {
            return Err(edit_problem(
                format!(
                    "{} declaration `{current_name}` is ambiguous",
                    kinds.join("/")
                ),
                "give each declaration a unique name before retrying",
            ));
        }
    };
    let header = cst
        .tokens
        .iter()
        .filter(|token| {
            token.span.start >= declaration.span.start && token.span.end <= declaration.span.end
        })
        .take_while(|token| token.text(cst.source()) != "{")
        .collect::<Vec<_>>();
    let name = header
        .iter()
        .find(|token| token.text(cst.source()) == current_name)
        .ok_or_else(|| {
            edit_problem(
                format!("declaration `{current_name}` has no editable name token"),
                "repair the declaration header, then retry",
            )
        })?;
    let has_id = header
        .windows(2)
        .any(|pair| pair[0].text(cst.source()) == "@" && pair[1].text(cst.source()) == "id");
    let mut edited = source.to_string();
    if !has_id {
        let brace = cst
            .tokens
            .iter()
            .find(|token| {
                token.span.start >= declaration.span.start
                    && token.span.end <= declaration.span.end
                    && token.text(cst.source()) == "{"
            })
            .ok_or_else(|| {
                edit_problem(
                    format!("declaration `{current_name}` has no opening brace"),
                    "repair the declaration header, then retry",
                )
            })?;
        edited.insert_str(brace.span.start, &format!("@id({effective_id}) "));
    }
    edited.replace_range(name.span.start..name.span.end, next_name);
    Ok(edited)
}

/// Add or remove one flag attribute on an entity header.
pub fn set_entity_attribute(
    source: &str,
    entity: &str,
    attribute: &str,
    enabled: bool,
) -> Result<String, Diagnostics> {
    let cst = parse_cst(source)?;
    let declaration = unique_declaration(&cst, "entity", entity)?;
    let header_tokens = cst
        .tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| {
            token.span.start >= declaration.span.start && token.span.end <= declaration.span.end
        })
        .take_while(|(_, token)| token.text(cst.source()) != "{")
        .collect::<Vec<_>>();
    let attribute_token = header_tokens.windows(2).find_map(|pair| {
        (pair[0].1.text(cst.source()) == "@" && pair[1].1.text(cst.source()) == attribute)
            .then_some((pair[0].0, pair[0].1.span.start, pair[1].1.span.end))
    });
    match (enabled, attribute_token) {
        (true, Some(_)) | (false, None) => Ok(source.to_string()),
        (true, None) => {
            let brace = cst
                .tokens
                .iter()
                .find(|token| {
                    token.span.start >= declaration.span.start
                        && token.span.end <= declaration.span.end
                        && token.text(cst.source()) == "{"
                })
                .ok_or_else(|| {
                    edit_problem(
                        format!("entity `{entity}` has no opening brace"),
                        "repair the entity header, then retry",
                    )
                })?;
            cst.replace_span(
                super::Span::new(brace.span.start, brace.span.start),
                &format!("@{attribute} "),
            )
        }
        (false, Some((token_index, start, end))) => {
            let start = cst
                .tokens
                .get(token_index.wrapping_sub(1))
                .filter(|token| {
                    token.kind == super::TokenKind::Whitespace
                        && token.span.end == start
                        && token.span.start >= declaration.span.start
                })
                .map_or(start, |token| token.span.start);
            cst.replace_span(super::Span::new(start, end), "")
        }
    }
}

/// Insert one direct entity child beside children of the same semantic class.
/// Pin the route a projection serves, so a rename does not move it.
///
/// **A derived name that moves is the point of `jails model explain`, and an
/// HTTP route is the one derived name with callers.** Renaming a resource
/// moves its Java type, its table and its route together, and the first two
/// are jails' business where the third is somebody else's -- a client that
/// asked for `/tasks` yesterday gets a 404 today, with nothing in the plan
/// saying so. Writing the accepted route into the model turns the convention
/// into a declaration, which is exactly what `derived` is for: the value stops
/// being recomputed and starts being stated.
///
/// Rewrites the projection's own `use` line rather than adding a second one:
/// two `use` members for one projection is a configuration conflict the linker
/// refuses, and correctly.
pub fn set_projection_path(
    source: &str,
    entity: &str,
    projection: &str,
    path: &str,
) -> Result<String, Diagnostics> {
    let cst = parse_cst(source)?;
    let owner = stable_fragment(entity);
    let quoted = format!("{path:?}");
    for member in &cst.members {
        if member.owner != owner || member.kind != "use" {
            continue;
        }
        let text = &source[member.span.start..member.span.end];
        let Some(rest) = text.trim_start().strip_prefix("use ") else {
            continue;
        };
        let named = rest
            .split(['(', ' ', '\n'])
            .next()
            .unwrap_or_default()
            .trim();
        if named != projection {
            continue;
        }
        // Already carries arguments: the author stated something here, and a
        // rename is not the moment to rewrite what they wrote.
        if rest.contains('(') {
            return Ok(source.to_string());
        }
        let indent = &text[..text.len() - text.trim_start().len()];
        let replacement = format!("{indent}use {projection}(path: {quoted})");
        let mut next = source.to_string();
        next.replace_range(member.span.start..member.span.end, &replacement);
        return Ok(next);
    }
    Ok(source.to_string())
}

pub fn insert_entity_member(
    source: &str,
    entity: &str,
    kind: &str,
    member: &str,
) -> Result<String, Diagnostics> {
    let cst = parse_cst(source)?;
    let owner = stable_fragment(entity);
    let entity_declaration = unique_declaration(&cst, "entity", entity)?;
    let siblings = cst
        .members
        .iter()
        .filter(|candidate| candidate.owner == owner)
        .collect::<Vec<_>>();
    let rank = member_rank(kind).ok_or_else(|| {
        edit_problem(
            format!("unknown JDL entity member class `{kind}`"),
            "use use, table, field, pk, unique, index, relation, command, query, transition, or event",
        )
    })?;
    let insertion = siblings
        .iter()
        .filter(|candidate| member_rank(&candidate.kind) == Some(rank))
        .map(|candidate| candidate.span.end)
        .max()
        .or_else(|| {
            siblings
                .iter()
                .filter(|candidate| member_rank(&candidate.kind).is_some_and(|value| value > rank))
                .map(|candidate| candidate.span.start)
                .min()
        })
        .or_else(|| closing_brace(&cst, entity_declaration.span))
        .ok_or_else(|| {
            edit_problem(
                format!("entity `{entity}` has no unambiguous closing brace"),
                "repair the entity block, then retry the command",
            )
        })?;
    let newline = newline_style(source);
    let mut rendered = normalize_newlines(member.trim_end_matches(['\r', '\n']), newline);
    rendered.push_str(newline);
    cst.replace_span(super::Span::new(insertion, insertion), &rendered)
}

/// Remove one direct entity member selected by syntax kind/name and optional ID.
pub fn remove_entity_member(
    source: &str,
    entity: &str,
    kinds: &[&str],
    name: Option<&str>,
    stable_id: Option<&str>,
) -> Result<String, Diagnostics> {
    let cst = parse_cst(source)?;
    let owner = stable_fragment(entity);
    let member = unique_member(&cst, &owner, kinds, name, stable_id)?;
    cst.replace_span(member.span, "")
}

/// Replace one direct entity member while retaining all unrelated bytes.
pub fn replace_entity_member(
    source: &str,
    entity: &str,
    kinds: &[&str],
    name: Option<&str>,
    stable_id: Option<&str>,
    replacement: &str,
) -> Result<String, Diagnostics> {
    let cst = parse_cst(source)?;
    let owner = stable_fragment(entity);
    let member = unique_member(&cst, &owner, kinds, name, stable_id)?;
    let newline = newline_style(source);
    let mut rendered = normalize_newlines(replacement.trim_end_matches(['\r', '\n']), newline);
    rendered.push_str(newline);
    cst.replace_span(member.span, &rendered)
}

fn unique_declaration<'a>(
    cst: &'a DocumentCst,
    kind: &str,
    name: &str,
) -> Result<&'a super::DeclarationCst, Diagnostics> {
    let matches = cst
        .declarations
        .iter()
        .filter(|candidate| candidate.kind == kind && candidate.name.as_deref() == Some(name))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [declaration] => Ok(*declaration),
        [] => Err(edit_problem(
            format!("could not find JDL {kind} `{name}`"),
            "name a declaration in the parsed JDL v1 source",
        )),
        _ => Err(edit_problem(
            format!("JDL {kind} `{name}` is ambiguous"),
            "give each declaration a unique name before retrying",
        )),
    }
}

fn unique_member<'a>(
    cst: &'a DocumentCst,
    owner: &str,
    kinds: &[&str],
    name: Option<&str>,
    stable_id: Option<&str>,
) -> Result<&'a MemberCst, Diagnostics> {
    let explicit_id = stable_id.map(|id| format!("@id({id})"));
    let matches = cst
        .members
        .iter()
        .filter(|candidate| candidate.owner == owner)
        .filter(|candidate| kinds.contains(&candidate.kind.as_str()))
        .filter(|candidate| name.is_none_or(|name| candidate.name.as_deref() == Some(name)))
        .filter(|candidate| {
            explicit_id
                .as_ref()
                .is_none_or(|id| cst.member_text(candidate).contains(id))
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [member] => Ok(*member),
        [] => Err(edit_problem(
            format!(
                "could not find the requested {} member in entity `{owner}`",
                kinds.join("/")
            ),
            "keep the declaration in its entity block with its stable @id when one is explicit",
        )),
        _ => Err(edit_problem(
            format!(
                "the requested {} member in entity `{owner}` is ambiguous",
                kinds.join("/")
            ),
            "give the member an explicit stable @id, then retry",
        )),
    }
}

fn closing_brace(cst: &DocumentCst, span: super::Span) -> Option<usize> {
    cst.tokens
        .iter()
        .filter(|token| token.span.start >= span.start && token.span.end <= span.end)
        .filter(|token| token.text(cst.source()) == "}")
        .map(|token| token.span.start)
        .max()
}

fn member_rank(kind: &str) -> Option<u8> {
    match kind {
        "use" => Some(0),
        "table" => Some(1),
        "field" => Some(2),
        "pk" | "unique" | "index" => Some(3),
        "relation" => Some(4),
        "command" | "query" | "transition" | "event" => Some(5),
        _ => None,
    }
}

fn newline_style(source: &str) -> &'static str {
    source.find('\n').map_or("\n", |newline| {
        if newline > 0 && source.as_bytes().get(newline - 1) == Some(&b'\r') {
            "\r\n"
        } else {
            "\n"
        }
    })
}

fn normalize_newlines(value: &str, newline: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', newline)
}

fn stable_fragment(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else if character == '-' {
            output.push('_');
        } else {
            output.push(character);
        }
    }
    output
}

fn edit_problem(message: impl Into<String>, fix: impl Into<String>) -> Diagnostics {
    Diagnostics::from_vec(vec![Diagnostic::new("JDL1002", "$", message, fix)])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "jdl 1\r\n\r\napp Demo {\r\n  pkg com.example.demo\r\n  java 26\r\n  platform spring\r\n  build maven\r\n  storage postgres\r\n}\r\n\r\nentity Task {\r\n  use repo\r\n\r\n  id: uuid @pk\r\n  title: string\r\n\r\n  command Create(title) {}\r\n}\r\n";

    #[test]
    fn insertion_uses_member_classes_and_preserves_crlf_and_unrelated_bytes() {
        let edited = insert_entity_member(
            SOURCE,
            "Task",
            "field",
            "  priority: int @id(fld_task_priority)",
        )
        .unwrap();
        assert!(edited.contains("  title: string\r\n  priority: int"));
        assert!(edited.contains("\r\n\r\n  command Create"));
        assert!(!edited.replace("\r\n", "").contains('\n'));
        assert_eq!(
            edited.replace("  priority: int @id(fld_task_priority)\r\n", ""),
            SOURCE
        );
    }

    #[test]
    fn removal_uses_the_parsed_member_span_only() {
        let edited = remove_entity_member(SOURCE, "Task", &["field"], Some("title"), None).unwrap();
        assert!(!edited.contains("title: string"));
        assert!(edited.contains("id: uuid @pk"));
        assert!(edited.contains("command Create(title)"));
    }

    #[test]
    fn entity_flag_edits_are_header_local_and_idempotent() {
        let retired = set_entity_attribute(SOURCE, "Task", "retired", true).unwrap();
        assert!(retired.contains("entity Task @retired {"));
        assert_eq!(
            set_entity_attribute(&retired, "Task", "retired", true).unwrap(),
            retired
        );
        let active = set_entity_attribute(&retired, "Task", "retired", false).unwrap();
        assert_eq!(active, SOURCE);
    }
}
