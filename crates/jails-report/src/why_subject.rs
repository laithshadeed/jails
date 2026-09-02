//! Read-only, source-bounded `jails why <subject> <name>` reports.

use jails_project::inspect;
use jails_project::model::Project;
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
