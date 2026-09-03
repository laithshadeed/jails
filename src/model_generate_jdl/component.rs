//! Typed JDL v1 frontend for the closed component registry.

use crate::ArtifactKind;
use crate::cli::GenerateArgs;
use jails_model::field_syntax::java_to_label;
use jails_model::{ComponentKind, UnitKind};
use jails_support::{Failure, Result};
use std::path::Path;

pub(super) fn v1_declaration(
    kind: &str,
    name: &str,
    variants: &[String],
    args: &GenerateArgs,
    model: &jails_model::AppModel,
) -> Result<String> {
    let parameters = if matches!(
        args.kind,
        ArtifactKind::Sealed | ArtifactKind::Strategy | ArtifactKind::Cases
    ) {
        Vec::new()
    } else {
        args.fields
            .iter()
            .map(|field| component_parameter(field))
            .collect::<Result<Vec<_>>>()?
    };
    let mut members = Vec::new();
    if let Some(on) = &args.strategy_on {
        members.push(format!("  on {}", reference_label(model, on)));
    }
    if let Some(yields) = &args.strategy_yields {
        members.push(format!("  yields {}", reference_label(model, yields)));
    }
    // A client with no `--path` gets the collection route written down, not
    // required of the reader and not invented later by the compiler: the
    // interface it renders is a collection -- `findAll` and `findById` -- and
    // its route is `/{plural}`. It is materialized rather than defaulted
    // downstream because JDL v1 §3.1 rule 4 makes a convention part of the
    // language, not something a compiler applies out of sight.
    let default_path = if args.path.is_some() {
        None
    } else if args.kind == ArtifactKind::Client {
        Some(format!(
            "/{}",
            // `sql::table_name`, not `jails_model::plural_snake_case`.
            // `both_pluralizers_answer_the_same_for_every_specified_rule`
            // pins the two over §9.7's vocabulary, and that gate covers the
            // pluralisation rules, not the whole projection
            // `SqlName::conventional_table` performs on the way to them. A
            // default route is reader-visible API, so it stays on the one
            // function that produces it.
            jails_model::plural_snake_case(&class_to_snake(name)).replace('_', "-")
        ))
    } else if args.method.is_some() || args.consumes.is_some() {
        // **A method or request format without a path is not a missing path.**
        // The convention already answers it: a component with no route lowers
        // to `/{label}`, which is what `g controller Foo` gets. Refusing here
        // would make `--method post` demand a `--path` that only restates the
        // default.
        //
        // Materialized rather than left to the compiler for the same reason
        // the client default is, one arm up: JDL v1 §3.1 rule 4 makes a
        // convention part of the language rather than something applied out of
        // sight, so the route the reader gets is the route their model states.
        Some(format!("/{}", java_to_label(name).replace('_', "-")))
    } else {
        None
    };
    if let Some(path) = args.path.as_ref().or(default_path.as_ref()) {
        let default_method = if args.kind == ArtifactKind::Webhook {
            "POST"
        } else {
            "GET"
        };
        let method = args.method.map_or(default_method.to_string(), |method| {
            method.wire_name().to_string()
        });
        let path = serde_json::to_string(path)
            .map_err(|error| Failure::Told(format!("could not quote route path: {error}")))?;
        let consumes = args
            .consumes
            .map(|format| format!(" consumes {}", format.label()))
            .unwrap_or_default();
        members.push(format!("  route {method} {path}{consumes}"));
    }
    for binding in &args.bind {
        let (parameter, wire_name) = binding.split_once('=').ok_or_else(|| {
            Failure::Told(format!(
                "`{binding}` is not a component binding.\n       fix: use `parameter=wire_name`"
            ))
        })?;
        let parameter = parameter.trim();
        let wire_name = wire_name.trim();
        if parameter.is_empty() || wire_name.is_empty() {
            return Err(Failure::Told(format!(
                "`{binding}` has an empty component parameter or wire name.\n       fix: use `parameter=wire_name`"
            )));
        }
        let wire_name = serde_json::to_string(wire_name)
            .map_err(|error| Failure::Told(format!("could not quote wire name: {error}")))?;
        members.push(format!("  bind {parameter} from form {wire_name}"));
    }
    for variant in variants {
        // The parser hangs a variant's id off its component's, so the
        // derivation reads the same value back and the attribute is a pin
        // that displaces nothing.
        members.push(format!("  variant {variant}"));
    }
    if args.kind == ArtifactKind::Cases {
        let source = serde_json::to_string(&args.name)
            .map_err(|error| Failure::Told(format!("could not quote cases source: {error}")))?;
        members.push(format!("  source {source}"));
    }
    let parameters = if parameters.is_empty() {
        String::new()
    } else {
        format!("({})", parameters.join(", "))
    };
    if members.is_empty() {
        return Ok(format!("component {kind} {name}{parameters} {{}}\n"));
    }
    Ok(format!(
        "component {kind} {name}{parameters} {{\n{}\n}}\n",
        members.join("\n")
    ))
}

fn component_parameter(source: &str) -> Result<String> {
    let field = crate::model_generate::parse_field(source)?;
    if field.primary_key
        || field.unique
        || field.indexed
        || field.scoped
        || field.version
        || field.updated
        || field.mapped_column.is_some()
    {
        return Err(Failure::Told(format!(
            "component parameter `{}` cannot carry entity storage or compiler-managed markers.\n       fix: keep only payload constraints and defaults",
            field.java_name
        )));
    }
    let mut rendered = format!(
        "{}: {}{}",
        field.java_name,
        field.type_name,
        if field.required { "" } else { "?" }
    );
    if let Some(default) = &field.default {
        rendered.push_str(&format!(" @default({default})"));
    }
    if field.min_length.is_some() || field.max_length.is_some() {
        rendered.push_str(&format!(
            " @length({}..{})",
            field
                .min_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
            field
                .max_length
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ));
    }
    if field.nonnegative {
        rendered.push_str(" @nonnegative");
    }
    if field.non_blank {
        rendered.push_str(" @notBlank");
    }
    if field.positive {
        rendered.push_str(" @positive");
    }
    Ok(rendered)
}

fn reference_label(model: &jails_model::AppModel, requested: &str) -> String {
    let requested_label = java_to_label(requested);
    if let Some(entity) = model
        .entities
        .values()
        .find(|entity| entity.label == requested_label || entity.names.java_type == requested)
    {
        return entity.label.clone();
    }
    if let Some(operation) = model.operations.values().find(|operation| {
        operation.label == requested_label || operation.names.java_type == requested
    }) {
        return operation.label.clone();
    }
    if let Some(component) = model.components.values().find(|component| {
        component.label == requested_label
            || component.name == requested
            || component_primary_name(component.kind, &component.name) == requested
    }) {
        return component.label.clone();
    }
    requested_label
}

fn component_primary_name(kind: ComponentKind, stem: &str) -> String {
    let suffix = match kind {
        ComponentKind::Service => "Service",
        ComponentKind::Controller => "Controller",
        ComponentKind::Handler => "Handler",
        ComponentKind::Command => "Command",
        ComponentKind::Cli => "Cli",
        ComponentKind::Cases => "Cases",
        ComponentKind::Client => "Client",
        ComponentKind::Fetcher => "Fetcher",
        ComponentKind::Job => "Job",
        ComponentKind::HttpWorkflow => "Workflow",
        ComponentKind::HttpSink => "HttpOutboxSink",
        ComponentKind::Idempotency => "Guard",
        ComponentKind::Auth => "TokenConfig",
        ComponentKind::Webhook => "Verifier",
        ComponentKind::DurableJob => "Work",
        ComponentKind::Socket => "SocketHandler",
        ComponentKind::Presence => "Presence",
        ComponentKind::Test => "Test",
        ComponentKind::IntegrationTest => "IT",
        ComponentKind::Class
        | ComponentKind::Interface
        | ComponentKind::Sealed
        | ComponentKind::Strategy => "",
    };
    format!("{stem}{suffix}")
}

pub(super) fn replace_v1_declaration(
    source: &str,
    name: &str,
    replacement: &str,
) -> Result<String> {
    let cst = jails_model::parse_jdl_cst(source).map_err(super::jdl_edit_failure)?;
    let matches = cst
        .declarations
        .iter()
        .filter(|declaration| {
            declaration.kind == "component" && declaration.name.as_deref() == Some(name)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [declaration] => cst
            .replace_declaration(declaration, replacement)
            .map_err(super::jdl_edit_failure),
        [] => Err(Failure::Told(format!(
            "could not find JDL component `{name}`\n       fix: restore its declaration, then retry"
        ))),
        _ => Err(Failure::Told(format!(
            "JDL component `{name}` is ambiguous\n       fix: keep one declaration with this name"
        ))),
    }
}

pub(crate) fn component_kind(kind: ArtifactKind) -> Option<ComponentKind> {
    Some(match kind {
        ArtifactKind::Class => ComponentKind::Class,
        ArtifactKind::Interface => ComponentKind::Interface,
        ArtifactKind::Service => ComponentKind::Service,
        ArtifactKind::Controller => ComponentKind::Controller,
        ArtifactKind::Sealed => ComponentKind::Sealed,
        ArtifactKind::Strategy => ComponentKind::Strategy,
        ArtifactKind::Handler => ComponentKind::Handler,
        ArtifactKind::Command => ComponentKind::Command,
        ArtifactKind::Cli => ComponentKind::Cli,
        ArtifactKind::Cases => ComponentKind::Cases,
        ArtifactKind::Client => ComponentKind::Client,
        ArtifactKind::Fetcher => ComponentKind::Fetcher,
        ArtifactKind::Job => ComponentKind::Job,
        ArtifactKind::HttpWorkflow => ComponentKind::HttpWorkflow,
        ArtifactKind::HttpSink => ComponentKind::HttpSink,
        ArtifactKind::Idempotency => ComponentKind::Idempotency,
        ArtifactKind::Auth => ComponentKind::Auth,
        ArtifactKind::Webhook => ComponentKind::Webhook,
        ArtifactKind::DurableJob => ComponentKind::DurableJob,
        ArtifactKind::Socket => ComponentKind::Socket,
        ArtifactKind::Presence => ComponentKind::Presence,
        ArtifactKind::Test => ComponentKind::Test,
        ArtifactKind::IntegrationTest => ComponentKind::IntegrationTest,
        ArtifactKind::Scaffold
        | ArtifactKind::Record
        | ArtifactKind::Field
        | ArtifactKind::Factory
        | ArtifactKind::Value
        | ArtifactKind::Enum
        | ArtifactKind::Repo
        | ArtifactKind::Migration
        | ArtifactKind::Association
        | ArtifactKind::Search
        | ArtifactKind::Dto
        | ArtifactKind::Usecase
        | ArtifactKind::Query
        | ArtifactKind::Transition
        | ArtifactKind::Event
        | ArtifactKind::Seed => return None,
    })
}

pub(super) fn legacy_unit_kind(kind: ArtifactKind) -> Option<UnitKind> {
    match kind {
        ArtifactKind::Class => Some(UnitKind::Class),
        ArtifactKind::Interface => Some(UnitKind::Interface),
        ArtifactKind::Service => Some(UnitKind::Service),
        ArtifactKind::Test => Some(UnitKind::Test),
        ArtifactKind::IntegrationTest => Some(UnitKind::IntegrationTest),
        ArtifactKind::Sealed => Some(UnitKind::Sealed),
        ArtifactKind::Strategy => Some(UnitKind::Strategy),
        ArtifactKind::Controller => Some(UnitKind::Controller),
        _ => None,
    }
}

pub(crate) fn component_stem(kind: ArtifactKind, requested: &str) -> Result<String> {
    if kind == ArtifactKind::Cases {
        return cases_stem(Path::new(requested));
    }
    Ok(crate::strip_redundant_suffix(kind, requested))
}

fn cases_stem(source: &Path) -> Result<String> {
    let raw = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| {
            Failure::Told(format!(
                "{} has no file name from which to derive a cases component.\n       fix: pass a project-relative markdown file",
                source.display()
            ))
        })?;
    let mut stem = String::new();
    let mut uppercase = true;
    for character in raw.chars() {
        if character.is_ascii_alphanumeric() {
            if uppercase {
                stem.extend(character.to_uppercase());
                uppercase = false;
            } else {
                stem.push(character);
            }
        } else {
            uppercase = true;
        }
    }
    if stem.is_empty() {
        return Err(Failure::Told(format!(
            "cannot derive a cases component from {}.\n       fix: give the markdown file an ASCII letter or digit in its name",
            source.display()
        )));
    }
    if stem.starts_with(|character: char| character.is_ascii_digit()) {
        stem.insert_str(0, "Case");
    }
    Ok(stem)
}

pub(super) fn reject_v1_options(args: &GenerateArgs, kind: ComponentKind) -> Result<()> {
    use ComponentKind as K;
    let accepts_on = matches!(
        kind,
        K::Controller
            | K::Strategy
            | K::Command
            | K::Client
            | K::HttpWorkflow
            | K::HttpSink
            | K::DurableJob
    );
    let requires_on = matches!(
        kind,
        K::Strategy | K::HttpWorkflow | K::HttpSink | K::DurableJob
    );
    let accepts_yields = matches!(
        kind,
        K::Controller | K::Strategy | K::Client | K::HttpSink | K::DurableJob
    );
    let requires_yields = matches!(kind, K::HttpSink | K::DurableJob);
    let accepts_route = matches!(
        kind,
        K::Controller | K::Handler | K::Client | K::Webhook | K::Socket
    );
    // **A client with no `--path` is not refused**: the collection route is
    // a documented default, materialized in `v1_declaration`.
    let accepts_bind = matches!(kind, K::Controller | K::Webhook);
    let unrelated = args.timestamps
        || args.default_literal.is_some()
        || args.backfill_file.is_some()
        || !args.indexes.is_empty()
        || !args.uniques.is_empty()
        || args.via.is_some()
        || args.order_by.is_some()
        || args.limit.is_some()
        || args.on_conflict.is_some()
        || args.select.is_some()
        || !args.set.is_empty()
        || args.if_match.is_some()
        || (!accepts_on && args.strategy_on.is_some())
        || (!accepts_yields && args.strategy_yields.is_some())
        || (!accepts_route && args.path.is_some())
        || (!accepts_route && args.method.is_some())
        || (!accepts_route && args.consumes.is_some())
        || (!accepts_bind && !args.bind.is_empty())
        || (args.kind == ArtifactKind::Cases && !args.fields.is_empty());
    if unrelated {
        return Err(Failure::Told(format!(
            "component {} received flags outside its closed JDL v1 schema.\n       fix: remove unrelated flags and use only the registry members for this kind",
            kind.label()
        )));
    }
    if requires_on && args.strategy_on.is_none() {
        return Err(Failure::Told(format!(
            "component {} requires `--on`.\n       fix: name its input semantic symbol",
            kind.label()
        )));
    }
    if requires_yields && args.strategy_yields.is_none() {
        return Err(Failure::Told(format!(
            "component {} requires `--yields`.\n       fix: name its output semantic symbol",
            kind.label()
        )));
    }
    if !args.bind.is_empty() && args.consumes != Some(jails_model::RequestFormat::Form) {
        return Err(Failure::Told(
            "component `--bind` overrides are valid only with `--consumes form`.\n       fix: select form consumption or remove the bindings"
                .to_string(),
        ));
    }
    Ok(())
}

/// `LedgerEntry` to `ledger_entry`: the class name in the spelling the
/// pluraliser and the SQL projection both read.
fn class_to_snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
