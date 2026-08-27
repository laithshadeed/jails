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

use crate::java::Target;
use crate::spec::find_project_root;
use jails_support::Result;

mod socket;

/// One HTTP route: verb, path, and the method behind it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Route {
    pub path: String,
    pub verb: String,
    pub handler: String,
    pub source: String,
    pub line: usize,
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

const STATIC_EVIDENCE_KIND: &str = "static-inference";
const STATIC_EVIDENCE_LIMITATION: &str = "profiles, conditions, post-processors, proxies, and programmatic runtime registrations are not evaluated";

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
    println!("evidence: {STATIC_EVIDENCE_KIND}");
    println!("limitation: {STATIC_EVIDENCE_LIMITATION}");
    Ok(())
}

pub fn collect_routes(root: &Path) -> Vec<Route> {
    let src = root.join("src/main/java");
    let mut found = Vec::new();
    for path in crate::java::source_files(&src) {
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
    let annotations = crate::java::annotations(source);
    let info = crate::java::type_info(source);
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
        && let Some(base) = path_constant(source)
    {
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
                line: line_of(source, "handle("),
            })
            .collect();
    }

    // A WebSocket endpoint is registered programmatically, so it carries no
    // mapping annotation and every scanner that looks for one misses it --
    // including this one, over a registration `jails g socket` had just
    // written (`bugs.md` B56). Two things jails emits and cannot see is worse
    // than a gap: the reader has no way to tell an unlisted route from an
    // absent one.
    //
    // Read the same way the `HttpHandler` arm above reads its constant: off
    // the *blanked* copy, so an `addHandler(` inside the Javadoc example this
    // template carries is not a registration, and sliced out of the original,
    // because blanking replaces the quotes too.
    if info
        .as_ref()
        .is_some_and(|i| i.supertypes.iter().any(|s| s == "WebSocketConfigurer"))
    {
        let routes = socket::registered_routes(source, &type_name, label, info.as_ref());
        if !routes.is_empty() {
            return routes;
        }
    }

    let is_controller = annotations
        .iter()
        .any(|a| CONTROLLER_ANNOTATIONS.contains(&a.name.as_str()));
    if !is_controller {
        return Vec::new();
    }

    // A type-level @RequestMapping is a prefix for every method below it.
    //
    // Its argument is not always a literal: a controller that keeps its path
    // in a constant writes `@RequestMapping(FooController.PATH)`, which is
    // better code and reads as an empty prefix to a scanner that only looks
    // for quotes. `jails g scaffold` generates exactly that shape, so falling
    // back to the constant's own value is not an edge case.
    let base = annotations
        .iter()
        .find(|a| a.name == "RequestMapping" && matches!(a.target, Target::Type(_)))
        .and_then(|a| {
            crate::java::annotation_string(&a.args).or_else(|| {
                a.args
                    .contains("PATH")
                    .then(|| path_constant(source))
                    .flatten()
            })
        })
        .unwrap_or_default();

    let mut found = Vec::new();
    for annotation in &annotations {
        let Target::Method { name: method, .. } = &annotation.target else {
            continue;
        };
        let verb =
            if let Some((_, verb)) = VERB_ANNOTATIONS.iter().find(|(a, _)| *a == annotation.name) {
                (*verb).to_string()
            } else if annotation.name == "RequestMapping" {
                request_mapping_verb(&annotation.args)
            } else {
                continue;
            };
        let suffix = crate::java::annotation_string(&annotation.args).unwrap_or_default();
        found.push(Route {
            path: join_path(&base, &suffix),
            verb,
            handler: format!("{type_name}#{method}"),
            source: label.to_string(),
            line: annotation.line,
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
    let text = crate::java::blanked(source);
    let at = text.find("String PATH")?;
    let eq = text[at..].find('=')? + at;
    // Not annotation_string(): that one reads Spring's `path =`/`value =`
    // attribute grammar and refuses a bare literal that follows an `=`,
    // which is exactly the shape of a constant initialiser.
    crate::java::first_string(source.get(eq..)?)
}

fn join_path(base: &str, suffix: &str) -> String {
    let base = base.trim_end_matches('/');
    let suffix = suffix.trim();
    if suffix.is_empty() {
        return if base.is_empty() {
            "/".into()
        } else {
            base.into()
        };
    }
    // `@PostMapping(path = "/")` on a type-level-prefixed controller maps
    // the collection itself, not a child of it -- so a suffix that is only
    // separators contributes nothing.
    let suffix = suffix.trim_matches('/');
    if suffix.is_empty() {
        return if base.is_empty() {
            "/".into()
        } else {
            base.into()
        };
    }
    format!("{base}/{suffix}")
}

fn routes_json(routes: &[Route]) -> String {
    let items: Vec<String> = routes
        .iter()
        .map(|r| {
            format!(
                r#"{{"verb":{},"path":{},"handler":{},"source":{},"line":{}}}"#,
                crate::json::string(&r.verb),
                crate::json::string(&r.path),
                crate::json::string(&r.handler),
                crate::json::string(&r.source),
                r.line
            )
        })
        .collect();
    format!(
        r#"{{"schema_version":3,"evidence":{{"kind":{},"limitations":[{}]}},"routes":[{}]}}"#,
        crate::json::string(STATIC_EVIDENCE_KIND),
        crate::json::string(STATIC_EVIDENCE_LIMITATION),
        items.join(",")
    )
}

/// One registered component and what its constructor asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bean {
    pub stereotype: String,
    pub type_name: String,
    pub source: String,
    pub line: usize,
    /// Types this bean's constructor needs, in declaration order.
    pub needs: Vec<String>,
    /// Interfaces and superclasses it can be injected as.
    pub provides: Vec<String>,
    /// Carries `@Primary`, which is how a project with two candidates for
    /// one injection point tells Spring which to prefer. Without tracking
    /// it, jails would report a resolved ambiguity as broken.
    pub primary: bool,
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
    let type_width = filtered
        .iter()
        .map(|b| b.type_name.len())
        .max()
        .unwrap_or(0);
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
            let candidates = supplied
                .get(need.as_str())
                .map(Vec::as_slice)
                .unwrap_or(&[]);
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
    println!("evidence: {STATIC_EVIDENCE_KIND}");
    println!("limitation: {STATIC_EVIDENCE_LIMITATION}");
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
pub fn collect_beans(root: &Path) -> (Vec<Bean>, BTreeSet<String>) {
    let src = root.join("src/main/java");
    let mut beans = Vec::new();
    let mut project_types = BTreeSet::new();
    for path in crate::java::source_files(&src) {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let label = relative(root, &path);
        if let Some(info) = crate::java::type_info(&source) {
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
pub fn providers(beans: &[Bean]) -> BTreeMap<String, Vec<String>> {
    let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut primary: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for bean in beans {
        for provided in std::iter::once(&bean.type_name).chain(bean.provides.iter()) {
            let candidates = index.entry(provided.clone()).or_default();
            if !candidates.contains(&bean.type_name) {
                candidates.push(bean.type_name.clone());
            }
            if bean.primary {
                primary
                    .entry(provided.clone())
                    .or_default()
                    .push(bean.type_name.clone());
            }
        }
    }
    // Exactly one @Primary among several candidates is not an ambiguity --
    // it is the answer, and reporting it as broken would train the reader to
    // ignore the check. Two @Primary beans for one type is still ambiguous,
    // so only a single winner collapses the list.
    for (provided, winners) in primary {
        if winners.len() == 1 {
            index.insert(provided, winners);
        }
    }
    index
}

fn file_beans(source: &str, label: &str) -> Vec<Bean> {
    let annotations = crate::java::annotations(source);
    let Some(info) = crate::java::type_info(source) else {
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
            line: stereotype.line,
            needs: info
                .constructor_params
                .iter()
                .map(|p| p.type_name.clone())
                .collect(),
            provides: info.supertypes.clone(),
            primary: annotations.iter().any(|a| a.name == "Primary"),
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
        let returns = crate::java::simple_name(returns);
        if returns.is_empty() {
            continue;
        }
        found.push(Bean {
            stereotype: "Bean".to_string(),
            type_name: returns,
            source: format!("{label} ({}#{name})", info.name),
            line: annotation.line,
            needs: Vec::new(),
            provides: Vec::new(),
            primary: matches!(&annotation.target, Target::Method { name: m, .. }
                if annotations.iter().any(|a| a.name == "Primary"
                    && matches!(&a.target, Target::Method { name: other, .. } if other == m))),
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
                    .map(|v| crate::json::string(v))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            format!(
                r#"{{"stereotype":{},"type":{},"source":{},"line":{},"needs":[{}],"provides":[{}]}}"#,
                crate::json::string(&b.stereotype),
                crate::json::string(&b.type_name),
                crate::json::string(&b.source),
                b.line,
                list(&b.needs),
                list(&b.provides)
            )
        })
        .collect();
    format!(
        r#"{{"schema_version":3,"evidence":{{"kind":{},"limitations":[{}]}},"beans":[{}]}}"#,
        crate::json::string(STATIC_EVIDENCE_KIND),
        crate::json::string(STATIC_EVIDENCE_LIMITATION),
        items.join(",")
    )
}

fn line_of(source: &str, needle: &str) -> usize {
    source
        .find(needle)
        .map(|at| source[..at].bytes().filter(|byte| *byte == b'\n').count() + 1)
        .unwrap_or(1)
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
    fn a_websocket_registration_is_a_route() {
        // `bugs.md` B56: `jails g socket Chat` writes this file and `jails
        // routes` then reported "No routes found under src/main/java". A
        // route jails emitted and cannot see is worse than a gap -- the
        // reader cannot tell an unlisted route from an absent one.
        let source = r#"
package com.example.web;

/**
 * Where the handler answers.
 *
 * <p>Example: {@code registry.addHandler(other, "/ws/from-the-javadoc");}
 */
@Configuration
@EnableWebSocket
public class ChatSocketConfig implements WebSocketConfigurer {

    private final ChatSocketHandler handler;

    public ChatSocketConfig(ChatSocketHandler handler) {
        this.handler = handler;
    }

    @Override
    public void registerWebSocketHandlers(WebSocketHandlerRegistry registry) {
        registry.addHandler(handler, "/ws/chat", "/ws/chat/{email}");
    }
}
"#;
        let routes = file_routes(source, "web/ChatSocketConfig.java");
        let seen: Vec<(&str, &str, &str)> = routes
            .iter()
            .map(|route| {
                (
                    route.verb.as_str(),
                    route.path.as_str(),
                    route.handler.as_str(),
                )
            })
            .collect();
        // Both registered paths, the handler resolved to the class that
        // answers rather than the field that holds it -- and nothing from the
        // Javadoc example, which `blanked()` is what keeps out.
        assert_eq!(
            seen,
            vec![
                ("WS", "/ws/chat", "ChatSocketHandler"),
                ("WS", "/ws/chat/{email}", "ChatSocketHandler"),
            ]
        );
    }

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
    fn a_type_level_mapping_can_be_a_constant_rather_than_a_literal() {
        // What `g scaffold` generates: the path lives in one constant that
        // the controller, its test and any link builder all reference.
        let src = r#"
package com.example.web;
@RestController
@RequestMapping(NoteController.PATH)
public final class NoteController {
    public static final String PATH = "/notes";
    @GetMapping("/{id}")
    public Note byId(String id) { return null; }
}"#;
        let found = file_routes(src, "web/NoteController.java");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].path, "/notes/{id}", "{found:?}");
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
        assert!(
            found.iter().all(|r| r.path.starts_with("/work-items")),
            "{found:?}"
        );
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

// ---------------------------------------------------------------------------
// `jails stats` and `jails notes` -- Rails' two oldest reading commands.
// ---------------------------------------------------------------------------

/// One row of the statistics table.
struct LayerStats {
    label: &'static str,
    files: usize,
    lines: usize,
    /// Lines that are neither blank nor a comment.
    code: usize,
}

// The layer list and its headings live in `config`, which is also what
// applies a project's renames. This module used to keep its own copy, so
// `stats` reported against jails' *default* package names: a project with
// `adapters = "persistence"` had its adapters counted as "Other", and `cli`
// and `messaging` were never counted at all because the copy was missing
// them. One list, read through the project's config.

pub fn stats(json: bool) -> Result<()> {
    let root = find_project_root()?;
    // Through the project's own config, so a renamed layer is counted under
    // the name the project actually uses.
    let layers = crate::config::Config::load(&root)?.layers();
    let main = collect_stats(&root.join("src/main/java"), &layers);
    let test = collect_stats(&root.join("src/test/java"), &layers);

    if json {
        return stats_json(&main, &test);
    }

    let rows: Vec<&LayerStats> = main.iter().filter(|r| r.files > 0).collect();
    if rows.is_empty() {
        println!("No Java sources under src/main/java.");
        return Ok(());
    }

    let width = rows.iter().map(|r| r.label.len()).max().unwrap_or(0).max(5);
    println!(
        "{:width$}  {:>6}  {:>7}  {:>7}",
        "Layer", "files", "lines", "code"
    );
    println!("{}", "-".repeat(width + 26));
    for row in &rows {
        println!(
            "{:width$}  {:>6}  {:>7}  {:>7}",
            row.label, row.files, row.lines, row.code
        );
    }

    let code: usize = main.iter().map(|r| r.code).sum();
    let test_code: usize = test.iter().map(|r| r.code).sum();
    let files: usize = main.iter().map(|r| r.files).sum();
    let test_files: usize = test.iter().map(|r| r.files).sum();
    println!("{}", "-".repeat(width + 26));
    println!("{:width$}  {:>6}  {:>7}  {:>7}", "Main", files, "", code);
    println!(
        "{:width$}  {:>6}  {:>7}  {:>7}",
        "Test", test_files, "", test_code
    );
    println!();
    // The ratio, not a verdict on it. What counts as healthy depends on what
    // the code does, and a tool asserting a target would only be ignored.
    if code > 0 {
        println!(
            "Test code to application code: {:.2}",
            test_code as f64 / code as f64
        );
    }
    Ok(())
}

/// The same counts, as data.
///
/// Every layer is emitted, including the empty ones -- the human table hides
/// those because a screen of zeroes is noise, but a consumer diffing two runs
/// needs a layer that went to zero to still be there. That is the one place
/// the two renderings legitimately differ, and it is worth saying out loud.
fn stats_json(main: &[LayerStats], test: &[LayerStats]) -> Result<()> {
    let render = |rows: &[LayerStats]| {
        rows.iter()
            .map(|row| {
                format!(
                    "      {{\"layer\": {}, \"files\": {}, \"lines\": {}, \"code\": {}}}",
                    crate::json::string(row.label),
                    row.files,
                    row.lines,
                    row.code
                )
            })
            .collect::<Vec<_>>()
            .join(",\n")
    };
    let code: usize = main.iter().map(|r| r.code).sum();
    let test_code: usize = test.iter().map(|r| r.code).sum();
    println!(
        "{{\n  \"schema_version\": 1,\n  \"main\": [\n{}\n  ],\n  \"test\": [\n{}\n  ],\n  \
         \"totals\": {{\"code\": {code}, \"test_code\": {test_code}, \"files\": {}, \
         \"test_files\": {}}}\n}}",
        render(main),
        render(test),
        main.iter().map(|r| r.files).sum::<usize>(),
        test.iter().map(|r| r.files).sum::<usize>()
    );
    Ok(())
}

fn collect_stats(src: &Path, layers: &[(String, &'static str)]) -> Vec<LayerStats> {
    let mut rows: Vec<LayerStats> = layers
        .iter()
        .map(|(_, label)| LayerStats {
            label,
            files: 0,
            lines: 0,
            code: 0,
        })
        .collect();
    rows.push(LayerStats {
        label: "Other",
        files: 0,
        lines: 0,
        code: 0,
    });

    for path in crate::java::source_files(src) {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let row = layer_index(&path, layers);
        let lines = source.lines().count();
        // "Code" excludes blanks and comment lines. Javadoc is most of a
        // jails-generated file, and counting it would make every layer look
        // three times its size.
        let code = source
            .lines()
            .map(str::trim)
            .filter(|line| {
                !line.is_empty()
                    && !line.starts_with("//")
                    && !line.starts_with("/*")
                    && !line.starts_with('*')
            })
            .count();
        rows[row].files += 1;
        rows[row].lines += lines;
        rows[row].code += code;
    }
    rows
}

/// Which row a file belongs to: the first layer subpackage its path contains,
/// or the catch-all. Matched on whole path *segments* so `com/example/webshop`
/// does not read as the `web` layer.
///
/// A configured layer may be a nested package (`adapters = "infra.jdbc"`), so
/// the match is on the segments in sequence rather than on one name.
fn layer_index(path: &Path, layers: &[(String, &'static str)]) -> usize {
    let segments: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    layers
        .iter()
        .position(|(package, _)| contains_package(&segments, package))
        .unwrap_or(layers.len())
}

/// Whether `package` ("adapters", or "infra.jdbc") appears as consecutive
/// segments of the path. An empty package is the base package, which every
/// file is under -- it would swallow the whole tree, so it never matches.
fn contains_package(segments: &[String], package: &str) -> bool {
    let wanted: Vec<&str> = package.split('.').filter(|p| !p.is_empty()).collect();
    if wanted.is_empty() {
        return false;
    }
    segments
        .windows(wanted.len())
        .any(|window| window.iter().zip(&wanted).all(|(seg, want)| seg == want))
}

/// A `TODO`/`FIXME`-style marker and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Note {
    pub tag: String,
    pub file: String,
    pub line: usize,
    pub text: String,
}

/// The tags worth surfacing. Deliberately short: a list long enough to catch
/// everything catches nothing, because the output stops being read.
const NOTE_TAGS: [&str; 4] = ["TODO", "FIXME", "HACK", "XXX"];

pub fn notes(tag: Option<&str>, json: bool) -> Result<()> {
    let root = find_project_root()?;
    let found = collect_notes(&root, tag);

    if json {
        let rows: Vec<String> = found
            .iter()
            .map(|note| {
                format!(
                    "    {{\"tag\": {}, \"file\": {}, \"line\": {}, \"text\": {}}}",
                    crate::json::string(&note.tag),
                    crate::json::string(&note.file),
                    note.line,
                    crate::json::string(&note.text)
                )
            })
            .collect();
        // `file` and `line` are the shape a quickfix list wants, which is the
        // whole reason this has a JSON form.
        println!(
            "{{\n  \"schema_version\": 1,\n  \"notes\": [\n{}\n  ]\n}}",
            rows.join(",\n")
        );
        return Ok(());
    }

    if found.is_empty() {
        match tag {
            Some(tag) => println!("No {tag} notes."),
            None => println!("No {} notes.", NOTE_TAGS.join("/")),
        }
        return Ok(());
    }
    let width = found.iter().map(|n| n.tag.len()).max().unwrap_or(0);
    for note in &found {
        println!("{:width$}  {}:{}", note.tag, note.file, note.line);
        println!("{:width$}    {}", "", note.text);
    }
    println!();
    println!("{} note(s).", found.len());
    Ok(())
}

pub(crate) fn collect_notes(root: &Path, only: Option<&str>) -> Vec<Note> {
    let mut found = Vec::new();
    for dir in ["src/main/java", "src/test/java"] {
        for path in crate::java::source_files(&root.join(dir)) {
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            let label = relative(root, &path);
            found.extend(file_notes(&source, &label, only));
        }
    }
    found
}

fn file_notes(source: &str, label: &str, only: Option<&str>) -> Vec<Note> {
    // Scanned against the blanked copy so a tag inside a string literal --
    // `"TODO"` in a message, or a SQL text block -- is not reported as work
    // to do. Offsets are shared, so the text still comes from the original.
    // Literals blanked, comments kept: a note lives in a comment, and the
    // word appearing inside a string ("TODO" in a message, or in a SQL text
    // block) is not work anyone signed up for.
    let scanned = crate::java::without_literals(source);
    let mut found = Vec::new();
    for (index, (line, raw)) in scanned.lines().zip(source.lines()).enumerate() {
        for tag in NOTE_TAGS {
            if only.is_some_and(|wanted| !wanted.eq_ignore_ascii_case(tag)) {
                continue;
            }
            // Only in a comment: `TODO` inside an identifier is not a note.
            let Some(at) = line.find(tag) else {
                continue;
            };
            if !line[..at].contains("//") && !line[..at].contains('*') {
                continue;
            }
            found.push(Note {
                tag: tag.to_string(),
                file: label.to_string(),
                line: index + 1,
                text: raw[at..].trim().to_string(),
            });
            break;
        }
    }
    found
}

#[cfg(test)]
mod layer_tests {
    use super::*;

    fn segs(path: &str) -> Vec<String> {
        path.split('/').map(str::to_string).collect()
    }

    /// Matched on whole segments, or `com/example/webshop` reads as the `web`
    /// layer and a project gets a Web row it never asked for.
    #[test]
    fn a_layer_matches_a_whole_segment_not_a_prefix() {
        assert!(contains_package(&segs("com/example/web/Foo.java"), "web"));
        assert!(!contains_package(
            &segs("com/example/webshop/Foo.java"),
            "web"
        ));
    }

    /// `jails.toml` allows a nested package, so the match has to span
    /// segments in sequence rather than compare one name.
    #[test]
    fn a_nested_layer_matches_its_segments_in_sequence() {
        let path = segs("com/example/demo/infra/jdbc/CsvReader.java");
        assert!(contains_package(&path, "infra.jdbc"));
        // Not the same thing as either half on its own appearing somewhere.
        assert!(!contains_package(
            &segs("com/example/jdbc/infra/X.java"),
            "infra.jdbc"
        ));
    }

    /// An empty layer value means the base package, which every file is
    /// under. Matching it would put the whole tree in one row.
    #[test]
    fn the_base_package_is_not_a_layer_that_swallows_everything() {
        assert!(!contains_package(&segs("com/example/demo/App.java"), ""));
    }

    /// The bug this fix exists for: `stats` kept its own layer list, so a
    /// project that renamed a layer in `jails.toml` had those files counted
    /// as "Other".
    #[test]
    fn a_renamed_layer_is_counted_under_its_configured_name() {
        let layers = vec![
            ("domain".to_string(), "Domain"),
            ("persistence".to_string(), "Adapters"),
        ];
        let adapters = Path::new("src/main/java/com/example/demo/persistence/CsvReader.java");
        assert_eq!(layer_index(adapters, &layers), 1);

        // And a file in no layer still falls to the catch-all.
        let loose = Path::new("src/main/java/com/example/demo/App.java");
        assert_eq!(layer_index(loose, &layers), layers.len());
    }

    /// Two layers the old copy of the list did not have at all, so their
    /// files were never counted anywhere but "Other".
    #[test]
    fn cli_and_messaging_are_layers_stats_knows_about() {
        let labels: Vec<&str> = crate::config::LAYERS_IN_ORDER
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert!(labels.contains(&"cli"), "{labels:?}");
        assert!(labels.contains(&"messaging"), "{labels:?}");
    }
}
