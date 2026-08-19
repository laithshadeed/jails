//! `jails routes` and `jails beans` -- the two "what is my app, actually"
//! commands.
//!
//! Everything else jails does creates something. These two only look. They
//! exist because the questions they answer ("which URL reaches which
//! method", "why is this dependency not injected") are otherwise answered
//! by booting the application and reading a stack trace, which is the slow
//! path this tool exists to remove.
//!
//! Both read the source rather than the running application, which is a
//! deliberate trade: a static read is instant, works on a project that does
//! not currently start (exactly when you need it most), and needs no Docker,
//! no database and no port. What it costs is anything decided at runtime --
//! a path built by concatenation, a bean registered by a conditional
//! auto-configuration. The output says which kind of answer it is giving.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::Result;
use crate::generate::find_project_root;
use crate::java::{self, Target};
use crate::project::json_string;

/// One HTTP route: verb, path, and the method behind it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Route {
    pub path: String,
    pub verb: String,
    pub handler: String,
    pub source: String,
}

/// Spring's mapping annotations and the verb each implies. `RequestMapping`
/// carries its verb in a `method =` attribute instead, and is handled apart.
const VERB_ANNOTATIONS: [(&str, &str); 5] = [
    ("GetMapping", "GET"),
    ("PostMapping", "POST"),
    ("PutMapping", "PUT"),
    ("DeleteMapping", "DELETE"),
    ("PatchMapping", "PATCH"),
];

const CONTROLLER_ANNOTATIONS: [&str; 2] = ["RestController", "Controller"];

pub fn routes(json: bool) -> Result<()> {
    let root = find_project_root()?;
    let found = collect_routes(&root);
    if json {
        println!("{}", routes_json(&found));
        return Ok(());
    }
    if found.is_empty() {
        println!("No routes found under src/main/java.");
        println!(
            "jails reads @GetMapping/@PostMapping/... and `implements HttpHandler` out of the\n\
             source; a path assembled at runtime is invisible to it."
        );
        return Ok(());
    }
    let verb_width = found.iter().map(|r| r.verb.len()).max().unwrap_or(0);
    let path_width = found.iter().map(|r| r.path.len()).max().unwrap_or(0);
    for route in &found {
        println!(
            "{:verb_width$}  {:path_width$}  {}",
            route.verb, route.path, route.handler
        );
    }
    println!();
    println!("{} route(s), read from source.", found.len());
    Ok(())
}

pub(crate) fn collect_routes(root: &Path) -> Vec<Route> {
    let src = root.join("src/main/java");
    let mut found = Vec::new();
    for path in java::source_files(&src) {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let label = relative(root, &path);
        found.extend(file_routes(&source, &label));
    }
    found.sort();
    found.dedup();
    found
}

fn file_routes(source: &str, label: &str) -> Vec<Route> {
    let annotations = java::annotations(source);
    let info = java::type_info(source);
    let type_name = info
        .as_ref()
        .map(|i| i.name.clone())
        .unwrap_or_else(|| "?".to_string());

    // A jails `generate handler` type is not a Spring controller and carries
    // no mapping annotations at all -- its path is a constant and its verbs
    // are fixed by the template (see generate::handler_java).
    if info
        .as_ref()
        .is_some_and(|i| i.supertypes.iter().any(|s| s == "HttpHandler"))
    {
        if let Some(base) = path_constant(source) {
            return ["GET", "POST"]
                .iter()
                .map(|verb| Route {
                    path: if *verb == "GET" {
                        format!("{base}[/{{id}}]")
                    } else {
                        base.clone()
                    },
                    verb: verb.to_string(),
                    handler: format!("{type_name}#handle"),
                    source: label.to_string(),
                })
                .collect();
        }
    }

    let is_controller = annotations
        .iter()
        .any(|a| CONTROLLER_ANNOTATIONS.contains(&a.name.as_str()));
    if !is_controller {
        return Vec::new();
    }

    // A type-level @RequestMapping is a prefix for every method below it.
    let base = annotations
        .iter()
        .find(|a| a.name == "RequestMapping" && matches!(a.target, Target::Type(_)))
        .and_then(|a| java::annotation_string(&a.args))
        .unwrap_or_default();

    let mut found = Vec::new();
    for annotation in &annotations {
        let Target::Method { name: method, .. } = &annotation.target else {
            continue;
        };
        let verb = if let Some((_, verb)) = VERB_ANNOTATIONS
            .iter()
            .find(|(a, _)| *a == annotation.name)
        {
            (*verb).to_string()
        } else if annotation.name == "RequestMapping" {
            request_mapping_verb(&annotation.args)
        } else {
            continue;
        };
        let suffix = java::annotation_string(&annotation.args).unwrap_or_default();
        found.push(Route {
            path: join_path(&base, &suffix),
            verb,
            handler: format!("{type_name}#{method}"),
            source: label.to_string(),
        });
    }
    found
}

/// `@RequestMapping(method = RequestMethod.GET)` -> `GET`. With no method
/// attribute the mapping answers every verb, which Spring writes as such.
fn request_mapping_verb(args: &str) -> String {
    for verb in ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"] {
        if args.contains(&format!("RequestMethod.{verb}")) {
            return verb.to_string();
        }
    }
    "ANY".to_string()
}

/// The value of a `public static final String PATH = "..."` declaration --
/// how `generate handler` records the path it serves.
fn path_constant(source: &str) -> Option<String> {
    let text = java::blanked(source);
    let at = text.find("String PATH")?;
    let eq = text[at..].find('=')? + at;
    // Not annotation_string(): that one reads Spring's `path =`/`value =`
    // attribute grammar and refuses a bare literal that follows an `=`,
    // which is exactly the shape of a constant initialiser.
    java::first_string(source.get(eq..)?)
}

fn join_path(base: &str, suffix: &str) -> String {
    let base = base.trim_end_matches('/');
    let suffix = suffix.trim();
    if suffix.is_empty() {
        return if base.is_empty() { "/".into() } else { base.into() };
    }
    // `@PostMapping(path = "/")` on a type-level-prefixed controller maps
    // the collection itself, not a child of it -- so a suffix that is only
    // separators contributes nothing.
    let suffix = suffix.trim_matches('/');
    if suffix.is_empty() {
        return if base.is_empty() { "/".into() } else { base.into() };
    }
    format!("{base}/{suffix}")
}

fn routes_json(routes: &[Route]) -> String {
    let items: Vec<String> = routes
        .iter()
        .map(|r| {
            format!(
                r#"{{"verb":{},"path":{},"handler":{},"source":{}}}"#,
                json_string(&r.verb),
                json_string(&r.path),
                json_string(&r.handler),
                json_string(&r.source)
            )
        })
        .collect();
    format!(r#"{{"version":1,"routes":[{}]}}"#, items.join(","))
}

/// One registered component and what its constructor asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bean {
    pub stereotype: String,
    pub type_name: String,
    pub source: String,
    /// Types this bean's constructor needs, in declaration order.
    pub needs: Vec<String>,
    /// Interfaces and superclasses it can be injected as.
    pub provides: Vec<String>,
}

/// The annotations that make a type a bean. `@Configuration` is included
/// because its `@Bean` methods are the other way one gets registered.
const STEREOTYPES: [&str; 8] = [
    "Component",
    "Service",
    "Repository",
    "Controller",
    "RestController",
    "Configuration",
    "ControllerAdvice",
    "RestControllerAdvice",
];

pub fn beans(pattern: Option<&str>, json: bool) -> Result<()> {
    let root = find_project_root()?;
    let (found, project_types) = collect_beans(&root);
    let filtered: Vec<&Bean> = found
        .iter()
        .filter(|b| {
            pattern.is_none_or(|p| {
                b.type_name.to_lowercase().contains(&p.to_lowercase())
                    || b.stereotype.to_lowercase().contains(&p.to_lowercase())
            })
        })
        .collect();

    if json {
        println!("{}", beans_json(&filtered));
        return Ok(());
    }
    if filtered.is_empty() {
        println!("No Spring beans found under src/main/java.");
        println!(
            "jails reads @Component/@Service/@Repository/@Controller/@Configuration and\n\
             @Bean methods out of the source; a bean registered by auto-configuration\n\
             is invisible to it."
        );
        return Ok(());
    }

    // Which project types can satisfy an injection point: a bean's own name
    // plus every interface it declares. `RewardRepository` is not itself a
    // bean, but `InMemoryRewardRepository implements RewardRepository` is.
    let supplied = providers(&found);

    let stereotype_width = filtered
        .iter()
        .map(|b| b.stereotype.len() + 1)
        .max()
        .unwrap_or(0);
    let type_width = filtered.iter().map(|b| b.type_name.len()).max().unwrap_or(0);
    let mut unsatisfied = 0usize;
    let mut ambiguous = 0usize;
    for bean in &filtered {
        // `src/main/java/` prefixes every one of these and tells the reader
        // nothing; the package below it is the part that locates the bean.
        let source = bean
            .source
            .strip_prefix("src/main/java/")
            .unwrap_or(&bean.source);
        println!(
            "{:stereotype_width$}  {:type_width$}  {source}",
            format!("@{}", bean.stereotype),
            bean.type_name,
        );
        for need in &bean.needs {
            let candidates = supplied.get(need.as_str()).map(Vec::as_slice).unwrap_or(&[]);
            let note = match candidates.len() {
                1 => "ok".to_string(),
                // Spring refuses to choose between candidates, so two is as
                // broken as zero -- and it fails at startup with a different
                // message and the opposite fix.
                n if n > 1 => {
                    ambiguous += 1;
                    format!(
                        "AMBIGUOUS -- {n} beans qualify ({}); mark one @Primary",
                        candidates.join(", ")
                    )
                }
                _ if project_types.contains(need.as_str()) => {
                    unsatisfied += 1;
                    "NO BEAN -- this project declares the type but registers no bean for it"
                        .to_string()
                }
                _ => "external -- the framework or a library is expected to supply it".to_string(),
            };
            println!("{:stereotype_width$}    needs {need}  ({note})", "");
        }
    }

    println!();
    println!("{} bean(s), read from source.", filtered.len());
    if unsatisfied > 0 {
        println!(
            "{unsatisfied} dependency/dependencies name a type this project declares but never\n\
             registers -- the usual cause of \"required a bean of type ... that could not be\n\
             found\" at startup. Annotate the implementation (@Service/@Repository/@Component)\n\
             or add an @Bean method for it."
        );
    }
    if ambiguous > 0 {
        println!(
            "{ambiguous} dependency/dependencies have more than one candidate -- \"required a\n\
             single bean, but N were found\" at startup. Mark the one the application should use\n\
             @Primary, or drop the stereotype from the other (an in-memory fake usually wants to\n\
             be constructed by tests, not registered in the context)."
        );
    }
    Ok(())
}

/// Every bean in the project, plus the set of type names the project
/// declares at all (needed to tell "your own type, unregistered" apart from
/// "something Spring provides").
pub(crate) fn collect_beans(root: &Path) -> (Vec<Bean>, BTreeSet<String>) {
    let src = root.join("src/main/java");
    let mut beans = Vec::new();
    let mut project_types = BTreeSet::new();
    for path in java::source_files(&src) {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let label = relative(root, &path);
        if let Some(info) = java::type_info(&source) {
            project_types.insert(info.name.clone());
        }
        beans.extend(file_beans(&source, &label));
    }
    beans.sort_by(|a, b| a.type_name.cmp(&b.type_name));
    (beans, project_types)
}

/// Which beans can satisfy each injectable type: a bean supplies its own
/// type and every interface it declares. `RewardRepository` is not itself a
/// bean, but `InMemoryRewardRepository implements RewardRepository` is -- and
/// so is `JdbcRewardRepository`, which is exactly how a project ends up with
/// two candidates for one injection point.
pub(crate) fn providers(beans: &[Bean]) -> BTreeMap<String, Vec<String>> {
    let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for bean in beans {
        for provided in std::iter::once(&bean.type_name).chain(bean.provides.iter()) {
            let candidates = index.entry(provided.clone()).or_default();
            if !candidates.contains(&bean.type_name) {
                candidates.push(bean.type_name.clone());
            }
        }
    }
    index
}

fn file_beans(source: &str, label: &str) -> Vec<Bean> {
    let annotations = java::annotations(source);
    let Some(info) = java::type_info(source) else {
        return Vec::new();
    };
    let mut found = Vec::new();

    if let Some(stereotype) = annotations.iter().find(|a| {
        matches!(&a.target, Target::Type(name) if *name == info.name)
            && STEREOTYPES.contains(&a.name.as_str())
    }) {
        found.push(Bean {
            stereotype: stereotype.name.clone(),
            type_name: info.name.clone(),
            source: label.to_string(),
            needs: info
                .constructor_params
                .iter()
                .map(|p| p.type_name.clone())
                .collect(),
            provides: info.supertypes.clone(),
        });
    }

    // `@Bean` factory methods register their *return* type, not the
    // configuration class -- a JdbcTemplate produced by AppConfig is a
    // JdbcTemplate bean, and looking for the class name would miss it.
    for annotation in &annotations {
        if annotation.name != "Bean" {
            continue;
        }
        let Target::Method { name, returns } = &annotation.target else {
            continue;
        };
        let returns = java::simple_name(returns);
        if returns.is_empty() {
            continue;
        }
        found.push(Bean {
            stereotype: "Bean".to_string(),
            type_name: returns,
            source: format!("{label} ({}#{name})", info.name),
            needs: Vec::new(),
            provides: Vec::new(),
        });
    }
    found
}

fn beans_json(beans: &[&Bean]) -> String {
    let items: Vec<String> = beans
        .iter()
        .map(|b| {
            let list = |values: &[String]| {
                values
                    .iter()
                    .map(|v| json_string(v))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            format!(
                r#"{{"stereotype":{},"type":{},"source":{},"needs":[{}],"provides":[{}]}}"#,
                json_string(&b.stereotype),
                json_string(&b.type_name),
                json_string(&b.source),
                list(&b.needs),
                list(&b.provides)
            )
        })
        .collect();
    format!(r#"{{"version":1,"beans":[{}]}}"#, items.join(","))
}

/// A path relative to the project root when possible -- absolute paths make
/// the output unusable in a narrow terminal split.
pub(crate) fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTROLLER: &str = r#"
package com.example.api;
@RestController
@RequestMapping("/rewards")
public final class RewardController {
    @GetMapping
    public List<Reward> list() { return null; }
    @GetMapping("/{id}")
    public Reward byId(@PathVariable String id) { return null; }
    @PostMapping(path = "/", consumes = "application/json")
    public Reward create(@RequestBody Reward reward) { return null; }
}"#;

    #[test]
    fn routes_join_the_type_level_prefix() {
        let found = file_routes(CONTROLLER, "api/RewardController.java");
        let rendered: Vec<String> = found
            .iter()
            .map(|r| format!("{} {} {}", r.verb, r.path, r.handler))
            .collect();
        assert!(
            rendered.contains(&"GET /rewards RewardController#list".to_string()),
            "{rendered:?}"
        );
        assert!(
            rendered.contains(&"GET /rewards/{id} RewardController#byId".to_string()),
            "{rendered:?}"
        );
        assert!(
            rendered.contains(&"POST /rewards RewardController#create".to_string()),
            "{rendered:?}"
        );
    }

    #[test]
    fn a_type_without_a_controller_annotation_has_no_routes() {
        let src = "package p;\npublic final class Helper {\n  public void get() {}\n}";
        assert!(file_routes(src, "Helper.java").is_empty());
    }

    #[test]
    fn a_generated_handler_reports_its_path_constant() {
        let src = r#"
package com.example.api;
public final class WorkItemHandler implements HttpHandler {
    public static final String PATH = "/work-items";
    public void handle(HttpExchange exchange) {}
}"#;
        let found = file_routes(src, "api/WorkItemHandler.java");
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().all(|r| r.path.starts_with("/work-items")), "{found:?}");
        assert!(found.iter().any(|r| r.verb == "POST"), "{found:?}");
    }

    #[test]
    fn request_mapping_reads_its_verb_attribute() {
        assert_eq!(request_mapping_verb("method = RequestMethod.PUT"), "PUT");
        assert_eq!(request_mapping_verb(r#""/x""#), "ANY");
    }

    #[test]
    fn join_path_keeps_exactly_one_separator() {
        assert_eq!(join_path("/rewards", "/{id}"), "/rewards/{id}");
        assert_eq!(join_path("/rewards/", "{id}"), "/rewards/{id}");
        assert_eq!(join_path("/rewards", ""), "/rewards");
        assert_eq!(join_path("/rewards", "/"), "/rewards");
        assert_eq!(join_path("", ""), "/");
    }

    #[test]
    fn beans_record_their_constructor_dependencies() {
        let src = r#"
package com.example.application;
@Service
public final class RewardHistoryService {
    public RewardHistoryService(RewardRepository repository, Clock clock) {}
}"#;
        let found = file_beans(src, "application/RewardHistoryService.java");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].stereotype, "Service");
        assert_eq!(found[0].needs, vec!["RewardRepository", "Clock"]);
    }

    #[test]
    fn a_bean_method_registers_its_return_type() {
        let src = r#"
package p;
@Configuration
public class AppConfig {
    @Bean
    public ObjectMapper objectMapper() { return null; }
}"#;
        let found = file_beans(src, "AppConfig.java");
        let names: Vec<&str> = found.iter().map(|b| b.type_name.as_str()).collect();
        assert!(names.contains(&"AppConfig"), "{names:?}");
        assert!(names.contains(&"ObjectMapper"), "{names:?}");
    }

    #[test]
    fn an_implementation_supplies_the_interface_it_declares() {
        let src = r#"
package p;
@Repository
public final class InMemoryRewardRepository implements RewardRepository {
}"#;
        let found = file_beans(src, "InMemoryRewardRepository.java");
        assert_eq!(found[0].provides, vec!["RewardRepository"]);
    }
}
