//! `component cases <Name> { source <path> }`: scenario test class.

use super::{Emitted, java};
use crate::Diagnostic;
use crate::emit_java::JavaUnit;
use jails_contracts::ProjectPath;
use jails_model::{AppModel, Component, Package};
use std::collections::BTreeSet;

pub(super) fn files(
    model: &AppModel,
    component: &Component,
    snapshot: &jails_contracts::WorkspaceSnapshot,
) -> Result<Vec<Emitted>, Diagnostic> {
    let package = format!("{}.cases", model.project.package_for(Package::Base));
    let type_name = component.kind.primary_type(&component.name);

    let mut scenarios = Vec::new();
    if let Some(source_path) = &component.source {
        let content = ProjectPath::parse(source_path)
            .ok()
            .and_then(|p| snapshot.files.get(&p))
            .map(|f| String::from_utf8_lossy(&f.bytes).to_string());

        if let Some(text) = content {
            for line in text.lines() {
                let trimmed = line.trim();
                let bullet = trimmed
                    .strip_prefix("- ")
                    .or_else(|| trimmed.strip_prefix("* "))
                    .map(|rest| rest.trim());
                if let Some(desc) = bullet
                    && !desc.is_empty()
                {
                    scenarios.push(desc.to_string());
                }
            }
        }
    }

    if scenarios.is_empty() {
        scenarios.push("scenario".to_string());
    }

    let mut methods = String::new();
    for desc in &scenarios {
        let words: Vec<&str> = desc
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .filter(|p| !p.is_empty())
            .collect();
        let method_name = if words.is_empty() {
            "scenario".to_string()
        } else {
            let mut name = String::new();
            for (i, word) in words.iter().enumerate() {
                let mut chars = word.chars();
                if i == 0 {
                    if let Some(first) = chars.next() {
                        name.push(first.to_ascii_lowercase());
                        name.push_str(chars.as_str());
                    }
                } else {
                    if let Some(first) = chars.next() {
                        name.push(first.to_ascii_uppercase());
                        name.push_str(chars.as_str());
                    }
                }
            }
            name
        };
        let safe_name = if method_name.is_empty()
            || !method_name.chars().next().unwrap().is_ascii_alphabetic()
        {
            format!("test_{method_name}")
        } else {
            method_name
        };
        let escaped_desc = desc.replace('\"', "\\\"");
        methods.push_str(&format!(
            "    @Test\n    @Disabled(\"pending implementation\")\n    @DisplayName(\"{escaped_desc}\")\n    void {safe_name}() {{\n    }}\n\n"
        ));
    }

    let body = format!("public class {type_name} {{\n\n{methods}}}\n");

    let mut imports = BTreeSet::new();
    imports.insert("org.junit.jupiter.api.Disabled".to_string());
    imports.insert("org.junit.jupiter.api.DisplayName".to_string());
    imports.insert("org.junit.jupiter.api.Test".to_string());

    let unit = JavaUnit::new(&package, &imports, &body);
    let emitted = java(component, "cases", &package, &type_name, true, true, unit)?;
    Ok(vec![emitted])
}
