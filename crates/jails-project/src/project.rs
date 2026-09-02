//! One resolved project: where it is, what builds it, and how it is laid out.
//!
//! `Project` is a parameter object, and the `root: &Path` ratchet measures
//! the absence of one: a root threaded through a call graph lets every level
//! re-derive the same facts, differently. Resolving once and passing the
//! value is the cure.
//!
//! It answers the questions the generators actually ask — the base package,
//! the layer packages after `jails.toml`'s renames, the pom flavour, the SQL
//! dialect, whether this is a multi-module build — and it answers them from
//! the project rather than from a manifest wherever the two could disagree.
//!
//! **`base_package()` falls back to the shallowest `.java` file** rather than
//! requiring `*Application.java`, which only Spring projects have; requiring
//! it fails `add` on exactly the projects it is most useful for.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::pom;
use jails_support::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MavenModule {
    pub artifact_id: Option<String>,
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectContext {
    pub reactor: MavenModule,
    pub module: MavenModule,
    pub java_release: Option<u32>,
    pub spring_boot: bool,
    /// Which build this report is about, so the labels can say so.
    pub build: crate::build::Build,
    /// The command that drives this build. Named for the job rather than for
    /// Maven: it is `gradlew` on a Gradle project, and a field called
    /// `maven_command` holding a path to `gradlew` is a lie the JSON then
    /// repeats to every consumer.
    pub build_command: PathBuf,
    pub modules: Vec<MavenModule>,
}

impl ProjectContext {
    pub fn discover() -> Result<Self> {
        let cwd = env::current_dir().map_err(|e| format!("failed to get cwd: {e}"))?;
        Self::discover_from(&cwd)
    }

    pub fn discover_from(start: &Path) -> Result<Self> {
        let start = fs::canonicalize(start)
            .map_err(|e| format!("failed to resolve {}: {e}", start.display()))?;
        let start = if start.is_file() {
            start
                .parent()
                .ok_or_else(|| format!("{} has no parent directory", start.display()))?
                .to_path_buf()
        } else {
            start
        };

        // One authority on "where does this project start", shared with every
        // other command. A second walk that knows only `pom.xml` refuses on a
        // Gradle project with "no pom.xml found", which is both wrong and
        // unactionable when jails works there.
        let module_root = nearest_build_root(&start)?;
        if crate::build::detect(&module_root) == crate::build::Build::Gradle {
            return Self::gradle(&module_root);
        }
        let reactor_root = reactor_root(&module_root)?;
        let module_pom = read_pom(&module_root)?;
        let reactor_pom = read_pom(&reactor_root)?;

        let mut modules = Vec::new();
        let mut seen = HashSet::new();
        collect_modules(&reactor_root, &reactor_root, &mut seen, &mut modules)?;

        Ok(Self {
            reactor: MavenModule {
                artifact_id: artifact_id(&reactor_pom),
                root: reactor_root.clone(),
            },
            module: MavenModule {
                artifact_id: artifact_id(&module_pom),
                root: module_root.clone(),
            },
            java_release: inherited_java_release(&module_root, &reactor_root)?,
            spring_boot: inherited_spring_boot(&module_root, &reactor_root)?,
            build: crate::build::Build::Maven,
            build_command: maven_command(&reactor_root),
            modules,
        })
    }

    /// The same report, for a Gradle build.
    ///
    /// Single-project only, and the module list says so rather than being
    /// silently empty: Gradle's multi-project model is `settings.gradle`'s
    /// `include` lines, which is a different shape from Maven's `<modules>`
    /// and is not read here. Reporting "no modules" for a build that has ten
    /// would be the confident wrong answer `gradle.rs` exists to avoid.
    fn gradle(root: &Path) -> Result<Self> {
        let text = fs::read_to_string(root.join(crate::gradle::FILE)).unwrap_or_default();
        // `settings.gradle` first, because that is where Gradle itself gets
        // the project name; the directory is the same fallback Gradle uses.
        let name = fs::read_to_string(root.join("settings.gradle"))
            .ok()
            .and_then(|settings| rootproject_name(&settings))
            .or_else(|| {
                root.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            });
        let module = MavenModule {
            artifact_id: name,
            root: root.to_path_buf(),
        };
        Ok(Self {
            reactor: module.clone(),
            module,
            java_release: crate::gradle::release_level(&text),
            spring_boot: crate::gradle::spring_boot_major(&text).is_some(),
            build: crate::build::Build::Gradle,
            build_command: match root.join("gradlew").is_file() {
                true => root.join("gradlew"),
                false => PathBuf::from("gradle"),
            },
            modules: Vec::new(),
        })
    }

    pub fn print(&self, json: bool) {
        if json {
            println!("{}", self.to_json());
        } else {
            self.print_human();
        }
    }

    fn print_human(&self) {
        println!("Reactor: {}", module_label(&self.reactor));
        println!("  root: {}", self.reactor.root.display());
        println!("Module: {}", module_label(&self.module));
        println!("  root: {}", self.module.root.display());
        println!(
            "Java: {}",
            self.java_release
                .map(|release| release.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        let gradle = self.build == crate::build::Build::Gradle;
        println!(
            "Framework: {}",
            match (self.spring_boot, gradle) {
                (true, _) => "Spring Boot",
                (false, true) => "plain Gradle",
                (false, false) => "plain Maven",
            }
        );
        println!(
            "{}: {}",
            match gradle {
                true => "Gradle",
                false => "Maven",
            },
            self.build_command.display()
        );
        // Named as unread rather than counted as none: Gradle's multi-project
        // model is `settings.gradle`'s `include` lines, and printing "(none)"
        // for a build with ten of them is the confident wrong answer.
        if gradle {
            println!("Modules: not read (Gradle multi-project is `settings.gradle` includes)");
            return;
        }
        println!("Modules ({}):", self.modules.len());
        if self.modules.is_empty() {
            println!("  (none)");
        } else {
            for module in &self.modules {
                let path = module
                    .root
                    .strip_prefix(&self.reactor.root)
                    .unwrap_or(&module.root);
                println!("  {}  {}", module_label(module), path.display());
            }
        }
    }

    fn to_json(&self) -> String {
        let config = crate::config::Config::load(&self.module.root).unwrap_or_default();
        let base_package = crate::spec::base_package(&self.module.root).ok();
        let layout = config
            .layout_entries()
            .into_iter()
            .map(|(name, package)| {
                format!(
                    "{}:{}",
                    crate::json::string(name),
                    crate::json::string(&package)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let capabilities = config
            .capabilities()
            .iter()
            .map(|capability| crate::json::string(capability))
            .collect::<Vec<_>>()
            .join(",");
        let java_root = self.module.root.join("src/main/java");
        let test_root = self.module.root.join("src/test/java");
        let modules = self
            .modules
            .iter()
            .map(|module| {
                let path = module
                    .root
                    .strip_prefix(&self.reactor.root)
                    .unwrap_or(&module.root);
                format!(
                    "    {{\"artifact_id\": {}, \"path\": {}}}",
                    crate::json::optional_string(module.artifact_id.as_deref()),
                    crate::json::string(&path.to_string_lossy())
                )
            })
            .collect::<Vec<_>>()
            .join(",\n");

        format!(
            concat!(
                "{{\n",
                "  \"schema_version\": 4,\n",
                "  \"reactor\": {{\"root\": {}, \"artifact_id\": {}}},\n",
                "  \"module\": {{\"root\": {}, \"artifact_id\": {}}},\n",
                "  \"base_package\": {},\n",
                "  \"java_root\": {},\n",
                "  \"test_root\": {},\n",
                "  \"layout\": {{{}}},\n",
                "  \"capabilities\": [{}],\n",
                "  \"java_release\": {},\n",
                "  \"spring_boot\": {},\n",
                "  \"build\": {}, \"build_command\": {},\n",
                "  \"modules\": [\n{}\n  ]\n",
                "}}"
            ),
            crate::json::string(&self.reactor.root.to_string_lossy()),
            crate::json::optional_string(self.reactor.artifact_id.as_deref()),
            crate::json::string(&self.module.root.to_string_lossy()),
            crate::json::optional_string(self.module.artifact_id.as_deref()),
            crate::json::optional_string(base_package.as_deref()),
            crate::json::string(&java_root.to_string_lossy()),
            crate::json::string(&test_root.to_string_lossy()),
            layout,
            capabilities,
            self.java_release
                .map(|release| release.to_string())
                .unwrap_or_else(|| "null".to_string()),
            self.spring_boot,
            crate::json::string(self.build.name()),
            crate::json::string(&self.build_command.to_string_lossy()),
            modules
        )
    }
}

pub fn about(json: bool) -> Result<()> {
    let context = ProjectContext::discover()?;
    context.print(json);
    Ok(())
}

/// The nearest ancestor holding a build file jails reads.
///
/// Through `build::detect`, so this cannot drift from what every other command
/// considers a project root.
fn nearest_build_root(start: &Path) -> Result<PathBuf> {
    for dir in start.ancestors() {
        if crate::build::is_readable(crate::build::detect(dir)) {
            return Ok(dir.to_path_buf());
        }
    }
    Err(jails_support::Failure::Told(
        "no pom.xml or build.gradle found in this or any parent directory".to_string(),
    ))
}

/// `rootProject.name = 'spring'` out of a `settings.gradle`.
fn rootproject_name(settings: &str) -> Option<String> {
    let at = settings.find("rootProject.name")?;
    let rest = &settings[at..];
    let open = rest.find(['\'', '"'])?;
    let quote = rest.as_bytes()[open];
    let tail = &rest[open + 1..];
    let end = tail.find(quote as char)?;
    Some(tail[..end].to_string())
}

fn reactor_root(module_root: &Path) -> Result<PathBuf> {
    let mut reactor = module_root.to_path_buf();
    for ancestor in module_root.ancestors().skip(1) {
        if !ancestor.join("pom.xml").is_file() {
            continue;
        }
        let pom = read_pom(ancestor)?;
        let contains_module = module_paths(&pom).into_iter().any(|declared| {
            fs::canonicalize(ancestor.join(declared))
                .map(|root| module_root.starts_with(root))
                .unwrap_or(false)
        });
        if contains_module {
            reactor = ancestor.to_path_buf();
        }
    }
    Ok(reactor)
}

fn collect_modules(
    reactor_root: &Path,
    aggregator_root: &Path,
    seen: &mut HashSet<PathBuf>,
    modules: &mut Vec<MavenModule>,
) -> Result<()> {
    let pom = read_pom(aggregator_root)?;
    for declared in module_paths(&pom) {
        let root = match fs::canonicalize(aggregator_root.join(&declared)) {
            Ok(root) if root.join("pom.xml").is_file() => root,
            _ => continue,
        };
        if root == reactor_root || !seen.insert(root.clone()) {
            continue;
        }
        let child_pom = read_pom(&root)?;
        modules.push(MavenModule {
            artifact_id: artifact_id(&child_pom),
            root: root.clone(),
        });
        collect_modules(reactor_root, &root, seen, modules)?;
    }
    Ok(())
}

fn read_pom(root: &Path) -> Result<String> {
    Ok(fs::read_to_string(root.join("pom.xml"))
        .map_err(|e| format!("failed to read {}/pom.xml: {e}", root.display()))?)
}

/// The module's own artifactId, ignoring the parent's -- which is why the
/// `<parent>` block is dropped before looking.
pub fn artifact_id(xml: &str) -> Option<String> {
    let xml = without_comments(xml);
    let xml = without_first_block(&xml, "parent");
    element_values(&xml, "artifactId").into_iter().next()
}

fn module_paths(xml: &str) -> Vec<String> {
    let xml = without_comments(xml);
    block_values(&xml, "modules")
        .into_iter()
        .flat_map(|block| element_values(block, "module"))
        .collect()
}

fn inherited_java_release(module_root: &Path, workspace_root: &Path) -> Result<Option<u32>> {
    for root in roots_to_workspace(module_root, workspace_root) {
        if let Some(release) = pom::release_level(&read_pom(root)?) {
            return Ok(Some(release));
        }
    }
    Ok(None)
}

fn inherited_spring_boot(module_root: &Path, workspace_root: &Path) -> Result<bool> {
    for root in roots_to_workspace(module_root, workspace_root) {
        if read_pom(root)?.contains("org.springframework.boot") {
            return Ok(true);
        }
    }
    Ok(false)
}

fn roots_to_workspace<'a>(module_root: &'a Path, workspace_root: &'a Path) -> Vec<&'a Path> {
    module_root
        .ancestors()
        .take_while(|root| root.starts_with(workspace_root))
        .collect()
}

/// What `about` reports is what `run`/`test` will actually execute, because
/// it is the same function; two copies disagree about `mvnd.cmd` on Windows.
fn maven_command(workspace_root: &Path) -> PathBuf {
    crate::maven::binary(workspace_root)
}

#[cfg(test)]
pub(crate) fn maven_command_for_tests(workspace_root: &Path) -> PathBuf {
    maven_command(workspace_root)
}

fn without_comments(xml: &str) -> String {
    let mut result = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find("<!--") {
        result.push_str(&rest[..start]);
        if let Some(end) = rest[start + 4..].find("-->") {
            rest = &rest[start + 4 + end + 3..];
        } else {
            rest = "";
            break;
        }
    }
    result.push_str(rest);
    result
}

fn without_first_block(xml: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let Some(start) = xml.find(&open) else {
        return xml.to_string();
    };
    let Some(relative_end) = xml[start + open.len()..].find(&close) else {
        return xml.to_string();
    };
    let end = start + open.len() + relative_end + close.len();
    let mut result = xml.to_string();
    result.replace_range(start..end, "");
    result
}

fn block_values<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut values = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let content = &rest[start + open.len()..];
        let Some(end) = content.find(&close) else {
            break;
        };
        values.push(&content[..end]);
        rest = &content[end + close.len()..];
    }
    values
}

fn element_values(xml: &str, tag: &str) -> Vec<String> {
    block_values(xml, tag)
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn module_label(module: &MavenModule) -> &str {
    module.artifact_id.as_deref().unwrap_or("unknown")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir(label: &str) -> PathBuf {
        jails_support::scratch::ScratchDir::in_temp(&format!("jails-project-{label}"))
            .unwrap()
            .keep()
    }

    fn write_pom(root: &Path, body: &str) {
        fs::create_dir_all(root).unwrap();
        fs::write(
            root.join("pom.xml"),
            format!("<project><modelVersion>4.0.0</modelVersion>{body}</project>"),
        )
        .unwrap();
    }

    #[test]
    fn discovers_nested_reactor_and_active_module() {
        let root = fixture_dir("nested-reactor");
        write_pom(
            &root,
            "<groupId>dev.example</groupId><artifactId>sample-parent</artifactId><properties><java.version>26</java.version></properties><dependencyManagement><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-dependencies</artifactId></dependency></dependencies></dependencyManagement><modules><module>sample-core</module><module>services</module></modules>",
        );
        write_pom(
            &root.join("sample-core"),
            "<parent><groupId>dev.example</groupId><artifactId>sample-parent</artifactId></parent><artifactId>sample-core</artifactId>",
        );
        write_pom(
            &root.join("services"),
            "<parent><groupId>dev.example</groupId><artifactId>sample-parent</artifactId></parent><artifactId>sample-services</artifactId><modules><module>sample-web</module></modules>",
        );
        write_pom(
            &root.join("services/sample-web"),
            "<parent><groupId>dev.example</groupId><artifactId>sample-services</artifactId></parent><artifactId>sample-web</artifactId>",
        );
        fs::write(root.join("mvnw"), "#!/bin/sh\n").unwrap();
        let source = root.join("services/sample-web/src/main/java/dev/example");
        fs::create_dir_all(&source).unwrap();

        let context = ProjectContext::discover_from(&source).unwrap();

        assert_eq!(context.reactor.root, fs::canonicalize(&root).unwrap());
        assert_eq!(
            context.reactor.artifact_id.as_deref(),
            Some("sample-parent")
        );
        assert_eq!(context.module.artifact_id.as_deref(), Some("sample-web"));
        assert_eq!(context.java_release, Some(26));
        assert!(context.spring_boot);
        assert_eq!(context.modules.len(), 3);
        assert_eq!(
            context
                .modules
                .iter()
                .filter_map(|module| module.artifact_id.as_deref())
                .collect::<Vec<_>>(),
            vec!["sample-core", "sample-services", "sample-web"]
        );
        assert_eq!(
            context.build_command,
            fs::canonicalize(&root).unwrap().join("mvnw")
        );
        let json = context.to_json();
        assert!(json.contains("\"schema_version\": 4"), "{json}");
        assert!(json.contains("\"reactor\":"), "{json}");
        assert!(json.contains("\"base_package\":"), "{json}");
        assert!(json.contains("\"layout\":"), "{json}");
        assert!(json.contains("\"capabilities\":"), "{json}");
    }

    #[test]
    fn standalone_project_is_its_own_reactor() {
        let root = fixture_dir("standalone");
        write_pom(
            &root,
            "<groupId>dev.example</groupId><artifactId>sample-cli</artifactId><properties><maven.compiler.release>27</maven.compiler.release></properties>",
        );

        let context = ProjectContext::discover_from(&root).unwrap();

        assert_eq!(context.reactor.root, context.module.root);
        assert_eq!(context.module.artifact_id.as_deref(), Some("sample-cli"));
        assert_eq!(context.java_release, Some(27));
        assert!(!context.spring_boot);
        assert!(context.modules.is_empty());
    }

    #[test]
    fn json_escapes_strings() {
        assert_eq!(crate::json::string("a\\b\"c\n"), "\"a\\\\b\\\"c\\n\"");
    }
}
