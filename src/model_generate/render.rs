//! What a `.jails/model.toml` declaration looks like as text.
//!
//! **This is the pre-v1 compatibility surface, and it is one secret.** JDL v1
//! is the authoring boundary and `model_generate_jdl` renders it; everything
//! here writes the temporary TOML input that existing canonical projects are
//! still on. Keeping the two apart is what stops a v1 grammar change being
//! made in a renderer that does not write v1 -- and the reverse, which is how
//! a deprecated alias comes back.
//!
//! Nothing here decides anything: the caller has already validated the flags
//! and resolved every token against the model, so each function is a shape.

use super::{
    MODEL_PATH, OperationProfile, ParsedField, operation_field_label, operation_field_labels,
    parse_field,
};
use crate::cli::GenerateArgs;
use crate::model_resource::java_to_label;
use jails_model::{AppModel, Facet};
use jails_support::{Failure, Result};
use std::collections::BTreeSet;

pub(super) fn operation_declaration(
    args: &GenerateArgs,
    profile: OperationProfile,
    model: &AppModel,
    label: &str,
) -> Result<String> {
    let on = args
        .strategy_on
        .as_deref()
        .expect("operation option validation requires --on");
    let on = java_to_label(on);
    let fields = operation_field_labels(model, &on, &args.fields)?;
    let fields = quoted_array(&fields)?;
    let mut output = format!(
        "[operations.{label}]\nkind = {}\nid = {}\njava_name = {}\non = {}\n",
        quoted(operation_kind(profile))?,
        quoted(&format!("op_{label}"))?,
        quoted(&args.name)?,
        quoted(&on)?,
    );
    match profile {
        OperationProfile::Command => {
            output.push_str(&format!("fields = {fields}\n"));
            // `--yields` on a use case is the legacy spelling of *staged*
            // delivery -- it is what `g usecase --yields E` has always built
            // an outbox for -- so it writes the policy as well as the event.
            // Emitting only `emits` would honour the flag with the weaker
            // guarantee, which is the substitution the policy exists to stop.
            if let Some(yields) = &args.strategy_yields {
                output.push_str(&format!(
                    "emits = [{}]\ndelivery = \"outbox\"\n",
                    quoted(&java_to_label(yields))?
                ));
            }
        }
        OperationProfile::Query => {
            output.push_str(&format!("filters = {fields}\n"));
            if let Some(order_by) = &args.order_by {
                let order_by = order_by
                    .split(',')
                    .map(str::trim)
                    .map(|item| {
                        if item.is_empty() || item.contains(char::is_whitespace) {
                            return Err(Failure::Told(format!(
                                "canonical query ordering does not yet represent directions in `{item}`.\n       fix: use a comma-separated field list without `asc`/`desc`, or declare the query directly in `{MODEL_PATH}`"
                            )));
                        }
                        operation_field_label(model, &on, item)
                    })
                    .collect::<Result<Vec<_>>>()?;
                output.push_str(&format!("order_by = {}\n", quoted_array(&order_by)?));
            }
            if let Some(limit) = args.limit {
                output.push_str(&format!("limit = {limit}\n"));
            }
        }
        OperationProfile::Transition => {
            output.push_str(&format!("fields = {fields}\nsets = {fields}\n"));
            if let Some(yields) = &args.strategy_yields {
                output.push_str(&format!("yields = {}\n", quoted(&java_to_label(yields))?));
            }
        }
        OperationProfile::Event => {
            output.push_str(&format!("fields = {fields}\n"));
        }
    }
    if let Some(path) = &args.path {
        let method = match profile {
            OperationProfile::Command => "POST".to_string(),
            // **A query answers GET, whatever its filters** -- the same rule
            // the JDL frontend applies, stated once in each because these are
            // two renderers of one decision and a divergence here is a route
            // that changes when the authoring format does. `--consumes json`
            // is the one way to ask for a body.
            OperationProfile::Query
                if args.consumes == Some(jails_spec::spec::kind::WireFormat::Json)
                    && !args.fields.is_empty() =>
            {
                "POST".to_string()
            }
            OperationProfile::Query => "GET".to_string(),
            OperationProfile::Transition => args.method.map_or_else(
                || "PUT".to_string(),
                |method| method.label().to_ascii_uppercase(),
            ),
            OperationProfile::Event => unreachable!("event paths are refused"),
        };
        output.push_str(&format!(
            "route = {}\n",
            quoted(&format!("{method} {path}"))?
        ));
    }
    Ok(output)
}

fn operation_kind(profile: OperationProfile) -> &'static str {
    match profile {
        OperationProfile::Command => "command",
        OperationProfile::Query => "query",
        OperationProfile::Transition => "transition",
        OperationProfile::Event => "event",
    }
}

fn quoted_array(values: &[String]) -> Result<String> {
    Ok(format!(
        "[{}]",
        values
            .iter()
            .map(|value| quoted(value))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ))
}

pub(crate) fn entity_declaration(
    label: &str,
    java_name: &str,
    facets: &[Facet],
    fields: &[String],
) -> Result<String> {
    let mut parsed = Vec::new();
    let mut labels = BTreeSet::new();
    for token in fields {
        let field = parse_field(token)?;
        if !labels.insert(field.label.clone()) {
            return Err(Failure::Told(format!(
                "field `{}` is declared more than once",
                field.java_name
            )));
        }
        parsed.push(field);
    }
    let facets = facets
        .iter()
        .map(|facet| quoted(facet_name(*facet)))
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let mut output = format!(
        "[entities.{label}]\nid = {}\njava_name = {}\nfacets = [{facets}]\n",
        quoted(&format!("ent_{label}"))?,
        quoted(java_name)?,
    );
    for field in parsed {
        output.push('\n');
        output.push_str(&field_declaration(label, &field)?);
    }
    Ok(output)
}

pub(crate) fn enum_declaration(label: &str, java_name: &str, values: &[String]) -> Result<String> {
    let values = values
        .iter()
        .map(|value| {
            jails_protocol::declaration::ConstantSpec::parse(value)
                .map(|constant| constant.canonical())
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(format!(
        "[entities.{label}]\nid = {}\njava_name = {}\nfacets = [\"enum\"]\nvalues = {}\n",
        quoted(&format!("ent_{label}"))?,
        quoted(java_name)?,
        quoted_array(&values)?,
    ))
}

pub(crate) fn field_declaration(entity: &str, field: &ParsedField) -> Result<String> {
    field.require_v1_for_rich_semantics()?;
    let mut output = format!(
        "[entities.{entity}.fields.{}]\nid = {}\njava_name = {}\ntype = {}\nrequired = {}\nnon_blank = {}\nprimary_key = {}\nunique = {}\nindexed = {}\n",
        field.label,
        quoted(&format!("fld_{entity}_{}", field.label))?,
        quoted(&field.java_name)?,
        quoted(&field.type_name)?,
        field.required,
        field.non_blank,
        field.primary_key,
        field.unique,
        field.indexed,
    );
    if let Some(min) = field.min_length {
        output.push_str(&format!("min_length = {min}\n"));
    }
    if let Some(max) = field.max_length {
        output.push_str(&format!("max_length = {max}\n"));
    }
    if let Some(column) = &field.mapped_column {
        output.push_str(&format!("column = {}\n", quoted(column)?));
    }
    Ok(output)
}

fn facet_name(facet: Facet) -> &'static str {
    match facet {
        Facet::Enum => "enum",
        Facet::Record => "record",
        Facet::Factory => "factory",
        Facet::Dto => "dto",
        Facet::Repository => "repository",
        Facet::Service => "service",
        Facet::Http => "http",
        Facet::Events => "events",
        Facet::Search => "search",
        Facet::Seed => "seed",
    }
}

fn quoted(value: &str) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|error| Failure::Told(format!("could not quote model value: {error}")))
}
