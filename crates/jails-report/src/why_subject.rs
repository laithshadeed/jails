//! Read-only, source-bounded `jails why <subject> <name>` reports.

use std::path::PathBuf;

use jails_project::model::Project;
use jails_project::{inspect, query_workspace};
use jails_support::Result;

const LIMITATION: &str = "profiles, conditions, post-processors, proxies, programmatic beans, and runtime database state are not evaluated";

struct Node {
    id: String,
    kind: &'static str,
    label: String,
    evidence: &'static str,
}

struct Edge {
    from: String,
    to: String,
    relation: &'static str,
}

struct Report {
    subject: String,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    fixes: Vec<String>,
}

impl Report {
    fn new(subject: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            nodes: vec![Node {
                id: "subject".into(),
                kind: "subject",
                label: label.into(),
                evidence: "static-inference",
            }],
            edges: Vec::new(),
            fixes: Vec::new(),
        }
    }

    fn node(
        &mut self,
        kind: &'static str,
        label: impl Into<String>,
        evidence: &'static str,
        relation: &'static str,
    ) {
        let id = format!("node-{}", self.nodes.len());
        self.edges.push(Edge {
            from: "subject".into(),
            to: id.clone(),
            relation,
        });
        self.nodes.push(Node {
            id,
            kind,
            label: label.into(),
            evidence,
        });
    }

    fn print(&self, json: bool) {
        if json {
            println!("{}", self.json());
            return;
        }
        println!("{}", self.nodes[0].label);
        println!("evidence: static-inference");
        for edge in &self.edges {
            if let Some(node) = self.nodes.iter().find(|node| node.id == edge.to) {
                println!("└─ {}: {} [{}]", edge.relation, node.label, node.evidence);
            }
        }
        println!("limitation: {LIMITATION}");
        if !self.fixes.is_empty() {
            println!("fix:");
            for fix in &self.fixes {
                println!("  {fix}");
            }
        }
    }

    fn json(&self) -> String {
        let nodes = self
            .nodes
            .iter()
            .map(|node| {
                format!(
                    "{{\"id\":{},\"kind\":{},\"label\":{},\"evidence_kind\":{}}}",
                    crate::json::string(&node.id),
                    crate::json::string(node.kind),
                    crate::json::string(&node.label),
                    crate::json::string(node.evidence)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let edges = self
            .edges
            .iter()
            .map(|edge| {
                format!(
                    "{{\"from\":{},\"to\":{},\"relation\":{}}}",
                    crate::json::string(&edge.from),
                    crate::json::string(&edge.to),
                    crate::json::string(edge.relation)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let fixes = self
            .fixes
            .iter()
            .map(|fix| crate::json::string(fix))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"schema_version\":3,\"subject\":{},\"evidence\":{{\"kind\":\"static-inference\",\"limitations\":[{}]}},\"cause_graph\":{{\"nodes\":[{}],\"edges\":[{}]}},\"fixes\":[{}]}}",
            crate::json::string(&self.subject),
            crate::json::string(LIMITATION),
            nodes,
            edges,
            fixes
        )
    }
}

pub fn report(kind: &str, name: &str, json: bool) -> Result<()> {
    let project = Project::discover()?;
    match kind {
        "bean" => bean(&project, name, json),
        "migration" => migration(&project, name, json),
        "query" => query(&project, name, json),
        _ => Err(format!(
            "unknown why subject `{kind}`.\n       fix: use `bean`, `migration`, or `query`, or pass one log-file path."
        )
        .into()),
    }
}

fn bean(project: &Project, name: &str, json: bool) -> Result<()> {
    let (beans, project_types) = inspect::collect_beans(project.root());
    let providers = inspect::providers(&beans);
    let mut report = Report::new(format!("bean:{name}"), format!("Bean {name}"));
    let supplied = providers.get(name).cloned().unwrap_or_default();
    for bean in beans
        .iter()
        .filter(|bean| bean.type_name == name || supplied.contains(&bean.type_name))
    {
        report.node(
            "declaration",
            format!(
                "{} is @{} at {}:{}",
                bean.type_name, bean.stereotype, bean.source, bean.line
            ),
            "static-inference",
            "declared-by",
        );
        for need in &bean.needs {
            let candidates = providers.get(need).cloned().unwrap_or_default();
            let (evidence, label) = if candidates.is_empty() && project_types.contains(need) {
                report.fixes.push(format!(
                    "register exactly one production implementation of {need} with a stereotype or @Bean"
                ));
                (
                    "hypothesis",
                    format!("needs {need}; no source-visible provider is registered"),
                )
            } else if candidates.is_empty() {
                (
                    "hypothesis",
                    format!("needs {need}; a framework or library may provide it at runtime"),
                )
            } else {
                (
                    "static-inference",
                    format!(
                        "needs {need}; source-visible provider(s): {}",
                        candidates.join(", ")
                    ),
                )
            };
            report.node("dependency", label, evidence, "needs");
        }
    }
    for consumer in beans
        .iter()
        .filter(|bean| bean.needs.iter().any(|need| need == name))
    {
        report.node(
            "consumer",
            format!(
                "{} injects {name} at {}:{}",
                consumer.type_name, consumer.source, consumer.line
            ),
            "static-inference",
            "consumed-by",
        );
    }
    if report.nodes.len() == 1 {
        let label = if project_types.contains(name) {
            format!("{name} exists in project source but is not a source-visible Spring bean")
        } else {
            format!("no source-visible project type or Spring bean named {name} was found")
        };
        report.node(
            "missing-declaration",
            label,
            "hypothesis",
            "unresolved-because",
        );
        report.fixes.push(format!(
            "inspect `jails beans {name}` and register a production bean explicitly"
        ));
    }
    report.print(json);
    Ok(())
}

fn migration(project: &Project, version: &str, json: bool) -> Result<()> {
    let wanted = version.trim_start_matches(['V', 'v']).parse::<u64>().map_err(|_| {
        format!("migration version `{version}` is not numeric.\n       fix: pass a value like `V014`.")
    })?;
    let path = migration_path(project, wanted)?.ok_or_else(|| {
        format!(
            "no migration uses version V{wanted:03}.\n       fix: inspect src/main/resources/db/migration."
        )
    })?;
    let relative = path
        .strip_prefix(project.root())
        .unwrap_or(&path)
        .to_string_lossy();
    let mut report = Report::new(
        format!("migration:V{wanted:03}"),
        format!("Migration V{wanted:03}"),
    );
    report.node(
        "migration-source",
        relative.to_string(),
        "static-inference",
        "declared-by",
    );
    match query_workspace::migration_schema(project, None) {
        Ok(snapshot) => report.node(
            "schema-projection",
            format!(
                "ordered migrations parse to {} normalized schema object(s)",
                snapshot.catalog.objects.len()
            ),
            "static-inference",
            "projects-to",
        ),
        Err(error) => report.node(
            "parse-failure",
            error.to_string(),
            "static-inference",
            "fails-because",
        ),
    }
    for finding in query_workspace::migration_lint(project, None)? {
        if finding.path.as_str() == relative {
            report.node(
                "migration-risk",
                format!(
                    "statement {}: {} ({:?})",
                    finding.statement, finding.summary, finding.risks
                ),
                "static-inference",
                "has-risk",
            );
        }
    }
    report.print(json);
    Ok(())
}

fn migration_path(project: &Project, wanted: u64) -> Result<Option<PathBuf>> {
    let directory = project.root().join("src/main/resources/db/migration");
    let mut matches = Vec::new();
    for entry in std::fs::read_dir(&directory).map_err(|error| {
        format!(
            "failed to read migration directory {}: {error}",
            directory.display()
        )
    })? {
        let path = entry
            .map_err(|error| format!("failed to read migration entry: {error}"))?
            .path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let parsed = name
            .strip_prefix('V')
            .and_then(|rest| rest.split_once("__"))
            .and_then(|(number, _)| number.parse::<u64>().ok());
        if parsed == Some(wanted) {
            matches.push(path);
        }
    }
    matches.sort();
    Ok(matches.into_iter().next())
}

fn query(project: &Project, name: &str, json: bool) -> Result<()> {
    let mut report = Report::new(format!("query:{name}"), format!("Query {name}"));
    match query_workspace::check_offline(project, None, Some(name)) {
        Ok(checked) => {
            for query in checked {
                report.node(
                    "query-source",
                    format!(
                        "{}::{} at {}",
                        query.source.id.slice.as_str(),
                        query.source.id.name.as_str(),
                        query.source.path.as_str()
                    ),
                    "static-inference",
                    "declared-by",
                );
                report.node(
                    "query-contract",
                    format!(
                        "verified-offline: {} parameter(s), {} column(s), query digest {}",
                        query.contract.parameters.len(),
                        query.contract.columns.len(),
                        query.contract.query_digest
                    ),
                    "static-inference",
                    "verified-by",
                );
                report.node(
                    "input-closure",
                    format!(
                        "{} complete input(s), including manifest, query, and ordered migrations",
                        query.inputs.len()
                    ),
                    "static-inference",
                    "depends-on",
                );
            }
        }
        Err(error) => {
            report.node(
                "query-diagnostic",
                error.to_string(),
                "hypothesis",
                "unresolved-because",
            );
            report.fixes.push(format!(
                "run `jails sql check {name}` for the full offline diagnostic"
            ));
        }
    }
    report.print(json);
    Ok(())
}
