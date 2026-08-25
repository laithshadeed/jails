//! Fast, closed-set source checks for APIs and architectural shortcuts that
//! compile successfully but conflict with the projects jails generates.

use crate::generate::find_project_root;
use jails_support::Result;
use std::path::Path;

pub(crate) struct Rule {
    pub needle: &'static str,
    pub replacement: &'static str,
    pub reason: &'static str,
}

/// The same table is rendered into generated AGENTS.md files. Keeping the
/// machine check and the agent guidance together prevents either from becoming
/// stale prose.
pub(crate) const RULES: &[Rule] = &[
    Rule {
        needle: "@MockBean",
        replacement: "@MockitoBean",
        reason: "the former Spring Boot test annotation is deprecated",
    },
    Rule {
        needle: "javax.validation",
        replacement: "jakarta.validation",
        reason: "current Spring uses the Jakarta namespace",
    },
    Rule {
        needle: "spring-boot-starter-web</artifactId>",
        replacement: "spring-boot-starter-webmvc",
        reason: "Boot 4 splits the MVC starter explicitly",
    },
    Rule {
        needle: "@Entity",
        replacement: "a record plus a generated repository port and explicit JDBC adapter",
        reason: "jails projects use explicit SQL rather than an ORM",
    },
    Rule {
        needle: "lombok.",
        replacement: "records or explicit Java",
        reason: "generated methods hide the API from compiler and editor checks",
    },
    Rule {
        needle: "--enable-preview",
        replacement: "a non-preview Java API",
        reason: "generated applications must run on a standard release toolchain",
    },
];

pub fn lint() -> Result<()> {
    let root = find_project_root()?;
    let mut findings = Vec::new();
    inspect_file(&root, &root.join("pom.xml"), &mut findings);
    for tree in ["src/main/java", "src/test/java"] {
        for path in crate::java::source_files(&root.join(tree)) {
            inspect_file(&root, &path, &mut findings);
        }
    }
    if findings.is_empty() {
        println!("lint: all clear ({} rules)", RULES.len());
        return Ok(());
    }
    findings.sort();
    for finding in &findings {
        println!("{finding}");
    }
    println!();
    println!("lint: {} finding(s)", findings.len());
    Err(String::new())
}

fn inspect_file(root: &Path, path: &Path, findings: &mut Vec<String>) {
    let Ok(source) = std::fs::read_to_string(path) else {
        return;
    };
    let relative = path.strip_prefix(root).unwrap_or(path).display();
    for (index, line) in source.lines().enumerate() {
        for rule in RULES {
            if line.contains(rule.needle) {
                findings.push(format!(
                    "{relative}:{}: `{}`; use {} ({})",
                    index + 1,
                    rule.needle,
                    rule.replacement,
                    rule.reason
                ));
            }
        }
    }
}

pub fn agents_rules() -> String {
    RULES
        .iter()
        .map(|rule| {
            format!(
                "- Never use `{}`; use {} because {}.",
                rule.needle, rule.replacement, rule.reason
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_guidance_is_rendered_from_every_lint_rule() {
        let guidance = agents_rules();
        for rule in RULES {
            assert!(guidance.contains(rule.needle), "{guidance}");
            assert!(guidance.contains(rule.replacement), "{guidance}");
        }
    }
}
