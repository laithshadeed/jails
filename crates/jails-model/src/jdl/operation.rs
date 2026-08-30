//! Parsing the four operation declarations out of pre-v1 JDL.
//!
//! `OperationDraft` is a flat, untyped landing place — labels and strings —
//! deliberately doing no resolution: the linker owns turning names into IDs,
//! and a parser that resolved as it read could only report the first bad
//! reference.
//!
//! This is the *pre-v1* dialect, kept for `jails model upgrade --to 1` and the
//! projects written before v1 existed. New syntax belongs in `jdl/v1/`; adding
//! it here as well would leave two parsers to keep in step, which is the
//! failure this repository has paid for more than once.

use crate::Diagnostics;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Kind {
    Command,
    Query,
    Transition,
    Event,
}

pub(super) struct OperationDraft {
    kind: Kind,
    label: String,
    id: String,
    on: String,
    fields: Vec<String>,
    sets: Option<Vec<String>>,
    order_by: Vec<String>,
    limit: Option<u32>,
    yields: Option<String>,
    route: Option<String>,
}

pub(super) fn is_header(line: &str) -> bool {
    ["command ", "query ", "transition ", "event "]
        .iter()
        .any(|prefix| line.starts_with(prefix))
}

pub(super) fn header(
    line_number: usize,
    line: &str,
    entity: &str,
) -> Result<OperationDraft, Diagnostics> {
    if !line.ends_with('{') {
        return Err(problem(
            line_number,
            "an operation header must end with `{`",
            "write `command CreateTask(title) {`",
        ));
    }
    let declaration = line.trim_end_matches('{').trim();
    let (kind_name, rest) = declaration.split_once(char::is_whitespace).ok_or_else(|| {
        problem(
            line_number,
            "the operation has no name",
            "write `command CreateTask() {`",
        )
    })?;
    let kind = match kind_name {
        "command" => Kind::Command,
        "query" => Kind::Query,
        "transition" => Kind::Transition,
        "event" => Kind::Event,
        _ => unreachable!("is_header recognized the operation kind"),
    };
    let open = rest.find('(').ok_or_else(|| {
        problem(
            line_number,
            "the operation has no parameter list",
            "write `Name()` after the operation kind",
        )
    })?;
    let close = rest[open + 1..]
        .find(')')
        .map(|at| open + 1 + at)
        .ok_or_else(|| {
            problem(
                line_number,
                "the operation parameter list is not closed",
                "add `)` before annotations",
            )
        })?;
    let name = rest[..open].trim();
    if name.is_empty() {
        return Err(problem(
            line_number,
            "the operation has no name",
            "write a name before `()`",
        ));
    }
    let operation_label = label(name);
    let fields = list(&rest[open + 1..close], line_number, "parameter")?;
    let annotations = &rest[close + 1..];
    Ok(OperationDraft {
        kind,
        id: annotation(annotations, "id")
            .map(str::to_string)
            .unwrap_or_else(|| format!("op_{operation_label}")),
        label: annotation(annotations, "as")
            .map(str::to_string)
            .unwrap_or(operation_label),
        on: entity.to_string(),
        fields,
        sets: None,
        order_by: Vec::new(),
        limit: None,
        yields: None,
        route: None,
    })
}

impl OperationDraft {
    pub(super) fn property(&mut self, line_number: usize, line: &str) -> Result<(), Diagnostics> {
        let line = line.trim_end_matches([',', ';']).trim();
        let (key, value) = line.split_once(':').ok_or_else(|| {
            problem(
                line_number,
                format!("`{line}` is not an operation property"),
                "write `key: value` inside the operation",
            )
        })?;
        let key = key.trim();
        let value = value.trim();
        match (self.kind, key) {
            (Kind::Transition, "sets") => {
                self.sets = Some(list(value, line_number, "sets")?);
            }
            (Kind::Query, "orderBy" | "order_by") => {
                self.order_by = list(value, line_number, "ordering")?;
            }
            (Kind::Query, "limit") => {
                self.limit = Some(value.parse().map_err(|_| {
                    problem(
                        line_number,
                        format!("`{value}` is not a query limit"),
                        "use a positive integer",
                    )
                })?);
            }
            (Kind::Transition, "yields") => self.yields = Some(label(value)),
            (Kind::Command | Kind::Query | Kind::Transition, "route") => {
                self.route = Some(value.to_string());
            }
            _ => {
                return Err(problem(
                    line_number,
                    format!("`{key}` is not valid on this operation kind"),
                    "use only the typed properties accepted by this operation",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn render_toml(self, output: &mut String) -> Result<(), Diagnostics> {
        output.push_str(&format!(
            "\n[operations.{}]\nkind = {}\nid = {}\non = {}\n",
            quote(&self.label),
            quote(self.kind.name()),
            quote(&self.id),
            quote(&self.on)
        ));
        match self.kind {
            Kind::Command => output.push_str(&format!("fields = {}\n", array(&self.fields))),
            Kind::Query => {
                output.push_str(&format!("filters = {}\n", array(&self.fields)));
                if !self.order_by.is_empty() {
                    output.push_str(&format!("order_by = {}\n", array(&self.order_by)));
                }
                if let Some(limit) = self.limit {
                    output.push_str(&format!("limit = {limit}\n"));
                }
            }
            Kind::Transition => {
                output.push_str(&format!("fields = {}\n", array(&self.fields)));
                output.push_str(&format!(
                    "sets = {}\n",
                    array(self.sets.as_deref().unwrap_or(&self.fields))
                ));
                if let Some(yields) = self.yields {
                    output.push_str(&format!("yields = {}\n", quote(&yields)));
                }
            }
            Kind::Event => output.push_str(&format!("fields = {}\n", array(&self.fields))),
        }
        if let Some(route) = self.route {
            output.push_str(&format!("route = {}\n", quote(&route)));
        }
        Ok(())
    }
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Query => "query",
            Self::Transition => "transition",
            Self::Event => "event",
        }
    }
}

fn list(value: &str, line_number: usize, description: &str) -> Result<Vec<String>, Diagnostics> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.contains(':') || value.contains(char::is_whitespace) {
                return Err(problem(
                    line_number,
                    format!("`{value}` is not a {description} field reference"),
                    "name fields without redeclaring their types",
                ));
            }
            Ok(label(value))
        })
        .collect()
}

fn annotation<'a>(input: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("@{name}(");
    let start = input.find(&prefix)? + prefix.len();
    let rest = &input[start..];
    Some(rest[..rest.find(')')?].trim())
}

fn label(value: &str) -> String {
    let mut output = String::new();
    for (offset, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if offset > 0 {
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

fn array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| quote(value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn quote(value: &str) -> String {
    serde_json::to_string(value).expect("a Rust string always has a JSON representation")
}

fn problem(line: usize, message: impl Into<String>, fix: impl Into<String>) -> Diagnostics {
    Diagnostics::jdl_syntax(line, message, fix)
}
