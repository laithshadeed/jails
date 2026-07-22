use crate::Result;
use clap::ValueEnum;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum ArtifactKind {
    Scaffold,
    Controller,
    Service,
    Repository,
    Entity,
    Test,
}

pub struct Field {
    pub name: String,
    pub java_type: String,
    pub needs_lob: bool,
    pub import: Option<&'static str>,
}

fn field_type(token: &str) -> Result<(&'static str, bool, Option<&'static str>)> {
    match token {
        "string" => Ok(("String", false, None)),
        "text" => Ok(("String", true, None)),
        "int" | "integer" => Ok(("Integer", false, None)),
        "long" => Ok(("Long", false, None)),
        "boolean" => Ok(("Boolean", false, None)),
        "date" => Ok(("LocalDate", false, Some("java.time.LocalDate"))),
        "datetime" => Ok(("LocalDateTime", false, Some("java.time.LocalDateTime"))),
        "double" => Ok(("Double", false, None)),
        other => Err(format!(
            "unknown field type '{other}' (known: string, text, int/integer, long, boolean, date, datetime, double)"
        )),
    }
}

fn parse_fields(args: &[String]) -> Result<Vec<Field>> {
    args.iter()
        .map(|arg| {
            let (name, ty) = arg
                .split_once(':')
                .ok_or_else(|| format!("field '{arg}' must be name:type"))?;
            let (java_type, needs_lob, import) = field_type(ty.trim().to_lowercase().as_str())?;
            Ok(Field {
                name: name.trim().to_string(),
                java_type: java_type.to_string(),
                needs_lob,
                import,
            })
        })
        .collect()
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Walk up from the current directory looking for pom.xml.
pub(crate) fn find_project_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().map_err(|e| format!("failed to get cwd: {e}"))?;
    loop {
        if dir.join("pom.xml").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err("no pom.xml found in this or any parent directory".to_string());
        }
    }
}

/// Same logic as springgen.nvim's base_package(): read the package line off
/// the project's *Application.java entry point rather than configuring it.
fn base_package(root: &Path) -> Result<String> {
    let src_root = root.join("src/main/java");
    let entry = find_application_file(&src_root)
        .ok_or_else(|| "could not find *Application.java to infer the base package".to_string())?;
    let contents = fs::read_to_string(&entry).map_err(|e| format!("failed to read {}: {e}", entry.display()))?;
    for line in contents.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("package ") {
            if let Some(pkg) = rest.trim().strip_suffix(';') {
                return Ok(pkg.trim().to_string());
            }
        }
    }
    Err(format!("no package declaration found in {}", entry.display()))
}

fn find_application_file(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_application_file(&path) {
                return Some(found);
            }
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("Application.java"))
        {
            return Some(path);
        }
    }
    None
}

fn has_lombok(root: &Path) -> bool {
    fs::read_to_string(root.join("pom.xml"))
        .map(|s| s.contains("lombok"))
        .unwrap_or(false)
}

/// Spring Boot 4 moved `@AutoConfigureMockMvc` from
/// `org.springframework.boot.test.autoconfigure.web.servlet` to
/// `org.springframework.boot.webmvc.test.autoconfigure` with no back-compat
/// shim, so the scaffolded controller test needs to import the right one.
fn mockmvc_autoconfigure_import(root: &Path) -> &'static str {
    const LEGACY: &str = "org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc";
    const CURRENT: &str = "org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc";

    let pom = match fs::read_to_string(root.join("pom.xml")) {
        Ok(s) => s,
        Err(_) => return LEGACY,
    };
    let Some(idx) = pom.find("spring-boot-starter-parent") else {
        return LEGACY;
    };
    let after = &pom[idx..];
    let Some(vstart) = after.find("<version>").map(|i| i + "<version>".len()) else {
        return LEGACY;
    };
    let Some(vend) = after[vstart..].find("</version>") else {
        return LEGACY;
    };
    let major: Option<u32> = after[vstart..vstart + vend].split('.').next().and_then(|s| s.parse().ok());
    if major.is_some_and(|m| m >= 4) {
        CURRENT
    } else {
        LEGACY
    }
}

fn pkg_dir(pkg: &str) -> String {
    pkg.replace('.', "/")
}

fn main_dir(root: &Path, pkg: &str) -> PathBuf {
    root.join("src/main/java").join(pkg_dir(pkg))
}

fn test_dir(root: &Path, pkg: &str) -> PathBuf {
    root.join("src/test/java").join(pkg_dir(pkg))
}

struct Artifact {
    kind: &'static str,
    path: PathBuf,
    contents: String,
}

fn write_new_file(path: &Path, contents: &str) -> Result<()> {
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    fs::write(path, contents).map_err(|e| format!("failed to write {}: {e}", path.display()))
}

pub fn generate(kind: ArtifactKind, name: &str, fields: &[String]) -> Result<()> {
    let root = find_project_root()?;
    let pkg = base_package(&root)?;
    let name = capitalize(name);

    let artifacts = match kind {
        ArtifactKind::Scaffold => scaffold_artifacts(&root, &pkg, &name, fields)?,
        ArtifactKind::Controller => vec![Artifact {
            kind: "controller",
            path: main_dir(&root, &pkg).join(format!("{name}Controller.java")),
            contents: stub_controller(&pkg, &name),
        }],
        ArtifactKind::Service => vec![Artifact {
            kind: "service",
            path: main_dir(&root, &pkg).join(format!("{name}Service.java")),
            contents: stub_service(&pkg, &name),
        }],
        ArtifactKind::Repository => vec![Artifact {
            kind: "repository",
            path: main_dir(&root, &pkg).join(format!("{name}Repository.java")),
            contents: stub_repository(&pkg, &name),
        }],
        ArtifactKind::Entity => {
            let parsed = parse_fields(fields)?;
            vec![Artifact {
                kind: "entity",
                path: main_dir(&root, &pkg).join(format!("{name}.java")),
                contents: entity_java(&pkg, &name, &parsed, has_lombok(&root)),
            }]
        }
        ArtifactKind::Test => vec![Artifact {
            kind: "test",
            path: test_dir(&root, &pkg).join(format!("{name}Test.java")),
            contents: stub_test(&pkg, &name),
        }],
    };

    for artifact in &artifacts {
        if artifact.path.exists() {
            return Err(format!("{} already exists", artifact.path.display()));
        }
    }
    for artifact in &artifacts {
        write_new_file(&artifact.path, &artifact.contents)?;
        println!("created {} {}", artifact.kind, artifact.path.display());
    }
    Ok(())
}

fn scaffold_artifacts(root: &Path, pkg: &str, name: &str, fields: &[String]) -> Result<Vec<Artifact>> {
    let parsed = parse_fields(fields)?;
    let lombok = has_lombok(root);
    let route = name.to_lowercase() + "s";

    Ok(vec![
        Artifact {
            kind: "entity",
            path: main_dir(root, pkg).join(format!("{name}.java")),
            contents: entity_java(pkg, name, &parsed, lombok),
        },
        Artifact {
            kind: "repository",
            path: main_dir(root, pkg).join(format!("{name}Repository.java")),
            contents: stub_repository(pkg, name),
        },
        Artifact {
            kind: "service",
            path: main_dir(root, pkg).join(format!("{name}Service.java")),
            contents: service_full(pkg, name),
        },
        Artifact {
            kind: "controller",
            path: main_dir(root, pkg).join(format!("{name}Controller.java")),
            contents: controller_full(pkg, name, &route),
        },
        Artifact {
            kind: "controller test",
            path: test_dir(root, pkg).join(format!("{name}ControllerTest.java")),
            contents: controller_test(pkg, name, &route, mockmvc_autoconfigure_import(root)),
        },
    ])
}

pub fn destroy(kind: ArtifactKind, name: &str, force: bool) -> Result<()> {
    let root = find_project_root()?;
    let pkg = base_package(&root)?;
    let name = capitalize(name);

    let paths: Vec<PathBuf> = match kind {
        ArtifactKind::Scaffold => vec![
            main_dir(&root, &pkg).join(format!("{name}.java")),
            main_dir(&root, &pkg).join(format!("{name}Repository.java")),
            main_dir(&root, &pkg).join(format!("{name}Service.java")),
            main_dir(&root, &pkg).join(format!("{name}Controller.java")),
            test_dir(&root, &pkg).join(format!("{name}ControllerTest.java")),
        ],
        ArtifactKind::Controller => vec![main_dir(&root, &pkg).join(format!("{name}Controller.java"))],
        ArtifactKind::Service => vec![main_dir(&root, &pkg).join(format!("{name}Service.java"))],
        ArtifactKind::Repository => vec![main_dir(&root, &pkg).join(format!("{name}Repository.java"))],
        ArtifactKind::Entity => vec![main_dir(&root, &pkg).join(format!("{name}.java"))],
        ArtifactKind::Test => vec![test_dir(&root, &pkg).join(format!("{name}Test.java"))],
    };

    let existing: Vec<&PathBuf> = paths.iter().filter(|p| p.exists()).collect();
    if existing.is_empty() {
        println!("nothing to destroy");
        return Ok(());
    }

    if !force {
        println!("about to delete:");
        for p in &existing {
            println!("  {}", p.display());
        }
        print!("proceed? [y/N] ");
        std::io::stdout().flush().ok();
        let mut answer = String::new();
        std::io::stdin()
            .read_line(&mut answer)
            .map_err(|e| format!("failed to read confirmation: {e}"))?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("aborted");
            return Ok(());
        }
    }

    for p in existing {
        fs::remove_file(p).map_err(|e| format!("failed to remove {}: {e}", p.display()))?;
        println!("removed {}", p.display());
    }
    Ok(())
}

// ---- standalone stub templates (ported from springgen.nvim) ----

fn stub_controller(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
public class {name}Controller {{

    @GetMapping("/{route}")
    public String get() {{
        return "{name}";
    }}
}}
"#,
        route = name.to_lowercase()
    )
}

fn stub_service(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import org.springframework.stereotype.Service;

@Service
public class {name}Service {{
}}
"#
    )
}

fn stub_repository(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import org.springframework.data.jpa.repository.JpaRepository;

public interface {name}Repository extends JpaRepository<{name}, Long> {{
}}
"#
    )
}

fn stub_test(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;

class {name}Test {{

    @Test
    void shouldDoSomething() {{
        assertThat(true).isTrue();
    }}

}}
"#
    )
}

// ---- entity (shared by standalone `generate entity` and `generate scaffold`) ----

fn entity_java(pkg: &str, name: &str, fields: &[Field], lombok: bool) -> String {
    let mut imports = vec!["jakarta.persistence.Entity", "jakarta.persistence.GeneratedValue", "jakarta.persistence.Id"];
    if fields.iter().any(|f| f.needs_lob) {
        imports.push("jakarta.persistence.Lob");
    }
    imports.sort();

    let mut out = format!("package {pkg};\n\n");
    for imp in &imports {
        out += &format!("import {imp};\n");
    }
    if lombok {
        out += "import lombok.Data;\n";
    }
    let mut time_imports: Vec<&str> = fields.iter().filter_map(|f| f.import).collect();
    time_imports.sort();
    time_imports.dedup();
    if !time_imports.is_empty() {
        out += "\n";
        for imp in &time_imports {
            out += &format!("import {imp};\n");
        }
    }

    out += "\n";
    if lombok {
        out += "@Data\n";
    }
    out += "@Entity\n";
    out += &format!("public class {name} {{\n\n");
    out += "    @Id\n    @GeneratedValue\n    private Long id;\n";
    for field in fields {
        out += "\n";
        if field.needs_lob {
            out += "    @Lob\n";
        }
        out += &format!("    private {} {};\n", field.java_type, field.name);
    }

    if !lombok {
        out += "\n";
        out += &getter_setter("Long", "id");
        for field in fields {
            out += "\n";
            out += &getter_setter(&field.java_type, &field.name);
        }
    }

    out += "}\n";
    out
}

fn getter_setter(java_type: &str, name: &str) -> String {
    let cap = capitalize(name);
    format!(
        "    public {java_type} get{cap}() {{\n        return {name};\n    }}\n\n    public void set{cap}({java_type} {name}) {{\n        this.{name} = {name};\n    }}\n"
    )
}

// ---- scaffold's fuller service/controller/test (beyond the bare stubs) ----

fn service_full(pkg: &str, name: &str) -> String {
    let var = name.to_lowercase();
    format!(
        r#"package {pkg};

import org.springframework.http.HttpStatus;
import org.springframework.stereotype.Service;
import org.springframework.web.server.ResponseStatusException;

import java.util.List;

@Service
public class {name}Service {{

    private final {name}Repository repository;

    public {name}Service({name}Repository repository) {{
        this.repository = repository;
    }}

    public List<{name}> findAll() {{
        return repository.findAll();
    }}

    public {name} findById(Long id) {{
        return repository.findById(id)
                .orElseThrow(() -> new ResponseStatusException(HttpStatus.NOT_FOUND));
    }}

    public {name} save({name} {var}) {{
        return repository.save({var});
    }}

    public void deleteById(Long id) {{
        if (repository.existsById(id)) {{
            repository.deleteById(id);
        }}
    }}
}}
"#
    )
}

fn controller_full(pkg: &str, name: &str, route: &str) -> String {
    let var = name.to_lowercase();
    format!(
        r#"package {pkg};

import org.springframework.web.bind.annotation.DeleteMapping;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.PutMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

import java.util.List;

@RestController
@RequestMapping("/{route}")
public class {name}Controller {{

    private final {name}Service service;

    public {name}Controller({name}Service service) {{
        this.service = service;
    }}

    @GetMapping
    public List<{name}> index() {{
        return service.findAll();
    }}

    @GetMapping("/{{id}}")
    public {name} show(@PathVariable Long id) {{
        return service.findById(id);
    }}

    @PostMapping
    public {name} create(@RequestBody {name} {var}) {{
        return service.save({var});
    }}

    @PutMapping("/{{id}}")
    public {name} update(@PathVariable Long id, @RequestBody {name} {var}) {{
        {var}.setId(id);
        return service.save({var});
    }}

    @DeleteMapping("/{{id}}")
    public void destroy(@PathVariable Long id) {{
        service.deleteById(id);
    }}
}}
"#
    )
}

fn controller_test(pkg: &str, name: &str, route: &str, mockmvc_import: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import {mockmvc_import};
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.test.web.servlet.MockMvc;

import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.delete;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.put;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest
@AutoConfigureMockMvc
class {name}ControllerTest {{

    @Autowired
    private MockMvc mockMvc;

    @Test
    void index() throws Exception {{
        mockMvc.perform(get("/{route}")).andExpect(status().isOk());
    }}

    @Test
    void create() throws Exception {{
        mockMvc.perform(post("/{route}")
                        .contentType("application/json")
                        .content("{{}}"))
                .andExpect(status().is2xxSuccessful());
    }}

    @Test
    void show() throws Exception {{
        mockMvc.perform(get("/{route}/999999")).andExpect(status().isNotFound());
    }}

    @Test
    void update() throws Exception {{
        mockMvc.perform(put("/{route}/1")
                        .contentType("application/json")
                        .content("{{}}"))
                .andExpect(status().is2xxSuccessful());
    }}

    @Test
    void destroy() throws Exception {{
        mockMvc.perform(delete("/{route}/1")).andExpect(status().isOk());
    }}
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::CWD_LOCK;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jails-generate-test-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn capitalize_uppercases_first_letter_only() {
        assert_eq!(capitalize("post"), "Post");
        assert_eq!(capitalize("Post"), "Post");
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn field_type_maps_known_tokens() {
        assert_eq!(field_type("string").unwrap().0, "String");
        assert_eq!(field_type("text").unwrap(), ("String", true, None));
        assert_eq!(field_type("int").unwrap().0, "Integer");
        assert_eq!(field_type("integer").unwrap().0, "Integer");
        assert_eq!(field_type("long").unwrap().0, "Long");
        assert_eq!(field_type("boolean").unwrap().0, "Boolean");
        assert_eq!(field_type("double").unwrap().0, "Double");
        assert_eq!(field_type("date").unwrap(), ("LocalDate", false, Some("java.time.LocalDate")));
        assert_eq!(
            field_type("datetime").unwrap(),
            ("LocalDateTime", false, Some("java.time.LocalDateTime"))
        );
    }

    #[test]
    fn field_type_rejects_unknown_tokens() {
        assert!(field_type("uuid").is_err());
    }

    #[test]
    fn parse_fields_splits_name_and_type() {
        let fields = parse_fields(&["title:string".to_string(), "body:TEXT".to_string()]).unwrap();
        assert_eq!(fields[0].name, "title");
        assert_eq!(fields[0].java_type, "String");
        assert!(!fields[0].needs_lob);
        assert_eq!(fields[1].name, "body");
        assert!(fields[1].needs_lob);
    }

    #[test]
    fn parse_fields_rejects_args_without_a_colon() {
        assert!(parse_fields(&["title".to_string()]).is_err());
    }

    #[test]
    fn find_project_root_walks_up_to_pom_xml() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("project-root");
        fs::write(root.join("pom.xml"), "<project/>").unwrap();
        let nested = root.join("src/main/java/com/example");
        fs::create_dir_all(&nested).unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&nested).unwrap();
        let found = find_project_root();
        std::env::set_current_dir(original_cwd).unwrap();

        assert_eq!(found.unwrap(), root);
    }

    #[test]
    fn base_package_reads_the_application_class_package() {
        let root = scratch("base-package");
        let src = root.join("src/main/java/com/example/blog");
        fs::create_dir_all(&src).unwrap();
        fs::write(
            src.join("BlogApplication.java"),
            "package com.example.blog;\n\npublic class BlogApplication {}\n",
        )
        .unwrap();

        assert_eq!(base_package(&root).unwrap(), "com.example.blog");
    }

    #[test]
    fn base_package_errors_without_an_application_class() {
        let root = scratch("no-application");
        fs::create_dir_all(root.join("src/main/java")).unwrap();
        assert!(base_package(&root).is_err());
    }

    #[test]
    fn has_lombok_checks_pom_for_the_dependency() {
        let root = scratch("lombok");
        fs::write(root.join("pom.xml"), "<project><artifactId>lombok</artifactId></project>").unwrap();
        assert!(has_lombok(&root));

        let root2 = scratch("no-lombok");
        fs::write(root2.join("pom.xml"), "<project></project>").unwrap();
        assert!(!has_lombok(&root2));
    }

    #[test]
    fn mockmvc_import_picks_legacy_package_for_boot_3() {
        let root = scratch("boot3");
        fs::write(
            root.join("pom.xml"),
            "<parent><artifactId>spring-boot-starter-parent</artifactId><version>3.3.4</version></parent>",
        )
        .unwrap();
        assert_eq!(
            mockmvc_autoconfigure_import(&root),
            "org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc"
        );
    }

    #[test]
    fn mockmvc_import_picks_current_package_for_boot_4() {
        let root = scratch("boot4");
        fs::write(
            root.join("pom.xml"),
            "<parent><artifactId>spring-boot-starter-parent</artifactId><version>4.1.0</version></parent>",
        )
        .unwrap();
        assert_eq!(
            mockmvc_autoconfigure_import(&root),
            "org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc"
        );
    }

    #[test]
    fn mockmvc_import_defaults_to_legacy_when_pom_is_unreadable() {
        let root = scratch("no-pom");
        assert_eq!(
            mockmvc_autoconfigure_import(&root),
            "org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc"
        );
    }

    #[test]
    fn entity_java_without_lombok_includes_plain_getters_and_setters() {
        let fields = parse_fields(&["title:string".to_string(), "body:text".to_string()]).unwrap();
        let src = entity_java("com.example.blog", "Post", &fields, false);

        assert!(src.contains("public class Post {"));
        assert!(src.contains("@Id"));
        assert!(src.contains("@GeneratedValue"));
        assert!(src.contains("private String title;"));
        assert!(src.contains("@Lob"));
        assert!(src.contains("private String body;"));
        assert!(src.contains("public String getTitle()"));
        assert!(src.contains("public void setTitle(String title)"));
        assert!(!src.contains("@Data"));
    }

    #[test]
    fn entity_java_with_lombok_uses_data_and_skips_getters() {
        let fields = parse_fields(&["title:string".to_string()]).unwrap();
        let src = entity_java("com.example.blog", "Post", &fields, true);

        assert!(src.contains("import lombok.Data;"));
        assert!(src.contains("@Data"));
        assert!(!src.contains("getTitle"));
    }

    #[test]
    fn entity_java_imports_time_types_for_date_fields() {
        let fields = parse_fields(&["postedAt:datetime".to_string()]).unwrap();
        let src = entity_java("com.example.blog", "Post", &fields, false);
        assert!(src.contains("import java.time.LocalDateTime;"));
        assert!(src.contains("private LocalDateTime postedAt;"));
    }

    #[test]
    fn stub_templates_use_the_package_and_class_name() {
        assert!(stub_controller("com.example.blog", "Post").contains("public class PostController"));
        assert!(stub_service("com.example.blog", "Post").contains("public class PostService"));
        assert!(stub_repository("com.example.blog", "Post").contains("extends JpaRepository<Post, Long>"));
        assert!(stub_test("com.example.blog", "Post").contains("class PostTest"));
    }

    #[test]
    fn service_full_wraps_repository_crud() {
        let src = service_full("com.example.blog", "Post");
        assert!(src.contains("findAll()"));
        assert!(src.contains("findById(Long id)"));
        assert!(src.contains("save(Post post)"));
        assert!(src.contains("deleteById(Long id)"));
        assert!(src.contains("existsById(id)"));
    }

    #[test]
    fn controller_full_exposes_full_crud_routes() {
        let src = controller_full("com.example.blog", "Post", "posts");
        assert!(src.contains(r#"@RequestMapping("/posts")"#));
        assert!(src.contains("@GetMapping"));
        assert!(src.contains("@PostMapping"));
        assert!(src.contains("@PutMapping(\"/{id}\")"));
        assert!(src.contains("@DeleteMapping(\"/{id}\")"));
    }

    #[test]
    fn generate_scaffold_writes_all_five_files() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("scaffold");
        let src = root.join("src/main/java/com/example/blog");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(
            src.join("BlogApplication.java"),
            "package com.example.blog;\n\npublic class BlogApplication {}\n",
        )
        .unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let result = generate(ArtifactKind::Scaffold, "post", &["title:string".to_string()]);
        std::env::set_current_dir(original_cwd).unwrap();
        result.unwrap();

        assert!(src.join("Post.java").is_file());
        assert!(src.join("PostRepository.java").is_file());
        assert!(src.join("PostService.java").is_file());
        assert!(src.join("PostController.java").is_file());
        assert!(root.join("src/test/java/com/example/blog/PostControllerTest.java").is_file());
    }

    #[test]
    fn generate_refuses_to_overwrite_an_existing_file() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("no-overwrite");
        let src = root.join("src/main/java/com/example/blog");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(
            src.join("BlogApplication.java"),
            "package com.example.blog;\n\npublic class BlogApplication {}\n",
        )
        .unwrap();
        fs::write(src.join("CommentController.java"), "// already here").unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let result = generate(ArtifactKind::Controller, "comment", &[]);
        std::env::set_current_dir(original_cwd).unwrap();

        assert!(result.is_err());
        assert_eq!(fs::read_to_string(src.join("CommentController.java")).unwrap(), "// already here");
    }

    #[test]
    fn destroy_removes_only_files_that_generate_would_have_created() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("destroy");
        let src = root.join("src/main/java/com/example/blog");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(
            src.join("BlogApplication.java"),
            "package com.example.blog;\n\npublic class BlogApplication {}\n",
        )
        .unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        generate(ArtifactKind::Entity, "tag", &["name:string".to_string()]).unwrap();
        let result = destroy(ArtifactKind::Entity, "tag", true);
        std::env::set_current_dir(original_cwd).unwrap();

        result.unwrap();
        assert!(!src.join("Tag.java").is_file());
        assert!(src.join("BlogApplication.java").is_file());
    }
}
