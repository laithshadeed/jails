//! Read-only portable HTTP contract projection and compatibility checks.

use crate::cli::{ContractCommand, ContractFormatArg, ContractScopeArg, Output};
use crate::{inspect, model};
use jails_support::Result;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::path::Path;

pub(crate) fn run(command: ContractCommand, invocation: crate::Invocation) -> Result<()> {
    match command {
        ContractCommand::Emit { format, out } => {
            if out.is_some() {
                return Err(
                    "`contract emit --out` requires a transaction-owned output path.\n       fix: omit `--out` and redirect stdout until the accepted plan writer is enabled."
                        .into(),
                );
            }
            let project = model::Project::discover()?;
            println!("{}", emit(&project, format, ContractScopeArg::Source));
            Ok(())
        }
        ContractCommand::Check { against, scope } => {
            let project = model::Project::discover()?;
            let current = emit(&project, ContractFormatArg::Openapi, scope);
            let baseline = baseline(&project, &against)?;
            let removed = route_set(&baseline)?
                .difference(&route_set(&current)?)
                .cloned()
                .collect::<Vec<_>>();
            if invocation.output == Output::Json {
                println!(
                    "{}",
                    json!({
                        "schema": "jails.contract-check.v1",
                        "scope": scope_label(scope),
                        "against": against,
                        "status": if removed.is_empty() { "compatible" } else { "breaking" },
                        "breaking": removed,
                        "evidence": "source-observed"
                    })
                );
            } else if removed.is_empty() {
                println!(
                    "contract compatible against {against} [{}; source-observed]",
                    scope_label(scope)
                );
            } else {
                for route in &removed {
                    println!("BREAKING  removed {route} [source-observed]");
                }
            }
            if removed.is_empty() {
                Ok(())
            } else {
                Err(jails_support::Failure::Told(
                    "contract check found backward-incompatible route removals.\n       fix: restore the operation or publish and approve a new contract version."
                        .into(),
                ))
            }
        }
    }
}

fn emit(project: &model::Project, format: ContractFormatArg, scope: ContractScopeArg) -> String {
    let routes = inspect::collect_routes(project.root());
    match format {
        ContractFormatArg::Openapi => {
            let mut paths = Map::new();
            for route in routes {
                let operations = paths
                    .entry(route.path)
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
                    .expect("path entry is an object");
                operations.insert(
                    route.verb.to_ascii_lowercase(),
                    json!({
                        "operationId": route.handler.replace('#', "_"),
                        "responses": {"200": {"description": "Observed response"}},
                        "x-jails-evidence": "static-inference"
                    }),
                );
            }
            serde_json::to_string_pretty(&json!({
                "openapi": "3.1.0",
                "info": {"title": "jails project contract", "version": "1"},
                "x-jails-schema": "jails.http-contract.v1",
                "x-jails-scope": scope_label(scope),
                "x-jails-observation": "source-observed",
                "paths": paths
            }))
            .expect("JSON value serializes")
        }
        ContractFormatArg::JsonSchema => serde_json::to_string_pretty(&json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "jails.http-routes.v1",
            "title": "Observed HTTP routes",
            "x-jails-scope": scope_label(scope),
            "type": "string",
            "enum": routes.into_iter().map(|route| format!("{} {}", route.verb, route.path)).collect::<Vec<_>>()
        }))
        .expect("JSON value serializes"),
    }
}

fn baseline(project: &model::Project, against: &str) -> Result<String> {
    let file = Path::new(against);
    if file.is_file() {
        return std::fs::read_to_string(file).map_err(|error| {
            format!("could not read contract {}: {error}", file.display()).into()
        });
    }
    let output = std::process::Command::new("git")
        .args(["show", &format!("{against}:.jails/contracts/openapi.json")])
        .current_dir(project.root())
        .output()
        .map_err(|error| format!("could not run git for contract baseline: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "contract baseline `{against}` is neither a file nor a Git revision containing `.jails/contracts/openapi.json`.\n       fix: pass an emitted contract file or commit that checked-in path."
        )
        .into());
    }
    String::from_utf8(output.stdout).map_err(|_| {
        "contract baseline is not UTF-8.\n       fix: use a JSON contract emitted by jails.".into()
    })
}

fn route_set(document: &str) -> Result<BTreeSet<String>> {
    let parsed: Value = serde_json::from_str(document)
        .map_err(|error| format!("invalid contract JSON: {error}.\n       fix: emit the baseline with `jails contract emit`."))?;
    let paths = parsed
        .get("paths")
        .and_then(Value::as_object)
        .ok_or_else(|| "contract has no OpenAPI `paths` object.\n       fix: use an OpenAPI contract emitted by jails.".to_string())?;
    let mut routes = BTreeSet::new();
    for (path, operations) in paths {
        let Some(operations) = operations.as_object() else {
            continue;
        };
        for method in operations.keys().filter(|method| {
            matches!(
                method.as_str(),
                "get" | "post" | "put" | "patch" | "delete" | "head" | "options"
            )
        }) {
            routes.insert(format!("{} {path}", method.to_ascii_uppercase()));
        }
    }
    Ok(routes)
}

fn scope_label(scope: ContractScopeArg) -> &'static str {
    match scope {
        ContractScopeArg::Declared => "declared",
        ContractScopeArg::Source => "source-observed",
    }
}
