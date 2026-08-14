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
    Record,
    Value,
    Command,
    Cli,
    Cases,
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

/// The field table maps `long` to `Long` because a JPA entity has to be able
/// to represent a null column. A `record` or a `value` has no such excuse:
/// boxed components are nullable by construction, which is exactly what the
/// compact constructor then has to spend a `requireNonNull` undoing. Primitives
/// make the invalid state unrepresentable instead -- and cost no allocation.
fn unboxed(java_type: &str) -> &str {
    match java_type {
        "Integer" => "int",
        "Long" => "long",
        "Boolean" => "boolean",
        "Double" => "double",
        other => other,
    }
}

/// A primitive component cannot be null, so it needs no runtime check.
fn is_reference_type(java_type: &str) -> bool {
    !matches!(java_type, "int" | "long" | "boolean" | "double")
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
pub(crate) fn base_package(root: &Path) -> Result<String> {
    let src_root = root.join("src/main/java");
    // Spring projects have a *Application.java entry point; `new-cli` ones
    // have App.java, so fall back to whatever source file sits closest to the
    // source root rather than failing on plain Maven projects.
    let entry = find_application_file(&src_root)
        .or_else(|| shallowest_java_file(&src_root))
        .ok_or_else(|| "could not find a .java file under src/main/java to infer the base package".to_string())?;
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

/// The .java file with the fewest path segments below `dir`, i.e. the one in
/// the outermost package -- for a plain Maven project that is the base package
/// by construction.
fn shallowest_java_file(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(usize, PathBuf)> = None;
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((current, depth)) = stack.pop() {
        for entry in fs::read_dir(&current).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push((path, depth + 1));
            } else if path.extension().is_some_and(|e| e == "java") {
                let better = best.as_ref().is_none_or(|(d, _)| depth < *d);
                if better {
                    best = Some((depth, path));
                }
            }
        }
    }
    best.map(|(_, path)| path)
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

/// Where each kind of artifact lives, relative to the project's base package.
///
/// A generated project should look like one a person laid out, and nobody
/// lays out thirty classes as siblings of `App.java`. The names are the ones
/// the Java ecosystem already uses, so the layout reads as conventional rather
/// than as jails' invention -- and every one of them is a package a human
/// would have created by hand on about the third file.
///
/// This is a default, not a policy: `--package` overrides it, and `--package
/// ''` puts everything back in the base package for a project small enough not
/// to want the ceremony.
pub(crate) mod layout {
    pub const DOMAIN: &str = "domain";
    pub const REPOSITORY: &str = "repository";
    pub const SERVICE: &str = "service";
    pub const WEB: &str = "web";
    pub const CLI: &str = "cli";
    pub const ADAPTERS: &str = "adapters";
    pub const API: &str = "api";
    pub const TESTKIT: &str = "testkit";
}

/// `com.example.demo` + `domain` -> `com.example.demo.domain`. An empty
/// subpackage leaves the base package alone.
pub(crate) fn subpackage(base: &str, sub: &str) -> String {
    if sub.is_empty() {
        base.to_string()
    } else {
        format!("{base}.{sub}")
    }
}

fn pkg_dir(pkg: &str) -> String {
    pkg.replace('.', "/")
}

pub(crate) fn main_dir(root: &Path, pkg: &str) -> PathBuf {
    root.join("src/main/java").join(pkg_dir(pkg))
}

pub(crate) fn test_dir(root: &Path, pkg: &str) -> PathBuf {
    root.join("src/test/java").join(pkg_dir(pkg))
}

struct Artifact {
    kind: &'static str,
    path: PathBuf,
    contents: String,
}

pub(crate) fn write_new_file(path: &Path, contents: &str) -> Result<()> {
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    let contents = if path.extension().is_some_and(|e| e == "java") {
        normalize_imports(contents)
    } else {
        contents.to_string()
    };
    fs::write(path, &contents).map_err(|e| format!("failed to write {}: {e}", path.display()))
}

/// Rewrite a generated file's import block into the order
/// palantir-java-format produces: static imports first, a blank line, then
/// everything else sorted.
///
/// Done here, once, rather than by hand in each of the twenty-odd templates.
/// Hand-ordering is a rule that decays -- the next template gets it wrong and
/// nobody notices until `jails add format` makes `mvn verify` fail on a
/// freshly generated project, which is a bad first impression for a scaffold
/// to make.
pub(crate) fn normalize_imports(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();

    let Some(package_at) = lines.iter().position(|l| l.trim_start().starts_with("package ")) else {
        return source.to_string();
    };

    // Imports are only ever between the package declaration and the first
    // other construct, so scanning stops at the first line that is neither an
    // import nor blank -- a Javadoc block, an annotation, the type itself.
    let mut statics: Vec<&str> = Vec::new();
    let mut plain: Vec<&str> = Vec::new();
    let mut end = package_at + 1;
    for (offset, line) in lines[package_at + 1..].iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("import ") {
            if rest.starts_with("static ") {
                statics.push(trimmed);
            } else {
                plain.push(trimmed);
            }
            end = package_at + 1 + offset + 1;
            continue;
        }
        break;
    }

    if statics.is_empty() && plain.is_empty() {
        return source.to_string();
    }

    statics.sort_unstable();
    statics.dedup();
    plain.sort_unstable();
    plain.dedup();

    let mut out = String::with_capacity(source.len() + 32);
    for line in &lines[..=package_at] {
        out.push_str(line);
        out.push('\n');
    }
    out.push('\n');
    for group in [&statics, &plain] {
        if group.is_empty() {
            continue;
        }
        for line in group.iter() {
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }
    // Whatever followed the imports, with any blank lines it was padded with
    // already consumed above.
    for line in lines[end..].iter().skip_while(|l| l.trim().is_empty()) {
        out.push_str(line);
        out.push('\n');
    }
    out
}

pub fn generate(kind: ArtifactKind, name: &str, fields: &[String], package: Option<&str>) -> Result<()> {
    let root = find_project_root()?;
    let base = base_package(&root)?;

    // `cases` is the one kind whose <NAME> is a path, not a class name: the
    // class is derived from the file it reads. Handle it before the shared
    // capitalize, which would mangle a path.
    if matches!(kind, ArtifactKind::Cases) {
        return generate_cases(&root, &subpackage(&base, package.unwrap_or("")), Path::new(name));
    }

    let name = capitalize(name);
    // `--package` replaces the conventional home for every artifact in this
    // call; without it each kind goes where its convention says.
    let place = |default: &str| subpackage(&base, package.unwrap_or(default));

    let artifacts = match kind {
        ArtifactKind::Scaffold => scaffold_artifacts(&root, &base, &name, fields, package)?,
        ArtifactKind::Controller => {
            let web = place(layout::WEB);
            vec![
                Artifact {
                    kind: "controller",
                    path: main_dir(&root, &web).join(format!("{name}Controller.java")),
                    contents: stub_controller(&web, &name),
                },
                Artifact {
                    kind: "controller test",
                    path: test_dir(&root, &web).join(format!("{name}ControllerTest.java")),
                    contents: controller_stub_test(&web, &name, mockmvc_autoconfigure_import(&root)),
                },
            ]
        }
        ArtifactKind::Service => {
            let service = place(layout::SERVICE);
            vec![
                Artifact {
                    kind: "service",
                    path: main_dir(&root, &service).join(format!("{name}Service.java")),
                    contents: stub_service(&service, &name),
                },
                Artifact {
                    kind: "service test",
                    path: test_dir(&root, &service).join(format!("{name}ServiceTest.java")),
                    contents: service_stub_test(&service, &name),
                },
            ]
        }
        ArtifactKind::Repository => {
            let repository = place(layout::REPOSITORY);
            // The entity it is a repository *of* now lives one package over.
            let domain = place(layout::DOMAIN);
            vec![Artifact {
                kind: "repository",
                path: main_dir(&root, &repository).join(format!("{name}Repository.java")),
                contents: stub_repository(&repository, &name, &import_of(&repository, &domain, &name)),
            }]
        }
        ArtifactKind::Entity => {
            let parsed = parse_fields(fields)?;
            let domain = place(layout::DOMAIN);
            vec![
                Artifact {
                    kind: "entity",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: entity_java(&domain, &name, &parsed, has_lombok(&root)),
                },
                Artifact {
                    kind: "entity test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: entity_test(&domain, &name, &parsed),
                },
            ]
        }
        ArtifactKind::Record => {
            let parsed = parse_fields(fields)?;
            let domain = place(layout::DOMAIN);
            vec![
                Artifact {
                    kind: "record",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: record_java(&domain, &name, &parsed),
                },
                Artifact {
                    kind: "record test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: record_test(&domain, &name, &parsed),
                },
            ]
        }
        ArtifactKind::Value => {
            let parsed = parse_fields(fields)?;
            if parsed.is_empty() {
                return Err("a value type needs at least one field, e.g. `generate value Money amount:long`".to_string());
            }
            let domain = place(layout::DOMAIN);
            vec![
                Artifact {
                    kind: "value",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: value_java(&domain, &name, &parsed),
                },
                Artifact {
                    kind: "value test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: value_test(&domain, &name, &parsed),
                },
            ]
        }
        ArtifactKind::Command => {
            let cli = place(layout::CLI);
            vec![
                Artifact {
                    kind: "command",
                    path: main_dir(&root, &cli).join(format!("{name}Command.java")),
                    contents: command_java(&cli, &name),
                },
                Artifact {
                    kind: "command test",
                    path: test_dir(&root, &cli).join(format!("{name}CommandTest.java")),
                    contents: command_test(&cli, &name),
                },
            ]
        }
        ArtifactKind::Cli => {
            let cli = place(layout::CLI);
            vec![
                Artifact {
                    kind: "cli",
                    path: main_dir(&root, &cli).join(format!("{name}Cli.java")),
                    contents: cli_java(&cli, &format!("{name}Cli"), &name.to_lowercase()),
                },
                Artifact {
                    kind: "cli test",
                    path: test_dir(&root, &cli).join(format!("{name}CliTest.java")),
                    contents: cli_test(&cli, &format!("{name}Cli")),
                },
            ]
        }
        ArtifactKind::Cases => unreachable!("handled above -- its NAME is a path, not a class"),
        ArtifactKind::Test => {
            let pkg = place("");
            vec![Artifact {
                kind: "test",
                path: test_dir(&root, &pkg).join(format!("{name}Test.java")),
                contents: stub_test(&pkg, &name),
            }]
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

    if matches!(kind, ArtifactKind::Command) {
        register_command(&root, &base, &name)?;
    }
    Ok(())
}

/// An `import` line for `{from}.{class}`, or nothing at all when the two
/// packages are the same -- importing a sibling is a compile error.
fn import_of(user: &str, owner: &str, class: &str) -> String {
    if user == owner {
        String::new()
    } else {
        format!("import {owner}.{class};\n")
    }
}

/// The one command that spans layers, and so the only place that has to say
/// out loud which package each half of a vertical slice lives in -- and add
/// the imports that crossing those boundaries now costs.
fn scaffold_artifacts(
    root: &Path,
    base: &str,
    name: &str,
    fields: &[String],
    package: Option<&str>,
) -> Result<Vec<Artifact>> {
    let parsed = parse_fields(fields)?;
    let lombok = has_lombok(root);
    let route = name.to_lowercase() + "s";

    let place = |default: &str| subpackage(base, package.unwrap_or(default));
    let domain = place(layout::DOMAIN);
    let repository = place(layout::REPOSITORY);
    let service = place(layout::SERVICE);
    let web = place(layout::WEB);

    let entity_in = |user: &str| import_of(user, &domain, name);
    let repository_in = |user: &str| import_of(user, &repository, &format!("{name}Repository"));
    let service_in = |user: &str| import_of(user, &service, &format!("{name}Service"));

    Ok(vec![
        Artifact {
            kind: "entity",
            path: main_dir(root, &domain).join(format!("{name}.java")),
            contents: entity_java(&domain, name, &parsed, lombok),
        },
        Artifact {
            kind: "entity test",
            path: test_dir(root, &domain).join(format!("{name}Test.java")),
            contents: entity_test(&domain, name, &parsed),
        },
        Artifact {
            kind: "repository",
            path: main_dir(root, &repository).join(format!("{name}Repository.java")),
            contents: stub_repository(&repository, name, &entity_in(&repository)),
        },
        Artifact {
            kind: "service",
            path: main_dir(root, &service).join(format!("{name}Service.java")),
            contents: service_full(
                &service,
                name,
                &format!("{}{}", entity_in(&service), repository_in(&service)),
            ),
        },
        Artifact {
            kind: "controller",
            path: main_dir(root, &web).join(format!("{name}Controller.java")),
            contents: controller_full(
                &web,
                name,
                &route,
                &format!("{}{}", entity_in(&web), service_in(&web)),
            ),
        },
        Artifact {
            kind: "controller test",
            path: test_dir(root, &web).join(format!("{name}ControllerTest.java")),
            contents: controller_test(&web, name, &route, mockmvc_autoconfigure_import(root), &entity_in(&web)),
        },
    ])
}

pub fn destroy(kind: ArtifactKind, name: &str, force: bool, package: Option<&str>) -> Result<()> {
    let root = find_project_root()?;
    let base = base_package(&root)?;
    let place = |default: &str| subpackage(&base, package.unwrap_or(default));
    // `cases` is addressed by the markdown path it was generated from, which
    // must not be run through capitalize like a class name.
    let raw_name = name.to_string();
    let name = capitalize(name);

    let paths: Vec<PathBuf> = match kind {
        ArtifactKind::Scaffold => vec![
            main_dir(&root, &place(layout::DOMAIN)).join(format!("{name}.java")),
            test_dir(&root, &place(layout::DOMAIN)).join(format!("{name}Test.java")),
            main_dir(&root, &place(layout::REPOSITORY)).join(format!("{name}Repository.java")),
            main_dir(&root, &place(layout::SERVICE)).join(format!("{name}Service.java")),
            main_dir(&root, &place(layout::WEB)).join(format!("{name}Controller.java")),
            test_dir(&root, &place(layout::WEB)).join(format!("{name}ControllerTest.java")),
        ],
        ArtifactKind::Controller => vec![
            main_dir(&root, &place(layout::WEB)).join(format!("{name}Controller.java")),
            test_dir(&root, &place(layout::WEB)).join(format!("{name}ControllerTest.java")),
        ],
        ArtifactKind::Service => vec![
            main_dir(&root, &place(layout::SERVICE)).join(format!("{name}Service.java")),
            test_dir(&root, &place(layout::SERVICE)).join(format!("{name}ServiceTest.java")),
        ],
        ArtifactKind::Repository => {
            vec![main_dir(&root, &place(layout::REPOSITORY)).join(format!("{name}Repository.java"))]
        }
        // An entity, a record and a value are three shapes of the same named
        // type, so they occupy -- and free -- exactly the same two paths.
        ArtifactKind::Entity | ArtifactKind::Record | ArtifactKind::Value => vec![
            main_dir(&root, &place(layout::DOMAIN)).join(format!("{name}.java")),
            test_dir(&root, &place(layout::DOMAIN)).join(format!("{name}Test.java")),
        ],
        ArtifactKind::Command => vec![
            main_dir(&root, &place(layout::CLI)).join(format!("{name}Command.java")),
            test_dir(&root, &place(layout::CLI)).join(format!("{name}CommandTest.java")),
        ],
        ArtifactKind::Cli => vec![
            main_dir(&root, &place(layout::CLI)).join(format!("{name}Cli.java")),
            test_dir(&root, &place(layout::CLI)).join(format!("{name}CliTest.java")),
        ],
        // `cases` derives its class from a markdown file's name, so destroy
        // takes that same path and resolves it the same way generate did.
        ArtifactKind::Cases => {
            vec![test_dir(&root, &place("")).join(format!("{}.java", cases_class_name(Path::new(&raw_name))?))]
        }
        ArtifactKind::Test => vec![test_dir(&root, &place("")).join(format!("{name}Test.java"))],
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

fn stub_repository(pkg: &str, name: &str, extra: &str) -> String {
    format!(
        r#"package {pkg};
{extra}
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

// ---- companion tests for the bare `generate controller`/`service` stubs
// (Rails generates a test alongside controller/model generators; we do the
// same -- repository is a plain JpaRepository delegate with nothing of its
// own to assert, so it gets no companion test) ----

fn controller_stub_test(pkg: &str, name: &str, mockmvc_import: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import {mockmvc_import};
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.test.web.servlet.MockMvc;

import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.content;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest
@AutoConfigureMockMvc
class {name}ControllerTest {{

    @Autowired
    private MockMvc mockMvc;

    @Test
    void getReturnsOk() throws Exception {{
        mockMvc.perform(get("/{route}"))
                .andExpect(status().isOk())
                .andExpect(content().string("{name}"));
    }}
}}
"#,
        route = name.to_lowercase()
    )
}

fn service_stub_test(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;

class {name}ServiceTest {{

    @Test
    void instantiates() {{
        assertThat(new {name}Service()).isNotNull();
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

/// A companion test round-tripping every getter/setter (including
/// Lombok's @Data-generated ones, which compile the same as hand-written).
fn entity_test(pkg: &str, name: &str, fields: &[Field]) -> String {
    let mut imports: Vec<&str> = fields.iter().filter_map(|f| f.import).collect();
    imports.sort();
    imports.dedup();

    let mut out = format!("package {pkg};\n\n");
    out += "import org.junit.jupiter.api.Test;\n";
    if !imports.is_empty() {
        out += "\n";
        for imp in &imports {
            out += &format!("import {imp};\n");
        }
    }
    out += "\nimport static org.assertj.core.api.Assertions.assertThat;\n\n";
    out += &format!("class {name}Test {{\n\n");
    out += "    @Test\n    void gettersAndSettersRoundTrip() {\n";
    out += &format!("        {name} entity = new {name}();\n");
    out += "        entity.setId(1L);\n";
    for field in fields {
        out += &format!("        entity.set{}({});\n", capitalize(&field.name), sample_literal(&field.java_type));
    }
    out += "\n        assertThat(entity.getId()).isEqualTo(1L);\n";
    for field in fields {
        out += &format!(
            "        assertThat(entity.get{}()).isEqualTo({});\n",
            capitalize(&field.name),
            sample_literal(&field.java_type)
        );
    }
    out += "    }\n}\n";
    out
}

// ---- record: the plain-Java counterpart to `entity`, for projects with no
// Spring or JPA in sight (`new-cli` ones, mostly). Same field:type parsing,
// no annotations, and a compact constructor so an invalid value cannot be
// constructed in the first place. ----

fn record_java(pkg: &str, name: &str, fields: &[Field]) -> String {
    // Only reference components can be null, so only they need a check -- and
    // if none of them can, the compact constructor is dead weight.
    let checked: Vec<&Field> = fields.iter().filter(|f| is_reference_type(unboxed(&f.java_type))).collect();
    let needs_objects = !checked.is_empty();
    let mut imports: Vec<&str> = fields.iter().filter_map(|f| f.import).collect();
    if needs_objects {
        imports.push("java.util.Objects");
    }
    imports.sort();
    imports.dedup();

    let mut out = format!("package {pkg};\n\n");
    for imp in &imports {
        out += &format!("import {imp};\n");
    }
    if !imports.is_empty() {
        out += "\n";
    }

    let components = fields
        .iter()
        .map(|f| format!("{} {}", unboxed(&f.java_type), f.name))
        .collect::<Vec<_>>()
        .join(", ");

    out += "/**\n";
    out += &format!(" * An immutable {name} value.\n");
    out += " *\n";
    if needs_objects {
        out += " * <p>The compact constructor rejects nulls, so any instance that exists is\n";
        out += " * a valid one and callers downstream do not have to re-check.\n";
    } else {
        out += " * <p>Every component is a primitive, so there is nothing to validate: no\n";
        out += " * instance of this record can be in an invalid state.\n";
    }
    out += " */\n";
    out += &format!("public record {name}({components}) {{\n");

    if needs_objects {
        out += &format!("\n    public {name} {{\n");
        for field in &checked {
            out += &format!(
                "        Objects.requireNonNull({name}, \"{name}\");\n",
                name = field.name
            );
        }
        out += "    }\n";
    }

    out += "}\n";
    out
}

/// A companion test asserting the accessors return what was passed and that
/// the compact constructor actually rejects a null.
fn record_test(pkg: &str, name: &str, fields: &[Field]) -> String {
    let mut imports: Vec<&str> = fields.iter().filter_map(|f| f.import).collect();
    imports.sort();
    imports.dedup();

    let args = fields.iter().map(|f| sample_literal(&f.java_type).to_string()).collect::<Vec<_>>().join(", ");
    let var = name.to_lowercase();

    let mut out = format!("package {pkg};\n\n");
    out += "import org.junit.jupiter.api.Test;\n";
    if !imports.is_empty() {
        out += "\n";
        for imp in &imports {
            out += &format!("import {imp};\n");
        }
    }
    // Only a reference component can be passed null; against a primitive the
    // test would not compile.
    let first_reference = fields.iter().find(|f| is_reference_type(unboxed(&f.java_type)));

    out += "\nimport static org.assertj.core.api.Assertions.assertThat;\n";
    if first_reference.is_some() {
        out += "import static org.assertj.core.api.Assertions.assertThatNullPointerException;\n";
    }
    out += &format!("\nclass {name}Test {{\n\n");

    out += "    @Test\n    void accessorsReturnWhatWasConstructed() {\n";
    out += &format!("        {name} {var} = new {name}({args});\n\n");
    if fields.is_empty() {
        out += &format!("        assertThat({var}).isEqualTo(new {name}());\n");
    } else {
        for field in fields {
            out += &format!(
                "        assertThat({var}.{}()).isEqualTo({});\n",
                field.name,
                sample_literal(&field.java_type)
            );
        }
    }
    out += "    }\n";

    if let Some(first) = first_reference {
        // Only one component is nulled out: one case proves the compact
        // constructor runs, and a case per field would just restate it.
        let nulled = fields
            .iter()
            .map(|f| {
                if f.name == first.name {
                    "null".to_string()
                } else {
                    sample_literal(&f.java_type).to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        out += "\n    @Test\n    void rejectsANullComponent() {\n";
        out += &format!("        assertThatNullPointerException()\n");
        out += &format!("                .isThrownBy(() -> new {name}({nulled}))\n");
        out += &format!("                .withMessageContaining(\"{}\");\n", first.name);
        out += "    }\n";
    }

    out += "}\n";
    out
}

// ---- command: a CLI subcommand for `new-cli` projects, which otherwise get
// a Hello World `main` and no pattern for growing past it. ----

fn command_java(pkg: &str, name: &str) -> String {
    let word = name.to_lowercase();
    format!(
        r#"package {pkg};

import java.io.PrintStream;

/**
 * The {{@code {word}}} subcommand.
 *
 * <p>{{@link #run}} returns an exit code instead of calling
 * {{@code System.exit}}, and takes its output streams as arguments instead of
 * reaching for {{@code System.out}}. Both exist so a test can drive the whole
 * command in-process and assert on what it printed. Keep {{@code main}} the
 * only place that exits.
 *
 * <p>jails registered this in the project's dispatcher when it generated the
 * class, so {{@code {word}}} already works. If you need to do it by hand -- a
 * second dispatcher, or one jails could not find -- the line is:
 *
 * <pre>{{@code
 * commands.put({name}Command.NAME, {name}Command::run);
 * }}</pre>
 */
public final class {name}Command {{

    /** The word that selects this command on the command line. */
    public static final String NAME = "{word}";

    public static final String USAGE = "usage: {word} <argument>";

    /** Conventional exit code for "you invoked this wrong". */
    public static final int USAGE_ERROR = 2;

    private {name}Command() {{}}

    /** Runs the command, returning the exit code the process should end with. */
    public static int run(PrintStream out, PrintStream err, String... args) {{
        if (args.length != 1) {{
            err.println(USAGE);
            return USAGE_ERROR;
        }}

        out.println(args[0]);
        return 0;
    }}
}}
"#
    )
}

fn command_test(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;

import static org.assertj.core.api.Assertions.assertThat;

class {name}CommandTest {{

    private final ByteArrayOutputStream out = new ByteArrayOutputStream();
    private final ByteArrayOutputStream err = new ByteArrayOutputStream();

    private int run(String... args) {{
        return {name}Command.run(new PrintStream(out), new PrintStream(err), args);
    }}

    @Test
    void succeedsAndPrintsItsArgument() {{
        assertThat(run("hello")).isZero();
        assertThat(out.toString()).contains("hello");
        assertThat(err.toString()).isEmpty();
    }}

    @Test
    void reportsUsageOnStderrWhenCalledWithoutArguments() {{
        assertThat(run()).isEqualTo({name}Command.USAGE_ERROR);
        assertThat(err.toString()).contains({name}Command.USAGE);
        assertThat(out.toString()).isEmpty();
    }}

    @Test
    void rejectsTooManyArguments() {{
        assertThat(run("one", "two")).isEqualTo({name}Command.USAGE_ERROR);
    }}
}}
"#
    )
}

fn sample_literal(java_type: &str) -> &'static str {
    match java_type {
        "String" => "\"sample\"",
        "Integer" | "int" => "1",
        "Long" | "long" => "1L",
        "Boolean" | "boolean" => "true",
        "Double" | "double" => "1.0",
        "LocalDate" => "LocalDate.of(2024, 1, 1)",
        "LocalDateTime" => "LocalDateTime.of(2024, 1, 1, 12, 0)",
        _ => "null",
    }
}

// ---- scaffold's fuller service/controller/test (beyond the bare stubs) ----

fn service_full(pkg: &str, name: &str, extra: &str) -> String {
    let var = name.to_lowercase();
    format!(
        r#"package {pkg};
{extra}
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

fn controller_full(pkg: &str, name: &str, route: &str, extra: &str) -> String {
    let var = name.to_lowercase();
    format!(
        r#"package {pkg};
{extra}
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

fn controller_test(pkg: &str, name: &str, route: &str, mockmvc_import: &str, extra: &str) -> String {
    format!(
        r#"package {pkg};
{extra}
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

// ---- value: a record that not only rejects nulls (which `record` already
// does) but normalises and validates, so an instance is *meaningful*, not just
// non-null. Blank strings are the case that bites in practice -- a required
// identifier that is present but empty passes every null check downstream. ----

fn value_java(pkg: &str, name: &str, fields: &[Field]) -> String {
    let strings: Vec<&Field> = fields.iter().filter(|f| f.java_type == "String").collect();
    let checked: Vec<&Field> = fields.iter().filter(|f| is_reference_type(unboxed(&f.java_type))).collect();

    let mut imports: Vec<&str> = fields.iter().filter_map(|f| f.import).collect();
    if !checked.is_empty() {
        imports.push("java.util.Objects");
    }
    imports.sort();
    imports.dedup();

    let mut out = format!("package {pkg};\n\n");
    for imp in &imports {
        out += &format!("import {imp};\n");
    }
    out += "\n";

    let components = fields
        .iter()
        .map(|f| format!("{} {}", unboxed(&f.java_type), f.name))
        .collect::<Vec<_>>()
        .join(", ");
    let names = fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>().join(", ");

    out += "/**\n";
    out += &format!(" * A validated {name} value.\n");
    out += " *\n";
    out += " * <p>All validation lives in the compact constructor, which runs before the\n";
    out += " * components are assigned -- so there is no way to reach an instance that\n";
    out += " * skipped it, not even through deserialisation or a copy.\n";
    if !strings.is_empty() {
        out += " *\n";
        out += " * <p>Text is trimmed and then required to be non-blank: a present-but-empty\n";
        out += " * value passes every null check downstream, which is exactly why it is\n";
        out += " * worth rejecting here instead.\n";
    }
    out += " */\n";
    out += &format!("public record {name}({components}) {{\n\n");

    // Compact constructor: normalise first, then validate what normalising
    // produced, so " " fails the blank check rather than sneaking past it.
    out += &format!("    public {name} {{\n");
    for field in &checked {
        out += &format!("        Objects.requireNonNull({0}, \"{0} is required\");\n", field.name);
    }
    for field in &strings {
        out += &format!("        {0} = {0}.trim();\n", field.name);
        out += &format!(
            "        if ({0}.isEmpty()) {{\n            throw new IllegalArgumentException(\"{0} must not be blank\");\n        }}\n",
            field.name
        );
    }
    out += "    }\n\n";

    out += "    /**\n";
    out += &format!("     * Builds a {name}. Identical to the constructor today; it exists so that\n");
    out += "     * parsing, defaulting or a cache can be added later without changing a\n";
    out += "     * single call site.\n";
    out += "     */\n";
    out += &format!("    public static {name} of({components}) {{\n");
    out += &format!("        return new {name}({names});\n");
    out += "    }\n";
    out += "}\n";
    out
}

fn value_test(pkg: &str, name: &str, fields: &[Field]) -> String {
    let strings: Vec<&Field> = fields.iter().filter(|f| f.java_type == "String").collect();

    let mut imports: Vec<&str> = fields.iter().filter_map(|f| f.import).collect();
    imports.sort();
    imports.dedup();

    let args = fields.iter().map(|f| sample_literal(&f.java_type).to_string()).collect::<Vec<_>>().join(", ");
    // Same argument list, but with one named component swapped out.
    let args_with = |target: &str, replacement: &str| {
        fields
            .iter()
            .map(|f| {
                if f.name == target {
                    replacement.to_string()
                } else {
                    sample_literal(&f.java_type).to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    let mut out = format!("package {pkg};\n\n");
    out += "import org.junit.jupiter.api.Test;\n";
    if !imports.is_empty() {
        out += "\n";
        for imp in &imports {
            out += &format!("import {imp};\n");
        }
    }
    out += "\nimport static org.assertj.core.api.Assertions.assertThat;\n";
    out += "import static org.assertj.core.api.Assertions.assertThatThrownBy;\n\n";
    out += &format!("class {name}Test {{\n\n");

    out += "    @Test\n    void keepsWhatItWasGiven() {\n";
    out += &format!("        var value = {name}.of({args});\n\n");
    for field in fields {
        out += &format!(
            "        assertThat(value.{}()).isEqualTo({});\n",
            field.name,
            sample_literal(&field.java_type)
        );
    }
    out += "    }\n\n";

    // A primitive component cannot be handed null, so there is nothing to
    // assert about one -- and the generated test would not compile.
    if let Some(first) = fields.iter().find(|f| is_reference_type(unboxed(&f.java_type))) {
        out += "    @Test\n    void rejectsANullComponent() {\n";
        out += &format!(
            "        assertThatThrownBy(() -> {name}.of({}))\n                .isInstanceOf(NullPointerException.class)\n                .hasMessageContaining(\"{}\");\n",
            args_with(&first.name, "null"),
            first.name
        );
        out += "    }\n";
    }

    if let Some(text) = strings.first() {
        out += "\n    @Test\n    void trimsSurroundingWhitespace() {\n";
        out += &format!(
            "        assertThat({name}.of({}).{}()).isEqualTo(\"trimmed\");\n",
            args_with(&text.name, "\"  trimmed  \""),
            text.name
        );
        out += "    }\n";

        out += "\n    /** Blank is the failure a null check never catches. */\n";
        out += "    @Test\n    void rejectsBlankText() {\n";
        out += &format!(
            "        assertThatThrownBy(() -> {name}.of({}))\n                .isInstanceOf(IllegalArgumentException.class)\n                .hasMessageContaining(\"{}\");\n",
            args_with(&text.name, "\"   \""),
            text.name
        );
        out += "    }\n";
    }

    out += "}\n";
    out
}

// ---- cli: the dispatcher that `generate command` leaves you to write. ----

pub(crate) fn cli_java(pkg: &str, class: &str, program: &str) -> String {
    format!(
        r#"package {pkg};

import java.io.PrintStream;
import java.util.LinkedHashMap;
import java.util.SequencedMap;

/**
 * Argv dispatch for the {program} command line: it owns argument routing, exit
 * codes and streams, and nothing else.
 *
 * <p>The registry is a parameter of {{@link #run}}, not a static the method
 * reaches for. That is what lets a test drive the whole dispatcher with its own
 * commands, without a real one existing and without touching
 * {{@code System.out}}. {{@link #commands()}} is the one place to edit as you add
 * commands; {{@code main}} is the only place that exits.
 *
 * {{@snippet :
 * var out = new ByteArrayOutputStream();
 * int code = {class}.run({class}.commands(), new PrintStream(out), System.err, "greet", "world");
 * }}
 */
public final class {class} {{

    /**
     * One subcommand. Matches the shape {{@code jails generate command}} emits,
     * so {{@code SomethingCommand::run}} is a method reference away.
     */
    @FunctionalInterface
    public interface Command {{
        int run(PrintStream out, PrintStream err, String... args);
    }}

    /** Conventional exit code for "you invoked this wrong". */
    public static final int USAGE_ERROR = 2;

    private {class}() {{}}

    /**
     * The commands this CLI answers to, in the order they should be listed.
     *
     * <p>Add yours here -- a {{@code SequencedMap}} because help output that
     * reorders itself between runs is a diff nobody wants:
     *
     * {{@snippet :
     * commands.put(ImportCommand.NAME, ImportCommand::run);
     * }}
     */
    public static SequencedMap<String, Command> commands() {{
        var commands = new LinkedHashMap<String, Command>();
        return commands;
    }}

    /** Runs one invocation and returns the exit code the process should end with. */
    public static int run(SequencedMap<String, Command> commands, PrintStream out, PrintStream err, String... args) {{
        var name = args.length == 0 ? "help" : args[0];

        if (name.equals("help") || name.equals("--help") || name.equals("-h")) {{
            usage(commands, out);
            return 0;
        }}

        var command = commands.get(name);
        if (command == null) {{
            err.println("unknown command: " + name);
            usage(commands, err);
            return USAGE_ERROR;
        }}

        // Everything after the command word belongs to the command itself.
        var rest = new String[args.length - 1];
        System.arraycopy(args, 1, rest, 0, rest.length);
        return command.run(out, err, rest);
    }}

    private static void usage(SequencedMap<String, Command> commands, PrintStream to) {{
        to.println("usage: {program} <command> [args]");
        to.println();
        to.println("commands:");
        to.println("  help");
        commands.keySet().forEach(name -> to.println("  " + name));
    }}

    public static void main(String[] args) {{
        System.exit(run(commands(), System.out, System.err, args));
    }}
}}
"#,
        program = program,
    )
}

pub(crate) fn cli_test(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.util.LinkedHashMap;
import java.util.SequencedMap;

import static org.assertj.core.api.Assertions.assertThat;

class {class}Test {{

    private final ByteArrayOutputStream out = new ByteArrayOutputStream();
    private final ByteArrayOutputStream err = new ByteArrayOutputStream();

    /**
     * A registry of test doubles. Because {{@code run}} takes the commands as an
     * argument, the dispatcher is testable on its own -- these assertions hold
     * before a single real command exists.
     */
    private SequencedMap<String, {class}.Command> commands() {{
        var commands = new LinkedHashMap<String, {class}.Command>();
        commands.put("echo", (out, err, args) -> {{
            out.println(String.join(" ", args));
            return 0;
        }});
        commands.put("boom", (out, err, args) -> {{
            err.println("failed");
            return 1;
        }});
        return commands;
    }}

    private int run(String... args) {{
        return {class}.run(commands(), new PrintStream(out), new PrintStream(err), args);
    }}

    @Test
    void routesToTheNamedCommandAndPassesTheRestOfArgv() {{
        assertThat(run("echo", "hello", "world")).isZero();
        assertThat(out.toString()).contains("hello world");
    }}

    @Test
    void returnsWhateverTheCommandReturned() {{
        assertThat(run("boom")).isEqualTo(1);
        assertThat(err.toString()).contains("failed");
    }}

    @Test
    void listsEveryCommandInHelp() {{
        assertThat(run("help")).isZero();
        assertThat(out.toString()).contains("echo").contains("boom");
    }}

    @Test
    void treatsNoArgumentsAsHelpRatherThanAnError() {{
        assertThat(run()).isZero();
        assertThat(out.toString()).contains("usage:");
    }}

    @Test
    void namesTheUnknownCommandAndExitsTwo() {{
        assertThat(run("nope")).isEqualTo({class}.USAGE_ERROR);
        assertThat(err.toString()).contains("nope");
    }}

    /** Help ordering is part of the contract, hence SequencedMap. */
    @Test
    void listsCommandsInRegistrationOrder() {{
        run("help");
        var text = out.toString();
        assertThat(text.indexOf("echo")).isLessThan(text.indexOf("boom"));
    }}
}}
"#
    )
}

// ---- registering a generated command with the dispatcher ----

/// Splice `commands.put(FooCommand.NAME, FooCommand::run);` into the
/// project's `*Cli.java`.
///
/// jails' rule used to be that only `pom.rs` edits a file the user owns, and
/// so `generate command` merely *documented* the dispatch line for you to
/// paste. But that rule was always a proxy for the real one -- an edit must be
/// surgical and leave every other byte alone -- and pasting a line by hand
/// after every single `generate` is exactly the plumbing this tool exists to
/// remove. The splice is idempotent and touches one line inside one method.
///
/// No dispatcher, or more than one, means jails cannot know where it goes: it
/// says so and leaves the Javadoc instructions as the fallback.
fn register_command(root: &Path, base: &str, name: &str) -> Result<()> {
    let dispatchers = find_dispatchers(&root.join("src/main/java"));
    let dispatcher = match dispatchers.as_slice() {
        [one] => one,
        [] => {
            println!(
                "note: no *Cli.java dispatcher found -- see {name}Command's Javadoc for the dispatch line,\n      \
                 or run `jails generate cli <Name>` to get one that registers commands for you"
            );
            return Ok(());
        }
        many => {
            println!(
                "note: {} dispatchers found, so {name}Command was not registered automatically -- add it to the one you meant",
                many.len()
            );
            return Ok(());
        }
    };

    let source = fs::read_to_string(dispatcher).map_err(|e| format!("failed to read {}: {e}", dispatcher.display()))?;
    let command_class = format!("{name}Command");
    // Scoped to the registry body, not the whole file: the dispatcher's own
    // Javadoc shows an example `commands.put(...)` line, and a whole-file
    // `contains` matched *that* -- so generating a command with the same name
    // as the example silently skipped registration.
    if registry_body(&source).is_some_and(|body| body.contains(&format!("{command_class}::run"))) {
        println!("  exists  {command_class} is already registered in {}", dispatcher.display());
        return Ok(());
    }

    // The dispatcher and the command can be in different packages once
    // `--package` is involved, so the registration may need an import too.
    let dispatcher_pkg = package_of(&source).unwrap_or_else(|| base.to_string());
    let command_pkg = subpackage(base, layout::CLI);

    let Some(spliced) = splice_registration(&source, &command_class, &import_of(&dispatcher_pkg, &command_pkg, &command_class)) else {
        println!(
            "note: could not find the `return commands;` line in {} -- add {command_class} by hand",
            dispatcher.display()
        );
        return Ok(());
    };

    fs::write(dispatcher, spliced).map_err(|e| format!("failed to write {}: {e}", dispatcher.display()))?;
    println!("registered {command_class} in {}", dispatcher.display());
    Ok(())
}

/// Every dispatcher under the source root.
///
/// Recognised by shape, not by filename: `new-cli` writes one called
/// `App.java` and `generate cli` writes one called `<Name>Cli.java`, and both
/// have to be findable. A file merely *named* like one is not enough to edit.
fn find_dispatchers(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "java") {
                if fs::read_to_string(&path).map(|s| is_dispatcher(&s)).unwrap_or(false) {
                    found.push(path);
                }
            }
        }
    }
    found.sort();
    found
}

/// The statements inside `commands()`, between the map's creation and the
/// `return` -- the only region where a registration counts.
fn registry_body(source: &str) -> Option<&str> {
    let anchor = source.find("return commands;")?;
    let start = source[..anchor].rfind("new LinkedHashMap")?;
    Some(&source[start..anchor])
}

/// What makes a file a jails command dispatcher: the registry type it
/// dispatches over, and the line `register_command` splices above. Both are
/// checked, because either alone shows up in files that are not dispatchers.
pub(crate) fn is_dispatcher(source: &str) -> bool {
    source.contains("SequencedMap<String, Command>") && source.contains("return commands;")
}

fn package_of(source: &str) -> Option<String> {
    source
        .lines()
        .find_map(|line| line.trim().strip_prefix("package ")?.trim().strip_suffix(';').map(|s| s.trim().to_string()))
}

/// Insert the registration immediately above `return commands;`, matching that
/// line's indentation, and add `import` if the command lives elsewhere.
/// Returns `None` when the anchor is missing, so the caller can say so rather
/// than write a mangled file.
fn splice_registration(source: &str, command_class: &str, import: &str) -> Option<String> {
    let anchor = source.find("return commands;")?;
    let line_start = source[..anchor].rfind('\n').map(|i| i + 1)?;
    let indent: String = source[line_start..anchor].to_string();

    let mut out = String::with_capacity(source.len() + import.len() + 96);
    out.push_str(&source[..line_start]);
    out.push_str(&format!("{indent}commands.put({command_class}.NAME, {command_class}::run);\n"));
    out.push_str(&source[line_start..]);

    if import.is_empty() {
        return Some(out);
    }
    // Imports go after the package line; ordering is the normaliser's problem,
    // but this file already exists, so re-sort it here too.
    let package_end = out.find(";\n").map(|i| i + 2)?;
    let mut with_import = String::with_capacity(out.len() + import.len());
    with_import.push_str(&out[..package_end]);
    with_import.push('\n');
    with_import.push_str(import);
    with_import.push_str(&out[package_end..]);
    Some(normalize_imports(&with_import))
}

// ---- cases: a markdown checklist in, a pending JUnit class out. ----

/// Turn a brief's checklist into a `@Disabled` test class -- the todo list you
/// delete one `@Disabled` at a time.
fn generate_cases(root: &Path, pkg: &str, brief: &Path) -> Result<()> {
    let text = fs::read_to_string(brief).map_err(|e| format!("failed to read {}: {e}", brief.display()))?;
    let cases = parse_cases(&text);
    if cases.is_empty() {
        return Err(format!(
            "no list items found in {} -- `generate cases` turns markdown bullets into test cases",
            brief.display()
        ));
    }

    let class = cases_class_name(brief)?;
    let path = test_dir(root, pkg).join(format!("{class}.java"));
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    write_new_file(&path, &cases_java(pkg, &class, brief, &cases))?;
    println!("created cases {} ({} case{})", path.display(), cases.len(), if cases.len() == 1 { "" } else { "s" });
    Ok(())
}

/// Bullets under an acceptance/criteria/cases/checklist heading if the brief
/// has one, otherwise every bullet in the file.
///
/// Deliberately the whole of the markdown support: a heading and a bullet. The
/// moment this grows a second rule it starts being a markdown parser, and that
/// is not what jails is.
fn parse_cases(markdown: &str) -> Vec<String> {
    let scoped = cases_section(markdown);
    let source = scoped.as_deref().unwrap_or(markdown);

    let mut cases = Vec::new();
    let mut in_fence = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(item) = list_item(trimmed) {
            let cleaned = clean_markdown(item);
            if !cleaned.is_empty() {
                cases.push(cleaned);
            }
        }
    }
    cases
}

/// The body under the first heading that looks like a list of expectations,
/// up to the next heading of the same or a higher level.
fn cases_section(markdown: &str) -> Option<String> {
    const MARKERS: [&str; 5] = ["acceptance", "criteria", "cases", "checklist", "requirements"];

    let mut lines = markdown.lines().enumerate();
    let (start, level) = loop {
        let (i, line) = lines.next()?;
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#') {
            continue;
        }
        let level = trimmed.chars().take_while(|c| *c == '#').count();
        let title = trimmed[level..].to_lowercase();
        if MARKERS.iter().any(|m| title.contains(m)) {
            break (i + 1, level);
        }
    };

    let body: Vec<&str> = markdown
        .lines()
        .skip(start)
        .take_while(|line| {
            let trimmed = line.trim_start();
            let depth = trimmed.chars().take_while(|c| *c == '#').count();
            depth == 0 || depth > level
        })
        .collect();
    Some(body.join("\n"))
}

/// The content of a `-`/`*`/`1.` list item, checkbox marker stripped.
fn list_item(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
        .or_else(|| {
            let digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
            (!digits.is_empty()).then(|| line[digits.len()..].strip_prefix(". ")).flatten()
        })?;
    let rest = rest.trim();
    // `- [ ]` / `- [x]` checkboxes: the box is not part of the case.
    let rest = rest.strip_prefix("[ ]").or_else(|| rest.strip_prefix("[x]")).or_else(|| rest.strip_prefix("[X]")).unwrap_or(rest);
    Some(rest.trim())
}

/// Strip the inline markup that would otherwise end up inside a `@DisplayName`
/// string: emphasis, code ticks, and link syntax (keeping the link text).
fn clean_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '_' | '`' | '#' => {}
            '[' => {}
            ']' => {
                // `](url)` -- drop the target, keep the text already emitted.
                if chars.peek() == Some(&'(') {
                    for skipped in chars.by_ref() {
                        if skipped == ')' {
                            break;
                        }
                    }
                }
            }
            '"' => out.push('\''),
            '\\' => out.push('/'),
            _ => out.push(c),
        }
    }
    out.trim().to_string()
}

/// `01-normalise.md` -> `Workout01NormaliseTest`? No -- `Normalise01Test` would
/// be a guess. The stem is turned into a class name verbatim (minus the
/// separators), with a leading `Case` when it starts with a digit, since a Java
/// identifier cannot.
fn cases_class_name(brief: &Path) -> Result<String> {
    let stem = brief
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("{} has no file name to derive a class from", brief.display()))?;

    let mut class = String::new();
    let mut capitalize_next = true;
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() {
            if capitalize_next {
                class.extend(c.to_uppercase());
                capitalize_next = false;
            } else {
                class.push(c);
            }
        } else {
            capitalize_next = true;
        }
    }
    if class.is_empty() {
        return Err(format!("cannot derive a class name from {}", brief.display()));
    }
    if class.starts_with(|c: char| c.is_ascii_digit()) {
        class.insert_str(0, "Case");
    }
    if !class.ends_with("Test") {
        class.push_str("Test");
    }
    Ok(class)
}

/// A markdown bullet as a Java method name: camelCase, alphanumerics only.
fn case_method_name(case: &str) -> String {
    let mut name = String::new();
    let mut capitalize_next = false;
    for c in case.chars() {
        if c.is_ascii_alphanumeric() {
            if capitalize_next && !name.is_empty() {
                name.extend(c.to_uppercase());
            } else if name.is_empty() {
                name.extend(c.to_lowercase());
            } else {
                name.push(c);
            }
            capitalize_next = false;
        } else {
            capitalize_next = true;
        }
    }
    if name.is_empty() {
        name.push_str("case");
    }
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert(0, 'c');
    }
    name
}

fn cases_java(pkg: &str, class: &str, brief: &Path, cases: &[String]) -> String {
    let mut out = format!("package {pkg};\n\n");
    out += "import org.junit.jupiter.api.Disabled;\n";
    out += "import org.junit.jupiter.api.DisplayName;\n";
    out += "import org.junit.jupiter.api.Test;\n\n";
    out += "/**\n";
    out += &format!(" * Pending cases generated from {}.\n", brief.display());
    out += " *\n";
    out += " * <p>This is a todo list the build can read: every case fails loudly rather\n";
    out += " * than passing vacuously, and the class-level @Disabled keeps the suite green\n";
    out += " * meanwhile. Delete one @Disabled, make that case pass, move to the next.\n";
    out += " */\n";
    out += &format!("@DisplayName(\"{}\")\n", clean_markdown(&brief.file_stem().unwrap_or_default().to_string_lossy()));
    out += "@Disabled(\"todo: implement these cases\")\n";
    out += &format!("class {class} {{\n");

    // Two bullets can easily reduce to the same identifier; a suffix keeps the
    // class compiling rather than silently dropping a case.
    let mut seen: Vec<String> = Vec::new();
    for case in cases {
        let base = case_method_name(case);
        let mut method = base.clone();
        let mut n = 2;
        while seen.contains(&method) {
            method = format!("{base}{n}");
            n += 1;
        }
        seen.push(method.clone());

        out += "\n    @Test\n";
        out += &format!("    @DisplayName(\"{case}\")\n");
        out += &format!("    void {method}() {{\n");
        out += "        throw new UnsupportedOperationException(\"todo\");\n";
        out += "    }\n";
    }
    out += "}\n";
    out
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
    fn record_java_emits_a_record_with_a_null_rejecting_compact_constructor() {
        let fields = parse_fields(&["amount:long".to_string(), "currency:string".to_string()]).unwrap();
        let src = record_java("com.example.demo", "Money", &fields);

        // Primitive components, not the boxed types the entity table uses: a
        // `long` cannot be null, so it needs neither the box nor the check.
        assert!(src.contains("public record Money(long amount, String currency) {"), "{src}");
        assert!(src.contains("public Money {"), "expected a compact constructor");
        assert!(!src.contains("requireNonNull(amount"), "a primitive cannot be null");
        assert!(src.contains(r#"Objects.requireNonNull(currency, "currency");"#));
        // The plain-Java counterpart to `entity`: no Spring, no JPA.
        for forbidden in ["jakarta.persistence", "@Entity", "@Id", "org.springframework"] {
            assert!(!src.contains(forbidden), "{forbidden} should not appear in a plain record");
        }
    }

    /// A record whose components are all primitives cannot hold a null, so the
    /// compact constructor would be empty -- and an empty one is noise.
    #[test]
    fn record_java_omits_the_compact_constructor_when_every_component_is_primitive() {
        let fields = parse_fields(&["amount:long".to_string(), "count:int".to_string()]).unwrap();
        let src = record_java("com.example.demo", "Tally", &fields);

        assert!(src.contains("public record Tally(long amount, int count) {"), "{src}");
        assert!(!src.contains("public Tally {"), "nothing to validate: {src}");
        assert!(!src.contains("import java.util.Objects;"));
    }

    /// A no-field record has nothing to null-check, so the compact constructor
    /// (and the Objects import that only exists to serve it) must be omitted
    /// rather than emitted empty.
    #[test]
    fn record_java_omits_the_compact_constructor_when_there_are_no_fields() {
        let src = record_java("com.example.demo", "Marker", &[]);

        assert!(src.contains("public record Marker() {"));
        assert!(!src.contains("public Marker {"));
        assert!(!src.contains("import java.util.Objects;"));
    }

    #[test]
    fn record_java_sorts_time_imports_with_the_objects_import() {
        let fields = parse_fields(&["on:date".to_string()]).unwrap();
        let src = record_java("com.example.demo", "Entry", &fields);

        let time = src.find("import java.time.LocalDate;").unwrap();
        let objects = src.find("import java.util.Objects;").unwrap();
        assert!(time < objects, "java.time should sort before java.util");
    }

    #[test]
    fn record_test_covers_the_accessors_and_the_null_rejection() {
        let fields = parse_fields(&["amount:long".to_string(), "currency:string".to_string()]).unwrap();
        let test = record_test("com.example.demo", "Money", &fields);

        assert!(test.contains("class MoneyTest"));
        assert!(test.contains("new Money(1L, \"sample\")"));
        assert!(test.contains("assertThat(money.amount()).isEqualTo(1L);"));
        assert!(test.contains("assertThatNullPointerException()"));
        // `amount` is a primitive now, so the null case has to target the first
        // *reference* component or the generated test would not compile.
        assert!(test.contains("new Money(1L, null)"), "{test}");
    }

    /// With no fields there is no null to reject, so the test that asserts the
    /// rejection would not compile -- it must not be emitted.
    #[test]
    fn record_test_skips_the_null_case_for_a_no_field_record() {
        let test = record_test("com.example.demo", "Marker", &[]);

        assert!(!test.contains("assertThatNullPointerException"));
        assert!(!test.contains("import static org.assertj.core.api.Assertions.assertThatNullPointerException;"));
        assert!(test.contains("new Marker()"));
    }

    #[test]
    fn command_java_returns_an_exit_code_and_never_exits_the_process() {
        let src = command_java("com.example.demo", "Greet");

        assert!(src.contains("public final class GreetCommand"));
        assert!(src.contains(r#"public static final String NAME = "greet";"#));
        assert!(src.contains("public static int run(PrintStream out, PrintStream err, String... args)"));
        // A CLI command has no business depending on Spring.
        assert!(!src.contains("org.springframework"));

        // The whole point: main owns the exit, so the command stays testable
        // in-process, and output goes to injected streams, not System.out.
        // Only the class body is checked -- the Javadoc deliberately shows a
        // `main` that does call System.exit, since that is where it belongs.
        let body = &src[src.find("public final class").unwrap()..];
        assert!(!body.contains("System.exit"), "run() must not exit the process");
        assert!(!body.contains("System.out"), "output should go to the injected stream");
    }

    #[test]
    fn command_test_drives_the_command_through_captured_streams() {
        let test = command_test("com.example.demo", "Greet");

        assert!(test.contains("class GreetCommandTest"));
        assert!(test.contains("ByteArrayOutputStream"));
        assert!(test.contains("GreetCommand.run(new PrintStream(out), new PrintStream(err), args)"));
        assert!(test.contains("GreetCommand.USAGE_ERROR"));
    }

    #[test]
    fn stub_templates_use_the_package_and_class_name() {
        assert!(stub_controller("com.example.blog", "Post").contains("public class PostController"));
        assert!(stub_service("com.example.blog", "Post").contains("public class PostService"));
        assert!(stub_repository("com.example.blog", "Post", "").contains("extends JpaRepository<Post, Long>"));
        assert!(stub_test("com.example.blog", "Post").contains("class PostTest"));
    }

    #[test]
    fn service_full_wraps_repository_crud() {
        let src = service_full("com.example.blog", "Post", "");
        assert!(src.contains("findAll()"));
        assert!(src.contains("findById(Long id)"));
        assert!(src.contains("save(Post post)"));
        assert!(src.contains("deleteById(Long id)"));
        assert!(src.contains("existsById(id)"));
    }

    #[test]
    fn controller_full_exposes_full_crud_routes() {
        let src = controller_full("com.example.blog", "Post", "posts", "");
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
        let result = generate(ArtifactKind::Scaffold, "post", &["title:string".to_string()], None);
        std::env::set_current_dir(original_cwd).unwrap();
        result.unwrap();

        assert!(root.join("src/main/java/com/example/blog/domain/Post.java").is_file());
        assert!(root.join("src/test/java/com/example/blog/domain/PostTest.java").is_file());
        assert!(root.join("src/main/java/com/example/blog/repository/PostRepository.java").is_file());
        assert!(root.join("src/main/java/com/example/blog/service/PostService.java").is_file());
        assert!(root.join("src/main/java/com/example/blog/web/PostController.java").is_file());
        assert!(root.join("src/test/java/com/example/blog/web/PostControllerTest.java").is_file());

        // Crossing a package boundary costs an import; the scaffold has to pay it.
        let service = fs::read_to_string(root.join("src/main/java/com/example/blog/service/PostService.java")).unwrap();
        assert!(service.contains("import com.example.blog.domain.Post;"), "{service}");
        assert!(service.contains("import com.example.blog.repository.PostRepository;"), "{service}");
    }

    /// Regression test: standalone `generate controller` used to write only
    /// the bare stub, unlike Rails (`rails generate controller` always
    /// emits a matching test).
    #[test]
    fn generate_controller_also_creates_a_controller_test() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("controller-test-companion");
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
        let result = generate(ArtifactKind::Controller, "health", &[], None);
        std::env::set_current_dir(original_cwd).unwrap();
        result.unwrap();

        assert!(root.join("src/main/java/com/example/blog/web/HealthController.java").is_file());
        let test_file = root.join("src/test/java/com/example/blog/web/HealthControllerTest.java");
        assert!(test_file.is_file(), "expected {}", test_file.display());
        assert!(fs::read_to_string(test_file).unwrap().contains("class HealthControllerTest"));
    }

    #[test]
    fn generate_service_also_creates_a_service_test() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("service-test-companion");
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
        let result = generate(ArtifactKind::Service, "billing", &[], None);
        std::env::set_current_dir(original_cwd).unwrap();
        result.unwrap();

        assert!(root.join("src/main/java/com/example/blog/service/BillingService.java").is_file());
        assert!(root.join("src/test/java/com/example/blog/service/BillingServiceTest.java").is_file());
    }

    #[test]
    fn generate_repository_creates_no_companion_test() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("repository-no-test");
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
        let result = generate(ArtifactKind::Repository, "widget", &[], None);
        std::env::set_current_dir(original_cwd).unwrap();
        result.unwrap();

        assert!(root.join("src/main/java/com/example/blog/repository/WidgetRepository.java").is_file());
        assert!(!root.join("src/test/java/com/example/blog/repository/WidgetRepositoryTest.java").exists());
    }

    /// `record` and `command` target plain Maven projects, whose entry point
    /// is App.java rather than *Application.java -- the case base_package()
    /// falls back for.
    #[test]
    fn generate_record_and_command_work_in_a_plain_cli_project() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("plain-record-command");
        let src = root.join("src/main/java/com/example/demo");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(src.join("App.java"), "package com.example.demo;\n\npublic class App {}\n").unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let record = generate(ArtifactKind::Record, "money", &["amount:long".to_string()], None);
        let command = generate(ArtifactKind::Command, "greet", &[], None);
        std::env::set_current_dir(original_cwd).unwrap();
        record.unwrap();
        command.unwrap();

        assert!(root.join("src/main/java/com/example/demo/domain/Money.java").is_file());
        assert!(root.join("src/test/java/com/example/demo/domain/MoneyTest.java").is_file());
        assert!(root.join("src/main/java/com/example/demo/cli/GreetCommand.java").is_file());
        assert!(root.join("src/test/java/com/example/demo/cli/GreetCommandTest.java").is_file());
    }

    #[test]
    fn destroy_command_removes_both_of_its_files() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("destroy-command");
        let src = root.join("src/main/java/com/example/demo");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(src.join("App.java"), "package com.example.demo;\n\npublic class App {}\n").unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        generate(ArtifactKind::Command, "greet", &[], None).unwrap();
        let result = destroy(ArtifactKind::Command, "greet", true, None);
        std::env::set_current_dir(original_cwd).unwrap();

        result.unwrap();
        assert!(!src.join("GreetCommand.java").exists());
        assert!(!root.join("src/test/java/com/example/demo/GreetCommandTest.java").exists());
        assert!(src.join("App.java").is_file());
    }

    /// A record and an entity are the same named type in two shapes, so
    /// generating one and destroying "the other" clears the same two paths --
    /// and `generate` still refuses to write over either.
    #[test]
    fn record_and_entity_occupy_the_same_paths() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("record-entity-paths");
        let src = root.join("src/main/java/com/example/demo");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(src.join("App.java"), "package com.example.demo;\n\npublic class App {}\n").unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        generate(ArtifactKind::Record, "tag", &["name:string".to_string()], None).unwrap();
        let clash = generate(ArtifactKind::Entity, "tag", &["name:string".to_string()], None);
        let result = destroy(ArtifactKind::Record, "tag", true, None);
        std::env::set_current_dir(original_cwd).unwrap();

        assert!(clash.is_err(), "generate must not overwrite the record with an entity");
        result.unwrap();
        assert!(!src.join("Tag.java").exists());
        assert!(!root.join("src/test/java/com/example/demo/TagTest.java").exists());
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
        let web = root.join("src/main/java/com/example/blog/web");
        fs::create_dir_all(&web).unwrap();
        fs::write(web.join("CommentController.java"), "// already here").unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let result = generate(ArtifactKind::Controller, "comment", &[], None);
        std::env::set_current_dir(original_cwd).unwrap();

        assert!(result.is_err());
        assert_eq!(fs::read_to_string(web.join("CommentController.java")).unwrap(), "// already here");
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
        generate(ArtifactKind::Entity, "tag", &["name:string".to_string()], None).unwrap();
        let result = destroy(ArtifactKind::Entity, "tag", true, None);
        std::env::set_current_dir(original_cwd).unwrap();

        result.unwrap();
        assert!(!src.join("Tag.java").is_file());
        assert!(!root.join("src/test/java/com/example/blog/TagTest.java").exists());
        assert!(src.join("BlogApplication.java").is_file());
    }
}
