//! `command`, `query`, `transition` and `event` declarations.
//!
//! The four verbs share a parser because they share a grammar — a name, a
//! parameter list, attributes, an optional block — and differ in which block
//! members they accept: `emit` for a command, `limit` and `order by` for a
//! query, `update` for a transition. `Kind` carries that difference so the
//! shared path stays shared and the differences are one match each.
//!
//! Nothing is resolved: the entity, the fields and the emitted event stay
//! strings until the linker turns them into IDs. A parser that resolved as it
//! read could only report the first unknown name, and an operation typically
//! names several.

use super::{
    Parser, flag_attribute, length, one_arg, one_raw_arg, reject_unknown_attributes,
    stable_fragment,
};
use crate::source;
use crate::{Diagnostics, EndpointMethod, RequestFormat};

#[derive(Clone, Copy, Eq, PartialEq)]
enum Kind {
    Command,
    Query,
    Transition,
    Event,
}

impl Kind {
    fn keyword(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Query => "query",
            Self::Transition => "transition",
            Self::Event => "event",
        }
    }
}

impl Parser<'_> {
    pub(super) fn parse_operation(&mut self, owner: Option<&str>) -> Result<(), Diagnostics> {
        let start = self.span().start;
        let kind = match self.text() {
            "command" => Kind::Command,
            "query" => Kind::Query,
            "transition" => Kind::Transition,
            "event" => Kind::Event,
            _ => unreachable!("operation parser is called only for operation declarations"),
        };
        if owner.is_none() && kind != Kind::Event {
            return Err(self.here(
                "JDL0903",
                format!("top-level `{}` is not valid", kind.keyword()),
                "nest commands, queries, and transitions inside their target entity",
            ));
        }
        self.bump();
        let name = self.take_word("operation name")?;
        let label = stable_fragment(&name);
        self.expect("(", "JDL0904", "an operation needs a parameter list")?;
        let parameters = self.parse_parameters(owner.is_none())?;
        let attributes = self.attributes()?;
        let allowed = if kind == Kind::Event {
            &["id"][..]
        } else {
            &["id", "internal"][..]
        };
        reject_unknown_attributes(&attributes, allowed, self)?;
        let id = one_arg(&attributes, "id")?.unwrap_or_else(|| format!("op_{label}"));
        let internal = flag_attribute(&attributes, "internal")?;

        let mut command = source::CommandSemantics {
            parameters,
            internal,
            ..source::CommandSemantics::default()
        };
        let mut query = source::QuerySemantics {
            parameters: Vec::new(),
            internal,
            ..source::QuerySemantics::default()
        };
        let mut transition = source::TransitionSemantics {
            parameters: Vec::new(),
            internal,
            ..source::TransitionSemantics::default()
        };
        let mut event = source::EventSemantics::default();
        match kind {
            Kind::Command => {}
            Kind::Query => query.parameters = std::mem::take(&mut command.parameters),
            Kind::Transition => transition.parameters = std::mem::take(&mut command.parameters),
            Kind::Event => event.parameters = std::mem::take(&mut command.parameters),
        }

        if self.consume("{") {
            if self.consume("}") {
                self.end_line()?;
            } else {
                self.end_line()?;
                loop {
                    self.skip_layout();
                    if self.consume("}") {
                        self.end_line()?;
                        break;
                    }
                    self.parse_operation_member(
                        kind,
                        &mut command,
                        &mut query,
                        &mut transition,
                        &mut event,
                    )?;
                }
            }
        } else {
            self.end_line()?;
        }

        let operation = match kind {
            Kind::Command => {
                let fields = compatibility_fields(&command.parameters);
                let route = command.route.as_ref().map(compatibility_route);
                source::Operation::Command {
                    id,
                    java_name: Some(name.clone()),
                    on: owner.expect("a command has an owner").to_string(),
                    fields,
                    route,
                    semantics: command,
                }
            }
            Kind::Query => {
                let filters = compatibility_fields(&query.parameters);
                let order_by = query
                    .order
                    .iter()
                    .filter(|order| !order.field.contains('.'))
                    .map(|order| order.field.clone())
                    .collect();
                let route = query.route.as_ref().map(compatibility_route);
                source::Operation::Query {
                    id,
                    java_name: Some(name.clone()),
                    on: owner.expect("a query has an owner").to_string(),
                    filters,
                    order_by,
                    limit: query.limit,
                    route,
                    semantics: query,
                }
            }
            Kind::Transition => {
                let fields = compatibility_fields(&transition.parameters);
                // No compatibility projection: a synthesised `sets` of
                // *every* parameter subtracts neither the row selector nor
                // the version, and a single `yields` keeps only the first
                // `emit`. `.jails/model.toml` still spells the flat pair and
                // the linker folds it in; a JDL v1 source carries the rich
                // form, so it leaves the compatibility fields empty and the
                // one representation is the linked semantics.
                let sets = Vec::new();
                let yields = None;
                let route = transition.route.as_ref().map(compatibility_route);
                source::Operation::Transition {
                    id,
                    java_name: Some(name.clone()),
                    on: owner.expect("a transition has an owner").to_string(),
                    fields,
                    sets,
                    yields,
                    route,
                    semantics: transition,
                }
            }
            Kind::Event => {
                let fields = compatibility_fields(&event.parameters);
                source::Operation::Event {
                    id,
                    java_name: Some(name.clone()),
                    on: owner.map(str::to_string),
                    fields,
                    semantics: event,
                }
            }
        };
        if self.operations.insert(label.clone(), operation).is_some() {
            return Err(self.here(
                "JDL0905",
                format!("operation `{name}` is declared more than once"),
                "give every operation a unique name",
            ));
        }
        if let Some(owner) = owner {
            self.member(
                owner,
                kind.keyword(),
                Some(name.clone()),
                start,
                self.previous_end(),
            );
        }
        self.declaration(kind.keyword(), Some(name), start, self.previous_end());
        Ok(())
    }

    pub(super) fn parse_parameters(
        &mut self,
        typed_only: bool,
    ) -> Result<Vec<source::OperationParameter>, Diagnostics> {
        let mut parameters = Vec::new();
        if self.consume(")") {
            return Ok(parameters);
        }
        loop {
            let first = self.take_word("operation parameter")?;
            if self.consume(":") {
                let type_name = self.parse_type_ref()?;
                let required = !self.consume("?");
                let attributes = self.attributes()?;
                reject_unknown_attributes(
                    &attributes,
                    &["default", "notBlank", "length", "positive", "nonnegative"],
                    self,
                )?;
                let (min_length, max_length) = length(&attributes, self)?;
                parameters.push(source::OperationParameter {
                    name: first,
                    source: source::ParameterSource::Typed { type_name },
                    required,
                    optional_filter: false,
                    constraints: source::ParameterConstraints {
                        default: one_raw_arg(&attributes, "default")?
                            .map(|value| value_from_attribute(&value, self))
                            .transpose()?,
                        non_blank: flag_attribute(&attributes, "notBlank")?,
                        min_length,
                        max_length,
                        positive: flag_attribute(&attributes, "positive")?,
                        nonnegative: flag_attribute(&attributes, "nonnegative")?,
                    },
                });
            } else {
                if typed_only {
                    return Err(self.here(
                        "JDL0906",
                        "a top-level event parameter must declare its type",
                        "write `name: type` for every top-level event parameter",
                    ));
                }
                let (path, default_name) = self.finish_field_path(first)?;
                let optional_filter = self.consume("?");
                let name = if self.consume("as") {
                    self.take_word("parameter alias")?
                } else {
                    default_name
                };
                parameters.push(source::OperationParameter {
                    name,
                    source: source::ParameterSource::Field { path },
                    required: true,
                    optional_filter,
                    constraints: source::ParameterConstraints::default(),
                });
            }
            if self.consume(")") {
                break;
            }
            self.expect(",", "JDL0904", "separate operation parameters with `,`")?;
        }
        Ok(parameters)
    }

    fn parse_operation_member(
        &mut self,
        kind: Kind,
        command: &mut source::CommandSemantics,
        query: &mut source::QuerySemantics,
        transition: &mut source::TransitionSemantics,
        event: &mut source::EventSemantics,
    ) -> Result<(), Diagnostics> {
        match self.text() {
            "set" if matches!(kind, Kind::Command | Kind::Transition) => {
                self.bump();
                let field = self.parse_field_path()?;
                self.expect("=", "JDL0910", "a set statement needs `=`")?;
                let assignment = source::Assignment {
                    field,
                    value: self.take_literal()?,
                };
                if kind == Kind::Command {
                    command.assignments.push(assignment);
                } else {
                    transition.assignments.push(assignment);
                }
                self.end_line()
            }
            "emit" if matches!(kind, Kind::Command | Kind::Transition) => {
                self.bump();
                let event_name = stable_fragment(&self.take_word("event name")?);
                if kind == Kind::Command {
                    command.emits.push(event_name);
                } else {
                    transition.emits.push(event_name);
                }
                self.end_line()
            }
            // `deliver outbox` rather than an attribute on `emit`, because it
            // is one policy for the command, not one per event: two events
            // from one command travel the same way or the transaction means
            // nothing.
            "deliver" if kind == Kind::Command => {
                self.bump();
                let policy = self.take_word("delivery policy")?;
                if !matches!(policy.as_str(), "direct" | "outbox") {
                    return Err(self.here(
                        "JDL0730",
                        format!("unknown delivery policy `{policy}`"),
                        "use `deliver direct` or `deliver outbox`",
                    ));
                }
                if command.delivery.replace(policy).is_some() {
                    return Err(self.here(
                        "JDL0731",
                        "delivery is declared more than once",
                        "keep one `deliver` member",
                    ));
                }
                self.end_line()
            }
            "conflict" if kind == Kind::Command => {
                self.bump();
                self.expect("on", "JDL0911", "conflict must be followed by `on`")?;
                let fields = self.operation_field_list(false)?;
                replace_once(&mut command.conflict_key, fields, "conflict on", self)?;
                self.end_line()
            }
            "resolve" if kind == Kind::Command => self.parse_resolve(command),
            "join" if kind == Kind::Query => self.parse_join(query),
            "order" if kind == Kind::Query => {
                self.bump();
                self.expect("by", "JDL0912", "order must be followed by `by`")?;
                let order = self.operation_order_list()?;
                replace_once(&mut query.order, order, "order by", self)?;
                self.end_line()
            }
            "limit" if kind == Kind::Query => {
                self.bump();
                if query.limit.is_some() {
                    return Err(self.duplicate_member("limit"));
                }
                let value = self.take_integer()?.parse::<u32>().map_err(|_| {
                    self.here(
                        "JDL0913",
                        "query limit is out of range",
                        "use a positive u32",
                    )
                })?;
                if value == 0 {
                    return Err(self.here(
                        "JDL0913",
                        "query limit cannot be zero",
                        "use a positive integer",
                    ));
                }
                query.limit = Some(value);
                self.end_line()
            }
            "select" if kind == Kind::Transition => {
                self.bump();
                let fields = self.operation_field_list(false)?;
                replace_once(&mut transition.select, fields, "select", self)?;
                self.end_line()
            }
            "update" if kind == Kind::Transition => {
                self.bump();
                let fields = self.operation_field_list(false)?;
                replace_once(&mut transition.update, fields, "update", self)?;
                self.end_line()
            }
            "if-match" if kind == Kind::Transition => {
                self.bump();
                if transition.precondition.is_some() {
                    return Err(self.duplicate_member("if-match"));
                }
                transition.precondition = Some(match self.take_word("if-match policy")?.as_str() {
                    "required" => source::Precondition::Required,
                    "optional" => source::Precondition::Optional,
                    "none" => source::Precondition::None,
                    other => {
                        return Err(self.here(
                            "JDL0914",
                            format!("unknown if-match policy `{other}`"),
                            "use required, optional, or none",
                        ));
                    }
                });
                self.end_line()
            }
            "partition" if kind == Kind::Event => {
                self.bump();
                self.expect("by", "JDL0915", "partition must be followed by `by`")?;
                if event.partition_by.is_some() {
                    return Err(self.duplicate_member("partition by"));
                }
                event.partition_by = Some(self.take_word("event parameter")?);
                self.end_line()
            }
            "route" if kind != Kind::Event => {
                let route = self.parse_route()?;
                let target = match kind {
                    Kind::Command => &mut command.route,
                    Kind::Query => &mut query.route,
                    Kind::Transition => &mut transition.route,
                    Kind::Event => unreachable!(),
                };
                if target.replace(route).is_some() {
                    return Err(self.duplicate_member("route"));
                }
                Ok(())
            }
            "bind" if kind != Kind::Event => {
                let binding = self.parse_binding()?;
                match kind {
                    Kind::Command => command.bindings.push(binding),
                    Kind::Query => query.bindings.push(binding),
                    Kind::Transition => transition.bindings.push(binding),
                    Kind::Event => unreachable!(),
                }
                Ok(())
            }
            member => Err(self.here(
                "JDL0916",
                format!("`{member}` is not valid inside a {}", kind.keyword()),
                "use only the closed statement vocabulary for this operation kind",
            )),
        }
    }

    fn parse_resolve(&mut self, command: &mut source::CommandSemantics) -> Result<(), Diagnostics> {
        self.bump();
        let target = self.parse_field_path()?;
        self.expect("from", "JDL0917", "resolve needs `from`")?;
        let remote_value = self.parse_field_path()?;
        self.expect("where", "JDL0917", "resolve needs `where`")?;
        let remote_lookup = self.parse_field_path()?;
        self.expect("=", "JDL0917", "resolve lookup needs `=`")?;
        let parameter = self.take_word("operation parameter")?;
        command.resolutions.push(source::Resolution {
            target,
            remote_value,
            remote_lookup,
            parameter,
        });
        self.end_line()
    }

    fn parse_join(&mut self, query: &mut source::QuerySemantics) -> Result<(), Diagnostics> {
        self.bump();
        let raw_entity = self.take_word("joined entity")?;
        let entity = stable_fragment(&raw_entity);
        let alias = if self.consume("as") {
            self.take_word("join alias")?
        } else {
            stable_fragment(&raw_entity)
        };
        self.expect("on", "JDL0918", "a join needs `on`")?;
        let mut mappings = Vec::new();
        loop {
            let local = self.parse_field_path()?;
            self.expect("->", "JDL0918", "a join mapping needs `->`")?;
            let remote = self.parse_field_path()?;
            mappings.push(source::FieldMapping { local, remote });
            if !self.consume(",") {
                break;
            }
        }
        query.joins.push(source::Join {
            entity,
            alias,
            mappings,
        });
        self.end_line()
    }

    pub(super) fn parse_route(&mut self) -> Result<source::OperationRoute, Diagnostics> {
        self.bump();
        let method_word = self.take_word("HTTP method")?.to_ascii_lowercase();
        let method = EndpointMethod::parse(&method_word).map_err(|message| {
            self.here("JDL0920", message, "use GET, POST, PUT, PATCH, or DELETE")
        })?;
        let path = self.take_string("route path")?;
        let consumes = if self.consume("consumes") {
            match self.take_word("request format")?.as_str() {
                "json" => Some(RequestFormat::Json),
                "form" => Some(RequestFormat::Form),
                "none" => None,
                other => {
                    return Err(self.here(
                        "JDL0921",
                        format!("unknown request format `{other}`"),
                        "use json, form, or none",
                    ));
                }
            }
        } else {
            None
        };
        self.end_line()?;
        Ok(source::OperationRoute {
            method,
            path,
            consumes,
        })
    }

    pub(super) fn parse_binding(&mut self) -> Result<source::ParameterBinding, Diagnostics> {
        self.bump();
        let parameter = self.take_word("bound parameter")?;
        self.expect("from", "JDL0922", "a binding needs `from`")?;
        let source = match self.take_word("binding source")?.as_str() {
            "path" => source::BindingSource::Path,
            "query" => source::BindingSource::Query,
            "header" => source::BindingSource::Header,
            "claim" => source::BindingSource::Claim,
            "form" => source::BindingSource::Form,
            other => {
                return Err(self.here(
                    "JDL0923",
                    format!("unknown binding source `{other}`"),
                    "use path, query, header, claim, or form",
                ));
            }
        };
        let wire_name = (self.kind() == super::TokenKind::String)
            .then(|| self.take_string("wire name"))
            .transpose()?;
        self.end_line()?;
        Ok(source::ParameterBinding {
            parameter,
            source,
            wire_name,
        })
    }

    fn operation_field_list(&mut self, ordered: bool) -> Result<Vec<String>, Diagnostics> {
        self.expect("[", "JDL0924", "expected a bracketed field list")?;
        let mut fields = Vec::new();
        loop {
            let field = self.parse_field_path()?;
            if ordered && (self.consume("asc") || self.consume("desc")) {
                unreachable!("ordering is parsed by operation_order_list")
            }
            fields.push(field);
            if self.consume("]") {
                break;
            }
            self.expect(",", "JDL0924", "separate fields with `,`")?;
        }
        Ok(fields)
    }

    fn operation_order_list(&mut self) -> Result<Vec<source::Ordering>, Diagnostics> {
        self.expect("[", "JDL0925", "expected a bracketed order list")?;
        let mut order = Vec::new();
        loop {
            let field = self.parse_field_path()?;
            let direction = if self.consume("desc") {
                source::SortDirection::Desc
            } else {
                self.consume("asc");
                source::SortDirection::Asc
            };
            order.push(source::Ordering { field, direction });
            if self.consume("]") {
                break;
            }
            self.expect(",", "JDL0925", "separate ordering fields with `,`")?;
        }
        Ok(order)
    }

    fn parse_field_path(&mut self) -> Result<String, Diagnostics> {
        let first = self.take_word("field reference")?;
        self.finish_field_path(first).map(|(path, _)| path)
    }

    fn finish_field_path(&mut self, first: String) -> Result<(String, String), Diagnostics> {
        let mut segments = first.split('.').map(str::to_string).collect::<Vec<_>>();
        while self.consume(".") {
            segments.push(self.take_word("field path segment")?);
        }
        let name = segments.last().cloned().expect("a path has one segment");
        Ok((
            segments
                .into_iter()
                .map(|segment| stable_fragment(&segment))
                .collect::<Vec<_>>()
                .join("."),
            name,
        ))
    }

    fn take_literal(&mut self) -> Result<source::Value, Diagnostics> {
        if self.kind() == super::TokenKind::String {
            return self.take_string("literal").map(source::Value::String);
        }
        let first = self.text().to_string();
        if !matches!(
            self.kind(),
            super::TokenKind::Word | super::TokenKind::Integer
        ) {
            return Err(self.here(
                "JDL0926",
                "expected a scalar literal",
                "use a string, signed number, boolean, or enum constant",
            ));
        }
        self.bump();
        if self.consume(".") {
            let fraction = self.take_integer()?;
            return Ok(source::Value::Decimal(format!("{first}.{fraction}")));
        }
        Ok(match first.as_str() {
            "true" => source::Value::Boolean(true),
            "false" => source::Value::Boolean(false),
            _ if first.parse::<i128>().is_ok() => source::Value::Integer(first),
            _ => source::Value::EnumConstant(first),
        })
    }

    fn duplicate_member(&self, member: &str) -> Diagnostics {
        self.here(
            "JDL0927",
            format!("`{member}` is declared more than once"),
            format!("keep one `{member}` statement"),
        )
    }
}

fn compatibility_fields(parameters: &[source::OperationParameter]) -> Vec<String> {
    parameters
        .iter()
        .filter_map(|parameter| match &parameter.source {
            source::ParameterSource::Field { path } if !path.contains('.') => Some(path.clone()),
            _ => None,
        })
        .collect()
}

fn compatibility_route(route: &source::OperationRoute) -> String {
    let method = match route.method {
        EndpointMethod::Get => "GET",
        EndpointMethod::Post => "POST",
        EndpointMethod::Put => "PUT",
        EndpointMethod::Patch => "PATCH",
        EndpointMethod::Delete => "DELETE",
    };
    format!("{method} {}", route.path)
}

pub(super) fn value_from_attribute(
    value: &str,
    parser: &Parser<'_>,
) -> Result<source::Value, Diagnostics> {
    if value.starts_with('"') {
        serde_json::from_str(value)
            .map(source::Value::String)
            .map_err(|error| {
                parser.here(
                    "JDL0917",
                    format!("invalid default string: {error}"),
                    "use a valid JSON string literal",
                )
            })
    } else if value == "true" {
        Ok(source::Value::Boolean(true))
    } else if value == "false" {
        Ok(source::Value::Boolean(false))
    } else if value.parse::<i128>().is_ok() {
        Ok(source::Value::Integer(value.to_string()))
    } else if value.parse::<f64>().is_ok() && value.contains('.') {
        Ok(source::Value::Decimal(value.to_string()))
    } else if let Some(name) = value.strip_suffix("()") {
        Ok(source::Value::Function(source::FunctionCall {
            name: name.to_string(),
            arguments: Vec::new(),
        }))
    } else if value.chars().all(|character| {
        character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
    }) && value
        .chars()
        .any(|character| character.is_ascii_uppercase())
    {
        Ok(source::Value::EnumConstant(value.to_string()))
    } else {
        Err(parser.here(
            "JDL0917",
            format!("`{value}` is not a closed default expression"),
            "quote strings or use a scalar, enum constant, uuid7(), identity(), now(), or today()",
        ))
    }
}

fn replace_once<T>(
    target: &mut Vec<T>,
    values: Vec<T>,
    member: &str,
    parser: &Parser<'_>,
) -> Result<(), Diagnostics> {
    if !target.is_empty() {
        return Err(parser.duplicate_member(member));
    }
    *target = values;
    Ok(())
}
