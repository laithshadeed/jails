use crate::Result;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

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

pub fn generate(kind: &str, name: &str, fields: &[String]) -> Result<()> {
    let root = find_project_root()?;
    let pkg = base_package(&root)?;
    let name = capitalize(name);

    let artifacts = match kind {
        "scaffold" => scaffold_artifacts(&root, &pkg, &name, fields)?,
        "controller" => vec![Artifact {
            kind: "controller",
            path: main_dir(&root, &pkg).join(format!("{name}Controller.java")),
            contents: stub_controller(&pkg, &name),
        }],
        "service" => vec![Artifact {
            kind: "service",
            path: main_dir(&root, &pkg).join(format!("{name}Service.java")),
            contents: stub_service(&pkg, &name),
        }],
        "repository" => vec![Artifact {
            kind: "repository",
            path: main_dir(&root, &pkg).join(format!("{name}Repository.java")),
            contents: stub_repository(&pkg, &name),
        }],
        "entity" => {
            let parsed = parse_fields(fields)?;
            vec![Artifact {
                kind: "entity",
                path: main_dir(&root, &pkg).join(format!("{name}.java")),
                contents: entity_java(&pkg, &name, &parsed, has_lombok(&root)),
            }]
        }
        "test" => vec![Artifact {
            kind: "test",
            path: test_dir(&root, &pkg).join(format!("{name}Test.java")),
            contents: stub_test(&pkg, &name),
        }],
        other => {
            return Err(format!(
                "unknown generator '{other}' (expected: scaffold, controller, service, repository, entity, test)"
            ))
        }
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

pub fn destroy(kind: &str, name: &str, force: bool) -> Result<()> {
    let root = find_project_root()?;
    let pkg = base_package(&root)?;
    let name = capitalize(name);

    let paths: Vec<PathBuf> = match kind {
        "scaffold" => vec![
            main_dir(&root, &pkg).join(format!("{name}.java")),
            main_dir(&root, &pkg).join(format!("{name}Repository.java")),
            main_dir(&root, &pkg).join(format!("{name}Service.java")),
            main_dir(&root, &pkg).join(format!("{name}Controller.java")),
            test_dir(&root, &pkg).join(format!("{name}ControllerTest.java")),
        ],
        "controller" => vec![main_dir(&root, &pkg).join(format!("{name}Controller.java"))],
        "service" => vec![main_dir(&root, &pkg).join(format!("{name}Service.java"))],
        "repository" => vec![main_dir(&root, &pkg).join(format!("{name}Repository.java"))],
        "entity" => vec![main_dir(&root, &pkg).join(format!("{name}.java"))],
        "test" => vec![test_dir(&root, &pkg).join(format!("{name}Test.java"))],
        other => {
            return Err(format!(
                "unknown generator '{other}' (expected: scaffold, controller, service, repository, entity, test)"
            ))
        }
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
