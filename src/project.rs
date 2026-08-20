use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::Result;
use crate::pom;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenModule {
    pub artifact_id: Option<String>,
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectContext {
    pub workspace: MavenModule,
    pub module: MavenModule,
    pub java_release: Option<u32>,
    pub spring_boot: bool,
    pub maven_command: PathBuf,
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

        let module_root = nearest_pom(&start)?;
        let workspace_root = workspace_root(&module_root)?;
        let module_pom = read_pom(&module_root)?;
        let workspace_pom = read_pom(&workspace_root)?;

        let mut modules = Vec::new();
        let mut seen = HashSet::new();
        collect_modules(&workspace_root, &workspace_root, &mut seen, &mut modules)?;

        Ok(Self {
            workspace: MavenModule {
                artifact_id: artifact_id(&workspace_pom),
                root: workspace_root.clone(),
            },
            module: MavenModule {
                artifact_id: artifact_id(&module_pom),
                root: module_root.clone(),
            },
            java_release: inherited_java_release(&module_root, &workspace_root)?,
            spring_boot: inherited_spring_boot(&module_root, &workspace_root)?,
            maven_command: maven_command(&workspace_root),
            modules,
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
        println!("Workspace: {}", module_label(&self.workspace));
        println!("  root: {}", self.workspace.root.display());
        println!("Module: {}", module_label(&self.module));
        println!("  root: {}", self.module.root.display());
        println!(
            "Java: {}",
            self.java_release
                .map(|release| release.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        println!(
            "Framework: {}",
            if self.spring_boot {
                "Spring Boot"
            } else {
                "plain Maven"
            }
        );
        println!("Maven: {}", self.maven_command.display());
        println!("Modules ({}):", self.modules.len());
        if self.modules.is_empty() {
            println!("  (none)");
        } else {
            for module in &self.modules {
                let path = module
                    .root
                    .strip_prefix(&self.workspace.root)
                    .unwrap_or(&module.root);
                println!("  {}  {}", module_label(module), path.display());
            }
        }
    }

    fn to_json(&self) -> String {
        let modules = self
            .modules
            .iter()
            .map(|module| {
                let path = module
                    .root
                    .strip_prefix(&self.workspace.root)
                    .unwrap_or(&module.root);
                format!(
                    "    {{\"artifact_id\": {}, \"path\": {}}}",
                    json_optional_string(module.artifact_id.as_deref()),
                    json_string(&path.to_string_lossy())
                )
            })
            .collect::<Vec<_>>()
            .join(",\n");

        format!(
            concat!(
                "{{\n",
                "  \"schema_version\": 1,\n",
                "  \"workspace\": {{\"root\": {}, \"artifact_id\": {}}},\n",
                "  \"module\": {{\"root\": {}, \"artifact_id\": {}}},\n",
                "  \"java_release\": {},\n",
                "  \"spring_boot\": {},\n",
                "  \"maven_command\": {},\n",
                "  \"modules\": [\n{}\n  ]\n",
                "}}"
            ),
            json_string(&self.workspace.root.to_string_lossy()),
            json_optional_string(self.workspace.artifact_id.as_deref()),
            json_string(&self.module.root.to_string_lossy()),
            json_optional_string(self.module.artifact_id.as_deref()),
            self.java_release
                .map(|release| release.to_string())
                .unwrap_or_else(|| "null".to_string()),
            self.spring_boot,
            json_string(&self.maven_command.to_string_lossy()),
            modules
        )
    }
}

pub fn about(json: bool) -> Result<()> {
    let context = ProjectContext::discover()?;
    context.print(json);
    Ok(())
}

fn nearest_pom(start: &Path) -> Result<PathBuf> {
    for dir in start.ancestors() {
        if dir.join("pom.xml").is_file() {
            return Ok(dir.to_path_buf());
        }
    }
    Err("no pom.xml found in this or any parent directory".to_string())
}

fn workspace_root(module_root: &Path) -> Result<PathBuf> {
    let mut workspace = module_root.to_path_buf();
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
            workspace = ancestor.to_path_buf();
        }
    }
    Ok(workspace)
}

fn collect_modules(
    workspace_root: &Path,
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
        if root == workspace_root || !seen.insert(root.clone()) {
            continue;
        }
        let child_pom = read_pom(&root)?;
        modules.push(MavenModule {
            artifact_id: artifact_id(&child_pom),
            root: root.clone(),
        });
        collect_modules(workspace_root, &root, seen, modules)?;
    }
    Ok(())
}

fn read_pom(root: &Path) -> Result<String> {
    fs::read_to_string(root.join("pom.xml"))
        .map_err(|e| format!("failed to read {}/pom.xml: {e}", root.display()))
}

/// The module's own artifactId, ignoring the parent's -- which is why the
/// `<parent>` block is dropped before looking.
pub(crate) fn artifact_id(xml: &str) -> Option<String> {
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

fn maven_command(workspace_root: &Path) -> PathBuf {
    let wrapper = workspace_root.join(if cfg!(windows) { "mvnw.cmd" } else { "mvnw" });
    if wrapper.is_file() {
        return wrapper;
    }
    if command_on_path(if cfg!(windows) { "mvnd.cmd" } else { "mvnd" }) {
        PathBuf::from("mvnd")
    } else {
        PathBuf::from("mvn")
    }
}

fn command_on_path(command: &str) -> bool {
    env::var_os("PATH")
        .map(|path| env::split_paths(&path).any(|dir| dir.join(command).is_file()))
        .unwrap_or(false)
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

fn json_optional_string(value: Option<&str>) -> String {
    value.map(json_string).unwrap_or_else(|| "null".to_string())
}

pub(crate) fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir(label: &str) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "jails-project-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        root
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

        assert_eq!(context.workspace.root, fs::canonicalize(&root).unwrap());
        assert_eq!(
            context.workspace.artifact_id.as_deref(),
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
            context.maven_command,
            fs::canonicalize(&root).unwrap().join("mvnw")
        );
    }

    #[test]
    fn standalone_project_is_its_own_workspace() {
        let root = fixture_dir("standalone");
        write_pom(
            &root,
            "<groupId>dev.example</groupId><artifactId>sample-cli</artifactId><properties><maven.compiler.release>27</maven.compiler.release></properties>",
        );

        let context = ProjectContext::discover_from(&root).unwrap();

        assert_eq!(context.workspace.root, context.module.root);
        assert_eq!(context.module.artifact_id.as_deref(), Some("sample-cli"));
        assert_eq!(context.java_release, Some(27));
        assert!(!context.spring_boot);
        assert!(context.modules.is_empty());
    }

    #[test]
    fn json_escapes_strings() {
        assert_eq!(json_string("a\\b\"c\n"), "\"a\\\\b\\\"c\\n\"");
    }
}
