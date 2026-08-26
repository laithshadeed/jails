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
            let project = model::Project::discover()?;
            let mut document = emit(&project, format, ContractScopeArg::Source);
            document.push('\n');
            match out {
                None => {
                    print!("{document}");
                    Ok(())
                }
                Some(out) => {
                    let out = out.to_str().ok_or_else(|| {
                        "contract output paths must be UTF-8.\n       fix: choose a project-relative UTF-8 path."
                            .to_string()
                    })?;
                    let target = jails_protocol::identity::ProjectPath::parse(out)?;
                    crate::dispatch::mutate(invocation, false, |run| {
                        jails_engine::route::contract_emit(
                            run,
                            target.clone(),
                            document.as_bytes().to_vec(),
                            format == ContractFormatArg::JsonSchema,
                        )
                    })
                }
            }
        }
        ContractCommand::Check { against, scope } => {
            let project = model::Project::discover()?;
            let current = emit(&project, ContractFormatArg::Openapi, scope);
            let baseline = baseline(&project, &against)?;
            let breaking = compatibility_breaks(&baseline, &current)?;
            if invocation.output == Output::Json {
                println!(
                    "{}",
                    json!({
                        "schema": "jails.contract-check.v1",
                        "scope": scope_label(scope),
                        "against": against,
                        "status": if breaking.is_empty() { "compatible" } else { "breaking" },
                        "breaking": breaking,
                        "evidence": "source-observed"
                    })
                );
            } else if breaking.is_empty() {
                println!(
                    "contract compatible against {against} [{}; source-observed]",
                    scope_label(scope)
                );
            } else {
                for finding in &breaking {
                    println!("BREAKING  {finding} [source-observed]");
                }
            }
            if breaking.is_empty() {
                Ok(())
            } else {
                Err(jails_support::Failure::Told(
                    "contract check found backward-incompatible changes.\n       fix: restore compatibility or publish and approve a new contract version."
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

const HTTP_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

fn compatibility_breaks(baseline: &str, current: &str) -> Result<Vec<String>> {
    let baseline: Value = serde_json::from_str(baseline)
        .map_err(|error| format!("invalid baseline contract JSON: {error}"))?;
    let current: Value = serde_json::from_str(current)
        .map_err(|error| format!("invalid current contract JSON: {error}"))?;
    let baseline_paths = baseline
        .get("paths")
        .and_then(Value::as_object)
        .ok_or_else(|| "contract has no OpenAPI `paths` object".to_string())?;
    let current_paths = current
        .get("paths")
        .and_then(Value::as_object)
        .ok_or_else(|| "current contract has no OpenAPI `paths` object".to_string())?;
    let mut findings = BTreeSet::new();
    for (path, baseline_item) in baseline_paths {
        let Some(current_item) = current_paths.get(path).and_then(Value::as_object) else {
            for method in baseline_item
                .as_object()
                .into_iter()
                .flat_map(Map::keys)
                .filter(|method| HTTP_METHODS.contains(&method.as_str()))
            {
                findings.insert(format!("removed {} {path}", method.to_ascii_uppercase()));
            }
            continue;
        };
        let Some(baseline_item) = baseline_item.as_object() else {
            continue;
        };
        for method in baseline_item
            .keys()
            .filter(|method| HTTP_METHODS.contains(&method.as_str()))
        {
            let label = format!("{} {path}", method.to_ascii_uppercase());
            let Some(current_operation) = current_item.get(method) else {
                findings.insert(format!("removed {label}"));
                continue;
            };
            let baseline_operation = &baseline_item[method];
            compare_responses(&label, baseline_operation, current_operation, &mut findings);
            compare_required_inputs(&label, baseline_operation, current_operation, &mut findings);
            compare_security(&label, baseline_operation, current_operation, &mut findings);
            compare_schema(
                &label,
                "$",
                baseline_operation,
                current_operation,
                &mut findings,
            );
        }
    }
    Ok(findings.into_iter().collect())
}

fn compare_responses(
    label: &str,
    baseline: &Value,
    current: &Value,
    findings: &mut BTreeSet<String>,
) {
    let baseline = baseline
        .get("responses")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(Map::keys);
    let current = current.get("responses").and_then(Value::as_object);
    for status in baseline {
        if current.is_none_or(|responses| !responses.contains_key(status)) {
            findings.insert(format!("removed response {status} from {label}"));
        }
    }
}

fn compare_required_inputs(
    label: &str,
    baseline: &Value,
    current: &Value,
    findings: &mut BTreeSet<String>,
) {
    let baseline_required = required_parameters(baseline);
    for parameter in required_parameters(current).difference(&baseline_required) {
        findings.insert(format!("newly required input {parameter} on {label}"));
    }
    let baseline_body = baseline
        .pointer("/requestBody/required")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let current_body = current
        .pointer("/requestBody/required")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if current_body && !baseline_body {
        findings.insert(format!("newly required request body on {label}"));
    }
}

fn required_parameters(operation: &Value) -> BTreeSet<String> {
    operation
        .get("parameters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|parameter| {
            parameter
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|parameter| {
            Some(format!(
                "{}:{}",
                parameter.get("in")?.as_str()?,
                parameter.get("name")?.as_str()?
            ))
        })
        .collect()
}

fn compare_security(
    label: &str,
    baseline: &Value,
    current: &Value,
    findings: &mut BTreeSet<String>,
) {
    let baseline = baseline.get("security").and_then(Value::as_array);
    let current = current.get("security").and_then(Value::as_array);
    if current.is_some_and(|security| !security.is_empty())
        && baseline.is_none_or(|security| security.is_empty() || current != Some(security))
    {
        findings.insert(format!("stricter authentication policy on {label}"));
    }
}

fn compare_schema(
    label: &str,
    pointer: &str,
    baseline: &Value,
    current: &Value,
    findings: &mut BTreeSet<String>,
) {
    if let (Some(before), Some(after)) = (
        baseline.get("type").and_then(Value::as_str),
        current.get("type").and_then(Value::as_str),
    ) && before != after
    {
        findings.insert(format!(
            "narrowed type at {label} {pointer}: {before} -> {after}"
        ));
    }
    if let Some(before) = baseline.get("enum").and_then(Value::as_array) {
        let after = current.get("enum").and_then(Value::as_array);
        for value in before {
            if after.is_none_or(|values| !values.contains(value)) {
                findings.insert(format!("removed enum value {value} at {label} {pointer}"));
            }
        }
    }
    if let (Some(before), Some(after)) = (baseline.as_object(), current.as_object()) {
        for (key, before) in before {
            if let Some(after) = after.get(key) {
                compare_schema(label, &format!("{pointer}/{key}"), before, after, findings);
            }
        }
    } else if let (Some(before), Some(after)) = (baseline.as_array(), current.as_array()) {
        for (index, (before, after)) in before.iter().zip(after).enumerate() {
            compare_schema(
                label,
                &format!("{pointer}/{index}"),
                before,
                after,
                findings,
            );
        }
    }
}

fn scope_label(scope: ContractScopeArg) -> &'static str {
    match scope {
        ContractScopeArg::Declared => "declared",
        ContractScopeArg::Source => "source-observed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_covers_every_normative_breaking_shape() {
        let baseline = json!({"paths": {"/orders": {"post": {
            "responses": {"200": {}, "201": {}},
            "parameters": [{"in": "query", "name": "limit", "required": false, "schema": {"type": "number", "enum": [1, 2]}}]
        }}}});
        let current = json!({"paths": {"/orders": {"post": {
            "responses": {"200": {}},
            "parameters": [{"in": "query", "name": "limit", "required": true, "schema": {"type": "integer", "enum": [1]}}],
            "requestBody": {"required": true},
            "security": [{"oauth": ["write"]}]
        }}}});
        let findings = compatibility_breaks(&baseline.to_string(), &current.to_string()).unwrap();
        for expected in [
            "removed response 201",
            "newly required input query:limit",
            "newly required request body",
            "narrowed type",
            "removed enum value 2",
            "stricter authentication policy",
        ] {
            assert!(
                findings.iter().any(|finding| finding.contains(expected)),
                "missing `{expected}`: {findings:?}"
            );
        }
    }
}
