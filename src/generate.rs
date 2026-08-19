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
    Class,
    Interface,
    Record,
    Value,
    Enum,
    Sealed,
    #[value(alias = "repository")]
    Repo,
    #[value(alias = "mig")]
    Migration,
    Handler,
    Command,
    Cli,
    Cases,
    Test,
    #[value(name = "integration-test", alias = "it")]
    IntegrationTest,
}

pub struct Field {
    pub name: String,
    pub java_type: String,
    pub imports: Vec<&'static str>,
    pub optionality: Optionality,
    /// True when the type came from the project rather than the built-in
    /// table, so jails knows the shape of exactly nothing about it.
    pub owned: bool,
    /// A `List` or `Map` component: copied defensively and defaulted to empty
    /// rather than null-checked.
    pub collection: bool,
}

/// What a `!` or `?` suffix on a field type means.
///
/// Hardcoding one policy is what made `value` reject every blank string,
/// including the description fields where blank is perfectly legal. Every
/// value type in every project has this distinction, so it belongs in the
/// syntax rather than in jails' opinion.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Optionality {
    /// `name:string` -- must not be null.
    Required,
    /// `name:string!` -- must not be null, and must not be blank.
    NonBlank,
    /// `name:string?` -- may be null; nothing is checked.
    Nullable,
}

/// One resolved type: how to spell it in Java, and what it needs imported.
struct Resolved {
    java_type: String,
    imports: Vec<&'static str>,
    owned: bool,
    collection: bool,
}

/// Resolve a type token, recursing through `list<...>` and `map<..,..>`.
///
/// Recursion is what makes the collection types worth having: `list<Match>`
/// and `map<string,double>` cost nothing extra once the element goes through
/// the same resolver as a bare field.
fn resolve_type(token: &str) -> Result<Resolved> {
    let token = token.trim();

    if let Some(inner) = generic_argument(token, "list") {
        let element = resolve_element(inner, token)?;
        let mut imports = element.imports;
        imports.push("java.util.List");
        return Ok(Resolved {
            java_type: format!("List<{}>", boxed(&element.java_type)),
            imports,
            owned: false,
            collection: true,
        });
    }

    if let Some(inner) = generic_argument(token, "map") {
        let (key, value) = inner.split_once(',').ok_or_else(|| {
            format!("'{token}' needs a key and a value type, e.g. map<string,double>")
        })?;
        let key = resolve_element(key, token)?;
        let value = resolve_element(value, token)?;
        let mut imports = key.imports;
        imports.extend(value.imports);
        imports.push("java.util.Map");
        return Ok(Resolved {
            java_type: format!(
                "Map<{}, {}>",
                boxed(&key.java_type),
                boxed(&value.java_type)
            ),
            imports,
            owned: false,
            collection: true,
        });
    }

    // The Java spellings of the built-ins, so `date:LocalDate` and `date:date`
    // mean the same thing and `id:String` is not read as a project type.
    if let Some((java_type, import)) = builtin_by_java_name(token) {
        return Ok(Resolved {
            java_type: java_type.to_string(),
            imports: import.into_iter().collect(),
            owned: false,
            collection: false,
        });
    }

    // Case is the whole rule: capitalised means a type this project owns.
    if token.starts_with(|c: char| c.is_uppercase()) {
        return Ok(Resolved {
            java_type: token.to_string(),
            imports: Vec::new(),
            owned: true,
            collection: false,
        });
    }

    let lower = token.to_lowercase();
    if lower == "list" || lower == "map" {
        return Err(format!(
            "'{token}' needs its element type(s) -- list<string>, list<Match>, map<string,double>"
        ));
    }

    let (java_type, import) = field_type(&lower)?;
    Ok(Resolved {
        java_type: java_type.to_string(),
        imports: import.into_iter().collect(),
        owned: false,
        collection: false,
    })
}

/// A collection's element type, with a message that names the collection it
/// came from -- `unknown field type 'nope'` alone is not much help when it
/// came out of `list<nope>`.
fn resolve_element(token: &str, outer: &str) -> Result<Resolved> {
    let token = token.trim();
    if token.is_empty() {
        return Err(format!("'{outer}' is missing an element type"));
    }
    let resolved = resolve_type(token).map_err(|e| format!("in '{outer}': {e}"))?;
    if resolved.collection {
        return Err(format!(
            "'{outer}': nested collections are not supported -- introduce a type for the inner one"
        ));
    }
    Ok(resolved)
}

/// The text inside `name<...>`, if the token is that shape. A bare `list` has
/// no element type and is meaningless, so it is not matched here and falls
/// through to the unknown-type error.
fn generic_argument<'a>(token: &'a str, name: &str) -> Option<&'a str> {
    token
        .strip_prefix(name)?
        .strip_prefix('<')?
        .strip_suffix('>')
}

fn field_type(token: &str) -> Result<(&'static str, Option<&'static str>)> {
    match token {
        "string" | "text" => Ok(("String", None)),
        "int" | "integer" => Ok(("Integer", None)),
        "long" => Ok(("Long", None)),
        "boolean" => Ok(("Boolean", None)),
        "date" => Ok(("LocalDate", Some("java.time.LocalDate"))),
        "datetime" => Ok(("LocalDateTime", Some("java.time.LocalDateTime"))),
        "instant" => Ok(("Instant", Some("java.time.Instant"))),
        "uuid" => Ok(("UUID", Some("java.util.UUID"))),
        "currency" => Ok(("Currency", Some("java.util.Currency"))),
        "bigdecimal" | "decimal" => Ok(("BigDecimal", Some("java.math.BigDecimal"))),
        "bytes" => Ok(("byte[]", None)),
        "duration" => Ok(("Duration", Some("java.time.Duration"))),
        "zone-id" | "zoneid" => Ok(("ZoneId", Some("java.time.ZoneId"))),
        "uri" => Ok(("URI", Some("java.net.URI"))),
        "path" => Ok(("Path", Some("java.nio.file.Path"))),
        "double" => Ok(("Double", None)),
        other => Err(format!(
            "unknown field type '{other}' (known: string, text, int/integer, long, boolean, date, datetime, instant, uuid, currency, decimal, bytes, duration, zone-id, uri, path, double, list<T>, map<K,V>).\n       \
             Capitalise it -- {}:{} -- to mean a type this project owns.",
            other,
            capitalize(other)
        )),
    }
}

/// The Java spellings of the built-in table, so `date:LocalDate` and
/// `date:date` mean the same thing.
fn builtin_by_java_name(ty: &str) -> Option<(&'static str, Option<&'static str>)> {
    match ty {
        "String" => Some(("String", None)),
        "Integer" | "int" => Some(("Integer", None)),
        "Long" | "long" => Some(("Long", None)),
        "Boolean" | "boolean" => Some(("Boolean", None)),
        "Double" | "double" => Some(("Double", None)),
        "LocalDate" => Some(("LocalDate", Some("java.time.LocalDate"))),
        "LocalDateTime" => Some(("LocalDateTime", Some("java.time.LocalDateTime"))),
        "Instant" => Some(("Instant", Some("java.time.Instant"))),
        "UUID" => Some(("UUID", Some("java.util.UUID"))),
        "BigDecimal" => Some(("BigDecimal", Some("java.math.BigDecimal"))),
        "Duration" => Some(("Duration", Some("java.time.Duration"))),
        "ZoneId" => Some(("ZoneId", Some("java.time.ZoneId"))),
        "URI" => Some(("URI", Some("java.net.URI"))),
        "Path" => Some(("Path", Some("java.nio.file.Path"))),
        _ => None,
    }
}

fn parse_fields(args: &[String]) -> Result<Vec<Field>> {
    args.iter()
        .map(|arg| {
            let (name, ty) = arg
                .split_once(':')
                .ok_or_else(|| format!("field '{arg}' must be name:type"))?;

            let ty = ty.trim();
            let (ty, optionality) = match ty.strip_suffix('!') {
                Some(rest) => (rest, Optionality::NonBlank),
                None => match ty.strip_suffix('?') {
                    Some(rest) => (rest, Optionality::Nullable),
                    None => (ty, Optionality::Required),
                },
            };
            if ty.is_empty() {
                return Err(format!("field '{arg}' has a suffix but no type"));
            }

            let resolved = resolve_type(ty)?;
            if optionality == Optionality::NonBlank && resolved.java_type != "String" {
                return Err(format!(
                    "'{arg}': the '!' suffix means non-blank, which only applies to text -- \
                     drop it, or use '{}:{ty}' if you only meant required",
                    name.trim()
                ));
            }
            if optionality == Optionality::Nullable && resolved.collection {
                return Err(format!(
                    "'{arg}': a collection already models absence as an empty one -- drop the '?'"
                ));
            }

            Ok(Field {
                name: name.trim().to_string(),
                java_type: resolved.java_type,
                imports: resolved.imports,
                optionality,
                owned: resolved.owned,
                collection: resolved.collection,
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

/// Parse through boxed names so collection elements work, then use primitives
/// for required record/value components where null is not a meaningful state.
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

/// A component gets a null check when it *can* be null and was not marked `?`.
fn needs_null_check(field: &Field) -> bool {
    !field.collection
        && is_reference_type(unboxed(&field.java_type))
        && field.optionality != Optionality::Nullable
}

/// Defensive copy plus an empty default, in one statement per collection.
///
/// Both halves matter and both are about the caller: a component holding the
/// list the caller passed in is not actually immutable, and a null bucket
/// makes every consumer downstream write a null check that should never have
/// been their problem.
fn collection_defaults(fields: &[Field]) -> String {
    fields
        .iter()
        .filter(|f| f.collection)
        .map(|f| {
            let empty = if f.java_type.starts_with("Map") {
                "Map.of()"
            } else {
                "List.of()"
            };
            let copy = if f.java_type.starts_with("Map") {
                "Map.copyOf"
            } else {
                "List.copyOf"
            };
            format!(
                "        {0} = {0} == null ? {empty} : {copy}({0});\n",
                f.name
            )
        })
        .collect()
}

fn has_collection(fields: &[Field]) -> bool {
    fields.iter().any(|f| f.collection)
}

/// The component's declared type. `?` wraps it in `Optional`, so absence is in
/// the type rather than in a comment nobody reads.
///
/// This is the one place jails deliberately parts company with `java.md`'s
/// "Optional as a return type only, never a field". A record component is both
/// at once, and the alternative -- a nullable component plus a differently
/// named `Optional`-returning method, since an accessor cannot be overridden
/// to change its return type -- is worse on every axis that matters here.
fn declared_type(field: &Field) -> String {
    match field.optionality {
        Optionality::Nullable => format!("Optional<{}>", boxed(&field.java_type)),
        _ if field.collection => field.java_type.clone(),
        _ => unboxed(&field.java_type).to_string(),
    }
}

/// `Optional<int>` does not exist, so an optional primitive takes its wrapper.
fn boxed(java_type: &str) -> &str {
    match java_type {
        "int" => "Integer",
        "long" => "Long",
        "boolean" => "Boolean",
        "double" => "Double",
        other => other,
    }
}

/// An `Optional` component still has to be non-null itself; a null `Optional`
/// is the one thing worse than a null value. Normalise rather than reject:
/// `of(..., null)` meaning "absent" is what every caller expects.
fn optional_defaults(fields: &[Field]) -> String {
    fields
        .iter()
        .filter(|f| f.optionality == Optionality::Nullable)
        .map(|f| {
            format!(
                "        {0} = Objects.requireNonNullElse({0}, Optional.empty());\n",
                f.name
            )
        })
        .collect()
}

fn has_optional(fields: &[Field]) -> bool {
    fields
        .iter()
        .any(|f| f.optionality == Optionality::Nullable)
}

/// Only `!` asks for the blank check, and only text can be blank.
fn needs_blank_check(field: &Field) -> bool {
    field.optionality == Optionality::NonBlank && field.java_type == "String"
}

/// Trim-then-reject, in that order, so " " fails rather than sneaking past.
fn blank_checks(fields: &[&Field]) -> String {
    let mut out = String::new();
    for field in fields {
        out += &format!("        {0} = {0}.trim();\n", field.name);
        out += &format!(
            "        if ({0}.isEmpty()) {{\n            throw new IllegalArgumentException(\"{0} must not be blank\");\n        }}\n",
            field.name
        );
    }
    out
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
        .ok_or_else(|| {
            "could not find a .java file under src/main/java to infer the base package".to_string()
        })?;
    let contents = fs::read_to_string(&entry)
        .map_err(|e| format!("failed to read {}: {e}", entry.display()))?;
    for line in contents.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("package ") {
            if let Some(pkg) = rest.trim().strip_suffix(';') {
                return Ok(pkg.trim().to_string());
            }
        }
    }
    Err(format!(
        "no package declaration found in {}",
        entry.display()
    ))
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

/// Spring Boot 4 moved `@AutoConfigureMockMvc` from
/// `org.springframework.boot.test.autoconfigure.web.servlet` to
/// `org.springframework.boot.webmvc.test.autoconfigure` with no back-compat
/// shim, so the scaffolded controller test needs to import the right one.
fn mockmvc_autoconfigure_import(root: &Path) -> &'static str {
    const LEGACY: &str =
        "org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc";
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
    let major: Option<u32> = after[vstart..vstart + vend]
        .split('.')
        .next()
        .and_then(|s| s.parse().ok());
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
    /// Ports -- the interfaces the application depends on, kept free of the
    /// technology that implements them.
    pub const APP: &str = "app";
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
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
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

    let Some(package_at) = lines
        .iter()
        .position(|l| l.trim_start().starts_with("package "))
    else {
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

pub fn generate(
    kind: ArtifactKind,
    name: &str,
    fields: &[String],
    package: Option<&str>,
) -> Result<()> {
    let root = find_project_root()?;
    let base = base_package(&root)?;

    // These kinds use NAME as a path/description rather than a Java class
    // name. Handle them before the shared capitalisation below.
    if matches!(kind, ArtifactKind::Cases) {
        return generate_cases(
            &root,
            &subpackage(&base, package.unwrap_or("")),
            Path::new(name),
        );
    }
    if matches!(kind, ArtifactKind::Migration) {
        return generate_migration(&root, name);
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
                    contents: controller_stub_test(
                        &web,
                        &name,
                        mockmvc_autoconfigure_import(&root),
                    ),
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
        // The layer-less kind: a plain class and its test, in the base package
        // rather than a subpackage, because "a class" says nothing about which
        // layer owns it. Everything else here has a conventional home; this is
        // the one for ordinary Java -- an algorithm, a ring buffer, a parser.
        ArtifactKind::Class => {
            let pkg = place("");
            vec![
                Artifact {
                    kind: "class",
                    path: main_dir(&root, &pkg).join(format!("{name}.java")),
                    contents: stub_class(&pkg, &name),
                },
                Artifact {
                    kind: "class test",
                    path: test_dir(&root, &pkg).join(format!("{name}Test.java")),
                    contents: class_test(&pkg, &name),
                },
            ]
        }
        ArtifactKind::Interface => {
            let pkg = place("");
            vec![Artifact {
                kind: "interface",
                path: main_dir(&root, &pkg).join(format!("{name}.java")),
                contents: interface_java(&pkg, &name),
            }]
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
                    contents: record_test(&root, &domain, &name, &parsed),
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
                    contents: value_test(&root, &domain, &name, &parsed),
                },
            ]
        }
        ArtifactKind::Enum => {
            let constants = parse_constants(fields)?;
            let domain = place(layout::DOMAIN);
            vec![
                Artifact {
                    kind: "enum",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: enum_java(&domain, &name, &constants),
                },
                Artifact {
                    kind: "enum test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: enum_test(&domain, &name, &constants),
                },
            ]
        }
        ArtifactKind::Repo => {
            let app = place(layout::APP);
            let adapters = place(layout::ADAPTERS);
            let domain = place(layout::DOMAIN);
            let mut artifacts = Vec::new();

            // A repository of a type that does not exist is meaningless, and
            // the port would not compile. Rather than fail, lay down the
            // smallest record that could be one -- it is a starting point the
            // reader will obviously edit, the same way `scaffold` works.
            if !main_dir(&root, &domain)
                .join(format!("{name}.java"))
                .exists()
            {
                let id = parse_fields(&["id:string!".to_string()])?;
                artifacts.push(Artifact {
                    kind: "record (placeholder for the repository)",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: record_java(&domain, &name, &id),
                });
                artifacts.push(Artifact {
                    kind: "record test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: record_test(&root, &domain, &name, &id),
                });
            }

            artifacts.push(Artifact {
                kind: "repository port",
                path: main_dir(&root, &app).join(format!("{name}Repository.java")),
                contents: repository_port(&app, &name, &import_of(&app, &domain, &name)),
            });
            artifacts.push(Artifact {
                kind: "JDBC adapter",
                path: main_dir(&root, &adapters).join(format!("Jdbc{name}Repository.java")),
                contents: jdbc_repository(
                    &adapters,
                    &name,
                    &format!(
                        "{}{}",
                        import_of(&adapters, &domain, &name),
                        import_of(&adapters, &app, &format!("{name}Repository"))
                    ),
                ),
            });
            artifacts.push(Artifact {
                kind: "JDBC adapter integration test",
                path: test_dir(&root, &adapters).join(format!("Jdbc{name}RepositoryIT.java")),
                contents: jdbc_repository_test(&adapters, &name),
            });
            artifacts
        }
        ArtifactKind::Handler => {
            let api = place(layout::API);
            let domain = place(layout::DOMAIN);
            let mut artifacts = Vec::new();

            // Every handler renders failures through the same envelope, so the
            // first one lays it down and the rest reuse it.
            if !main_dir(&root, &domain).join("ApiError.java").exists() {
                let fields = parse_fields(&[
                    "code:string!".to_string(),
                    "message:string!".to_string(),
                    "details:map<string,string>".to_string(),
                ])?;
                artifacts.push(Artifact {
                    kind: "error envelope",
                    path: main_dir(&root, &domain).join("ApiError.java"),
                    contents: value_java(&domain, "ApiError", &fields),
                });
                artifacts.push(Artifact {
                    kind: "error envelope test",
                    path: test_dir(&root, &domain).join("ApiErrorTest.java"),
                    contents: value_test(&root, &domain, "ApiError", &fields),
                });
            }

            artifacts.push(Artifact {
                kind: "handler",
                path: main_dir(&root, &api).join(format!("{name}Handler.java")),
                contents: handler_java(&api, &name, &import_of(&api, &domain, "ApiError")),
            });
            artifacts.push(Artifact {
                kind: "handler test",
                path: test_dir(&root, &api).join(format!("{name}HandlerTest.java")),
                contents: handler_test(&api, &name),
            });
            artifacts
        }
        ArtifactKind::Sealed => {
            let variants = parse_variants(fields)?;
            let domain = place(layout::DOMAIN);
            vec![
                Artifact {
                    kind: "sealed type",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: sealed_java(&domain, &name, &variants),
                },
                Artifact {
                    kind: "sealed type test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: sealed_test(&domain, &name, &variants),
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
        ArtifactKind::Migration => unreachable!("handled above -- its NAME is a SQL description"),
        ArtifactKind::Test => {
            let pkg = place("");
            vec![Artifact {
                kind: "test",
                path: test_dir(&root, &pkg).join(format!("{name}Test.java")),
                contents: stub_test(&pkg, &name),
            }]
        }
        ArtifactKind::IntegrationTest => {
            let pkg = place("");
            vec![Artifact {
                kind: "integration test",
                path: test_dir(&root, &pkg).join(format!("{name}IT.java")),
                contents: integration_test_java(&pkg, &name),
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
pub(crate) fn import_of(user: &str, owner: &str, class: &str) -> String {
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

    let place = |default: &str| subpackage(base, package.unwrap_or(default));
    let domain = place(layout::DOMAIN);
    let repository = place(layout::APP);
    let adapters = place(layout::ADAPTERS);
    let service = place(layout::SERVICE);
    let web = place(layout::WEB);

    let domain_in = |user: &str| import_of(user, &domain, name);

    Ok(vec![
        Artifact {
            kind: "record",
            path: main_dir(root, &domain).join(format!("{name}.java")),
            contents: record_java(&domain, name, &parsed),
        },
        Artifact {
            kind: "record test",
            path: test_dir(root, &domain).join(format!("{name}Test.java")),
            contents: record_test(root, &domain, name, &parsed),
        },
        Artifact {
            kind: "repository port",
            path: main_dir(root, &repository).join(format!("{name}Repository.java")),
            contents: repository_port(&repository, name, &domain_in(&repository)),
        },
        Artifact {
            kind: "JDBC adapter",
            path: main_dir(root, &adapters).join(format!("Jdbc{name}Repository.java")),
            contents: jdbc_repository(
                &adapters,
                name,
                &format!(
                    "{}{}",
                    domain_in(&adapters),
                    import_of(&adapters, &repository, &format!("{name}Repository"))
                ),
            ),
        },
        Artifact {
            kind: "JDBC adapter integration test",
            path: test_dir(root, &adapters).join(format!("Jdbc{name}RepositoryIT.java")),
            contents: jdbc_repository_test(&adapters, name),
        },
        Artifact {
            kind: "service",
            path: main_dir(root, &service).join(format!("{name}Service.java")),
            contents: stub_service(&service, name),
        },
        Artifact {
            kind: "service test",
            path: test_dir(root, &service).join(format!("{name}ServiceTest.java")),
            contents: service_stub_test(&service, name),
        },
        Artifact {
            kind: "controller",
            path: main_dir(root, &web).join(format!("{name}Controller.java")),
            contents: stub_controller(&web, name),
        },
        Artifact {
            kind: "controller test",
            path: test_dir(root, &web).join(format!("{name}ControllerTest.java")),
            contents: controller_stub_test(&web, name, mockmvc_autoconfigure_import(root)),
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
            main_dir(&root, &place(layout::APP)).join(format!("{name}Repository.java")),
            main_dir(&root, &place(layout::ADAPTERS)).join(format!("Jdbc{name}Repository.java")),
            test_dir(&root, &place(layout::ADAPTERS)).join(format!("Jdbc{name}RepositoryIT.java")),
            main_dir(&root, &place(layout::SERVICE)).join(format!("{name}Service.java")),
            test_dir(&root, &place(layout::SERVICE)).join(format!("{name}ServiceTest.java")),
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
        ArtifactKind::Record | ArtifactKind::Value | ArtifactKind::Enum | ArtifactKind::Sealed => {
            vec![
                main_dir(&root, &place(layout::DOMAIN)).join(format!("{name}.java")),
                test_dir(&root, &place(layout::DOMAIN)).join(format!("{name}Test.java")),
            ]
        }
        ArtifactKind::Command => vec![
            main_dir(&root, &place(layout::CLI)).join(format!("{name}Command.java")),
            test_dir(&root, &place(layout::CLI)).join(format!("{name}CommandTest.java")),
        ],
        ArtifactKind::Handler => vec![
            main_dir(&root, &place(layout::API)).join(format!("{name}Handler.java")),
            test_dir(&root, &place(layout::API)).join(format!("{name}HandlerTest.java")),
        ],
        ArtifactKind::Repo => vec![
            main_dir(&root, &place(layout::APP)).join(format!("{name}Repository.java")),
            main_dir(&root, &place(layout::ADAPTERS)).join(format!("Jdbc{name}Repository.java")),
            test_dir(&root, &place(layout::ADAPTERS)).join(format!("Jdbc{name}RepositoryIT.java")),
        ],
        ArtifactKind::Cli => vec![
            main_dir(&root, &place(layout::CLI)).join(format!("{name}Cli.java")),
            test_dir(&root, &place(layout::CLI)).join(format!("{name}CliTest.java")),
        ],
        // `cases` derives its class from a markdown file's name, so destroy
        // takes that same path and resolves it the same way generate did.
        ArtifactKind::Cases => {
            vec![
                test_dir(&root, &place(""))
                    .join(format!("{}.java", cases_class_name(Path::new(&raw_name))?)),
            ]
        }
        ArtifactKind::Migration => {
            return Err(
                "migrations are forward-only; create a new migration instead of destroying one"
                    .to_string(),
            );
        }
        ArtifactKind::Class => vec![
            main_dir(&root, &place("")).join(format!("{name}.java")),
            test_dir(&root, &place("")).join(format!("{name}Test.java")),
        ],
        ArtifactKind::Interface => vec![main_dir(&root, &place("")).join(format!("{name}.java"))],
        ArtifactKind::Test => vec![test_dir(&root, &place("")).join(format!("{name}Test.java"))],
        ArtifactKind::IntegrationTest => {
            vec![test_dir(&root, &place("")).join(format!("{name}IT.java"))]
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

fn interface_java(pkg: &str, name: &str) -> String {
    format!("package {pkg};\n\npublic interface {name} {{\n}}\n")
}

fn integration_test_java(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

@Disabled("todo: wire the real integration boundary")
class {name}IT {{

    @Test
    void worksEndToEnd() {{
        throw new UnsupportedOperationException("todo");
    }}
}}
"#
    )
}

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

fn stub_class(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

public final class {name} {{
}}
"#
    )
}

/// The companion test for `generate class`. It instantiates the class rather
/// than asserting `true`: a bare class has an implicit no-arg constructor, so
/// this compiles the moment it is generated, and the day a real constructor
/// arrives the test stops compiling -- which is the reminder to write the real
/// assertion, not a failure.
fn class_test(pkg: &str, name: &str) -> String {
    let victim = lower_first(name);
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;

class {name}Test {{

    @Test
    void shouldDoSomething() {{
        {name} {victim} = new {name}();

        assertThat({victim}).isNotNull();
    }}

}}
"#
    )
}

fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
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

// ---- companion tests for the bare `generate controller`/`service` stubs. ----

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

// ---- record: an immutable plain-Java data carrier. Same field:type parsing,
// no framework annotations, and a compact constructor so an invalid value cannot be
// constructed in the first place. ----

fn record_java(pkg: &str, name: &str, fields: &[Field]) -> String {
    // Only reference components can be null, and only ones not marked `?`
    // are checked -- if that leaves nothing, the compact constructor is dead
    // weight.
    let checked: Vec<&Field> = fields.iter().filter(|f| needs_null_check(f)).collect();
    let blank_checked: Vec<&Field> = fields.iter().filter(|f| needs_blank_check(f)).collect();
    let optional = has_optional(fields);
    let needs_objects = !checked.is_empty() || optional;
    let needs_constructor = needs_objects || !blank_checked.is_empty() || has_collection(fields);
    let mut imports: Vec<&str> = fields.iter().flat_map(|f| f.imports.clone()).collect();
    if needs_objects {
        imports.push("java.util.Objects");
    }
    if optional {
        imports.push("java.util.Optional");
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
        .map(|f| format!("{} {}", declared_type(f), f.name))
        .collect::<Vec<_>>()
        .join(", ");

    out += "/**\n";
    out += &format!(" * An immutable {name} value.\n");
    out += " *\n";
    if needs_constructor {
        out += " * <p>The compact constructor rejects what the field spec said to reject, so\n";
        out += " * any instance that exists is a valid one and callers downstream do not\n";
        out += " * have to re-check.\n";
    } else {
        out += " * <p>There is nothing to validate: no instance of this record can be in an\n";
        out += " * invalid state.\n";
    }
    if optional {
        out += " *\n * <p>An {@code Optional} component is absence in the type rather than a\n";
        out += " * null nobody checks. Passing {@code null} for one means absent.\n";
    }
    out += " */\n";
    out += &format!("public record {name}({components}) {{\n");

    if needs_constructor {
        out += &format!("\n    public {name} {{\n");
        for field in &checked {
            out += &format!(
                "        Objects.requireNonNull({name}, \"{name}\");\n",
                name = field.name
            );
        }
        out += &optional_defaults(fields);
        out += &collection_defaults(fields);
        out += &blank_checks(&blank_checked);
        out += "    }\n";
    }

    out += "}\n";
    out
}

/// A companion test asserting the accessors return what was passed and that
/// the compact constructor actually rejects a null.
fn record_test(root: &Path, pkg: &str, name: &str, fields: &[Field]) -> String {
    let mut imports: Vec<&str> = fields.iter().flat_map(|f| f.imports.clone()).collect();
    imports.sort();
    imports.dedup();

    // A component whose type this project owns has no literal jails can write.
    // Rather than invent a constructor call that will not compile, the test is
    // generated in full and disabled, naming exactly what it needs.
    let samples: Vec<Option<String>> = fields.iter().map(|f| sample_value(f, root, pkg)).collect();
    let unfabricable: Vec<&str> = fields
        .iter()
        .zip(&samples)
        .filter(|(_, s)| s.is_none())
        .map(|(f, _)| f.name.as_str())
        .collect();
    let args = samples
        .iter()
        .zip(fields)
        .map(|(sample, field)| {
            sample
                .clone()
                .unwrap_or_else(|| format!("/* TODO: a {} */ null", field.java_type))
        })
        .collect::<Vec<_>>()
        .join(", ");
    let var = name.to_lowercase();
    if has_optional(fields) {
        imports.push("java.util.Optional");
        imports.sort();
        imports.dedup();
    }

    let mut out = format!("package {pkg};\n\n");
    out += "import org.junit.jupiter.api.Test;\n";
    if !imports.is_empty() {
        out += "\n";
        for imp in &imports {
            out += &format!("import {imp};\n");
        }
    }
    // The nulled component must be one the constructor actually checks: a
    // primitive cannot take null at all, and a `?` one is allowed to be null.
    let first_reference = fields.iter().find(|f| needs_null_check(f));

    out += "\nimport static org.assertj.core.api.Assertions.assertThat;\n";
    if first_reference.is_some() {
        out += "import static org.assertj.core.api.Assertions.assertThatNullPointerException;\n";
    }
    if !unfabricable.is_empty() {
        out += "\nimport org.junit.jupiter.api.Disabled;\n";
    }
    out += "\n";
    if !unfabricable.is_empty() {
        out += &format!(
            "@Disabled(\"todo: supply a sample for {} -- jails cannot know how to build one\")\n",
            unfabricable.join(", ")
        );
    }
    out += &format!("class {name}Test {{\n\n");

    out += "    @Test\n    void accessorsReturnWhatWasConstructed() {\n";
    out += &format!("        {name} {var} = new {name}({args});\n\n");
    if fields.is_empty() {
        out += &format!("        assertThat({var}).isEqualTo(new {name}());\n");
    } else {
        for (field, sample) in fields.iter().zip(&samples) {
            match sample {
                Some(value) => {
                    out += &format!(
                        "        assertThat({var}.{}()).isEqualTo({value});\n",
                        field.name
                    )
                }
                None => out += &format!("        // TODO: assert on {}\n", field.name),
            }
        }
    }
    out += "    }\n";

    if let Some(first) = first_reference {
        // Only one component is nulled out: one case proves the compact
        // constructor runs, and a case per field would just restate it.
        let nulled = fields
            .iter()
            .zip(&samples)
            .map(|(f, sample)| {
                if f.name == first.name {
                    "null".to_string()
                } else {
                    sample.clone().unwrap_or_else(|| "null".to_string())
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        out += "\n    @Test\n    void rejectsANullComponent() {\n";
        out += &format!("        assertThatNullPointerException()\n");
        out += &format!("                .isThrownBy(() -> new {name}({nulled}))\n");
        out += &format!(
            "                .withMessageContaining(\"{}\");\n",
            first.name
        );
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

/// A literal a generated test can construct the component from.
///
/// `None` means jails cannot fabricate one: a type this project owns could
/// have any constructor at all, and guessing produces a test that does not
/// compile. The one case it *can* solve is an enum -- hence `generate enum`
/// pulling its weight twice.
fn sample_value(field: &Field, root: &Path, pkg: &str) -> Option<String> {
    // An absent Optional is a sample of anything, so `?` rescues even a type
    // jails knows nothing about.
    if field.optionality == Optionality::Nullable {
        return Some("Optional.empty()".to_string());
    }
    // An empty collection is a sample of any element type, known or not.
    if field.collection {
        return Some(if field.java_type.starts_with("Map") {
            "Map.of()".to_string()
        } else {
            "List.of()".to_string()
        });
    }
    if !field.owned {
        return Some(sample_literal(&field.java_type).to_string());
    }
    is_enum(root, pkg, &field.java_type).then(|| format!("{}.values()[0]", field.java_type))
}

/// Whether `<Type>.java` in this package declares an enum. Reading the file is
/// the only honest way to know: jails has no type model, and guessing from the
/// name would be worse than admitting ignorance.
fn is_enum(root: &Path, pkg: &str, type_name: &str) -> bool {
    fs::read_to_string(main_dir(root, pkg).join(format!("{type_name}.java")))
        .map(|src| src.contains(&format!("enum {type_name}")))
        .unwrap_or(false)
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
        "Instant" => "Instant.parse(\"2024-01-01T00:00:00Z\")",
        "UUID" => "UUID.fromString(\"00000000-0000-0000-0000-000000000001\")",
        "Currency" => "Currency.getInstance(\"GBP\")",
        "BigDecimal" => "new BigDecimal(\"1.00\")",
        "byte[]" => "new byte[] {1}",
        "Duration" => "Duration.ofSeconds(1)",
        "ZoneId" => "ZoneId.of(\"UTC\")",
        "URI" => "URI.create(\"https://example.com\")",
        "Path" => "Path.of(\"sample\")",
        _ => "null",
    }
}

// ---- value: a record that not only rejects nulls (which `record` already
// does) but normalises and validates, so an instance is *meaningful*, not just
// non-null. Blank strings are the case that bites in practice -- a required
// identifier that is present but empty passes every null check downstream. ----

fn value_java(pkg: &str, name: &str, fields: &[Field]) -> String {
    let strings: Vec<&Field> = fields.iter().filter(|f| needs_blank_check(f)).collect();
    let checked: Vec<&Field> = fields.iter().filter(|f| needs_null_check(f)).collect();
    let optional = has_optional(fields);

    let mut imports: Vec<&str> = fields.iter().flat_map(|f| f.imports.clone()).collect();
    if !checked.is_empty() || optional {
        imports.push("java.util.Objects");
    }
    if optional {
        imports.push("java.util.Optional");
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
        .map(|f| format!("{} {}", declared_type(f), f.name))
        .collect::<Vec<_>>()
        .join(", ");
    let names = fields
        .iter()
        .map(|f| f.name.clone())
        .collect::<Vec<_>>()
        .join(", ");

    out += "/**\n";
    out += &format!(" * A validated {name} value.\n");
    out += " *\n";
    out += " * <p>All validation lives in the compact constructor, which runs before the\n";
    out += " * components are assigned -- so there is no way to reach an instance that\n";
    out += " * skipped it, not even through deserialisation or a copy.\n";
    if !strings.is_empty() {
        out += " *\n";
        out += " * <p>Text marked {@code !} is trimmed and then required to be non-blank: a\n";
        out += " * present-but-empty value passes every null check downstream, which is\n";
        out += " * exactly why it is worth rejecting here instead.\n";
    }
    if optional {
        out += " *\n";
        out += " * <p>An {@code Optional} component is absence in the type rather than a null\n";
        out += " * nobody checks. Passing {@code null} for one means absent.\n";
    }
    out += " */\n";
    out += &format!("public record {name}({components}) {{\n\n");

    // Compact constructor: normalise first, then validate what normalising
    // produced, so " " fails the blank check rather than sneaking past it.
    out += &format!("    public {name} {{\n");
    for field in &checked {
        out += &format!(
            "        Objects.requireNonNull({0}, \"{0} is required\");\n",
            field.name
        );
    }
    out += &optional_defaults(fields);
    out += &collection_defaults(fields);
    out += &blank_checks(&strings);
    out += "    }\n\n";

    out += "    /**\n";
    out +=
        &format!("     * Builds a {name}. Identical to the constructor today; it exists so that\n");
    out += "     * parsing, defaulting or a cache can be added later without changing a\n";
    out += "     * single call site.\n";
    out += "     */\n";
    out += &format!("    public static {name} of({components}) {{\n");
    out += &format!("        return new {name}({names});\n");
    out += "    }\n";
    out += "}\n";
    out
}

fn value_test(root: &Path, pkg: &str, name: &str, fields: &[Field]) -> String {
    // Only `!` fields are trimmed and blank-checked, so only they have those
    // behaviours to assert.
    let strings: Vec<&Field> = fields.iter().filter(|f| needs_blank_check(f)).collect();

    let mut imports: Vec<&str> = fields.iter().flat_map(|f| f.imports.clone()).collect();
    imports.sort();
    imports.dedup();

    let samples: Vec<Option<String>> = fields.iter().map(|f| sample_value(f, root, pkg)).collect();
    let unfabricable: Vec<&str> = fields
        .iter()
        .zip(&samples)
        .filter(|(_, s)| s.is_none())
        .map(|(f, _)| f.name.as_str())
        .collect();
    if has_optional(fields) {
        imports.push("java.util.Optional");
        imports.sort();
        imports.dedup();
    }
    let placeholder = |field: &Field| format!("/* TODO: a {} */ null", field.java_type);
    let args = samples
        .iter()
        .zip(fields)
        .map(|(sample, field)| sample.clone().unwrap_or_else(|| placeholder(field)))
        .collect::<Vec<_>>()
        .join(", ");
    // Same argument list, but with one named component swapped out.
    let args_with = |target: &str, replacement: &str| {
        fields
            .iter()
            .zip(&samples)
            .map(|(f, sample)| {
                if f.name == target {
                    replacement.to_string()
                } else {
                    sample.clone().unwrap_or_else(|| placeholder(f))
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
    out += "import static org.assertj.core.api.Assertions.assertThatThrownBy;\n";
    if !unfabricable.is_empty() {
        out += "\nimport org.junit.jupiter.api.Disabled;\n";
    }
    out += "\n";
    if !unfabricable.is_empty() {
        out += &format!(
            "@Disabled(\"todo: supply a sample for {} -- jails cannot know how to build one\")\n",
            unfabricable.join(", ")
        );
    }
    out += &format!("class {name}Test {{\n\n");

    out += "    @Test\n    void keepsWhatItWasGiven() {\n";
    out += &format!("        var value = {name}.of({args});\n\n");
    for (field, sample) in fields.iter().zip(&samples) {
        match sample {
            Some(value) => {
                out += &format!(
                    "        assertThat(value.{}()).isEqualTo({value});\n",
                    field.name
                )
            }
            None => out += &format!("        // TODO: assert on {}\n", field.name),
        }
    }
    out += "    }\n\n";

    // Only a component the constructor actually checks: a primitive cannot be
    // handed null, and a `?` one is allowed to be.
    if let Some(first) = fields.iter().find(|f| needs_null_check(f)) {
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

// ---- enum: the closed set of alternatives, and the one owned type whose
// shape jails can work out without being told. ----

/// Enum constants are `SCREAMING_SNAKE_CASE` by convention, and a generated
/// file that ignores the convention is one the reader has to think about.
fn parse_constants(args: &[String]) -> Result<Vec<String>> {
    if args.is_empty() {
        return Err(
            "an enum needs at least one constant, e.g. `generate enum Currency GBP EUR`"
                .to_string(),
        );
    }
    let mut constants = Vec::new();
    for arg in args {
        let constant: String = arg
            .trim()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect();
        if constant.is_empty() || constant.starts_with(|c: char| c.is_ascii_digit()) {
            return Err(format!("'{arg}' is not a usable enum constant"));
        }
        if constants.contains(&constant) {
            return Err(format!("duplicate enum constant '{constant}'"));
        }
        constants.push(constant);
    }
    Ok(constants)
}

fn enum_java(pkg: &str, name: &str, constants: &[String]) -> String {
    let mut out = format!("package {pkg};\n\n");
    out += "/**\n";
    out += &format!(" * The {name} values this application understands.\n");
    out += " *\n";
    out += " * <p>A closed set, so a switch over it is checked for exhaustiveness and\n";
    out += " * adding a constant makes the compiler point at every place that has to\n";
    out += " * handle it.\n";
    out += " */\n";
    out += &format!("public enum {name} {{\n");
    out += &format!("    {}\n", constants.join(",\n    "));
    out += "}\n";
    out
}

fn enum_test(pkg: &str, name: &str, constants: &[String]) -> String {
    let first = &constants[0];
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

class {name}Test {{

    @Test
    void parsesItsOwnNames() {{
        assertThat({name}.valueOf("{first}")).isEqualTo({name}.{first});
    }}

    /** The failure mode worth pinning: valueOf throws, it does not return null. */
    @Test
    void rejectsAnUnknownName() {{
        assertThatIllegalArgumentException().isThrownBy(() -> {name}.valueOf("NOPE"));
    }}

    @Test
    void declaresEveryConstantExactlyOnce() {{
        assertThat({name}.values()).hasSize({count}).doesNotHaveDuplicates();
    }}
}}
"#,
        count = constants.len()
    )
}

// ---- handler: HTTP for one resource, thin by construction. ----

/// `WorkItem` -> `/work-items`. The URL convention is kebab-case and plural,
/// and deriving it beats making every caller remember to type it.
fn resource_path(name: &str) -> String {
    let mut path = String::from("/");
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            path.push('-');
        }
        path.extend(c.to_lowercase());
    }
    path.push('s');
    path
}

fn handler_java(pkg: &str, name: &str, extra: &str) -> String {
    let path = resource_path(name);
    format!(
        r#"package {pkg};

{extra}import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpHandler;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;

/**
 * HTTP for the {{@code {path}}} resource.
 *
 * <p>Thin by construction: this class binds, routes, and maps outcomes to
 * status codes. It holds no rules of its own, so the same {{@link Service}} can
 * be driven from the CLI without any of this.
 *
 * <p>{{@link Service}} deals in JSON strings because a scaffold cannot know
 * your types. Narrowing it to real ones is the first thing worth doing here.
 *
 * <p>Status codes are the contract:
 * <ul>
 *   <li>400 -- the body is not JSON, or a query parameter is not a number
 *   <li>404 -- no such resource
 *   <li>422 -- well-formed, but the domain rejected it
 * </ul>
 */
public final class {name}Handler implements HttpHandler {{

    /** The path this handler is registered under. */
    public static final String PATH = "{path}";

    /** What this handler needs from the application behind it. */
    public interface Service {{

        /** @return a JSON array of items, never null. */
        String list(int offset, int limit);

        /** @return the item as JSON, or empty when there is no such id. */
        Optional<String> find(String id);

        /**
         * @param body the raw request body
         * @return the created item as JSON
         * @throws IllegalArgumentException when the domain rejects it -- becomes a 422
         */
        String create(String body);
    }}

    private final Service service;

    public {name}Handler(Service service) {{
        this.service = Objects.requireNonNull(service, "service is required");
    }}

    @Override
    public void handle(HttpExchange exchange) throws IOException {{
        try (exchange) {{
            var path = exchange.getRequestURI().getPath();
            var id = idFrom(path);

            var response =
                    switch (exchange.getRequestMethod()) {{
                        case "GET" -> id.isEmpty() ? list(exchange) : find(id);
                        case "POST" -> create(body(exchange));
                        default -> error(405, "method_not_allowed", "use GET or POST");
                    }};

            send(exchange, response);
        }}
    }}

    /** The trailing path segment, or empty for a request against the collection. */
    private String idFrom(String path) {{
        var rest = path.length() > PATH.length() ? path.substring(PATH.length()) : "";
        return rest.startsWith("/") ? rest.substring(1) : rest;
    }}

    private Response list(HttpExchange exchange) {{
        var query = query(exchange);
        try {{
            var offset = Integer.parseInt(query.getOrDefault("offset", "0"));
            var limit = Integer.parseInt(query.getOrDefault("limit", "50"));
            return new Response(200, service.list(offset, limit));
        }} catch (NumberFormatException malformed) {{
            return error(400, "bad_request", "offset and limit must be whole numbers");
        }}
    }}

    private Response find(String id) {{
        return service.find(id)
                .map(json -> new Response(200, json))
                .orElseGet(() -> error(404, "not_found", "no {path} with id " + id));
    }}

    private Response create(String body) {{
        if (body.isBlank() || !body.stripLeading().startsWith("{{")) {{
            return error(400, "bad_request", "expected a JSON object");
        }}
        try {{
            return new Response(201, service.create(body));
        }} catch (IllegalArgumentException rejected) {{
            // Well-formed but wrong: the client sent something the domain will
            // not accept, which is 422 rather than 400.
            return error(422, "unprocessable", rejected.getMessage());
        }}
    }}

    /** An {{@link ApiError}} rendered as the response body. */
    private Response error(int status, String code, String message) {{
        var envelope = new ApiError(code, message == null ? code : message, Map.of());
        return new Response(
                status,
                "{{\"code\":\"" + envelope.code() + "\",\"message\":\"" + envelope.message() + "\"}}");
    }}

    private record Response(int status, String body) {{}}

    private static String body(HttpExchange exchange) throws IOException {{
        try (var in = exchange.getRequestBody()) {{
            return new String(in.readAllBytes(), StandardCharsets.UTF_8);
        }}
    }}

    private static Map<String, String> query(HttpExchange exchange) {{
        var raw = exchange.getRequestURI().getQuery();
        if (raw == null || raw.isBlank()) {{
            return Map.of();
        }}
        var parsed = new java.util.LinkedHashMap<String, String>();
        for (var pair : raw.split("&")) {{
            var split = pair.split("=", 2);
            if (split.length == 2) {{
                parsed.put(split[0], split[1]);
            }}
        }}
        return Map.copyOf(parsed);
    }}

    private static void send(HttpExchange exchange, Response response) throws IOException {{
        var bytes = response.body().getBytes(StandardCharsets.UTF_8);
        exchange.getResponseHeaders().set("Content-Type", "application/json");
        exchange.sendResponseHeaders(response.status(), bytes.length);
        try (var out = exchange.getResponseBody()) {{
            out.write(bytes);
        }}
    }}
}}
"#
    )
}

fn handler_test(pkg: &str, name: &str) -> String {
    let path = resource_path(name);
    format!(
        r#"package {pkg};

import com.sun.net.httpserver.HttpServer;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.net.InetSocketAddress;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.util.Optional;
import java.util.concurrent.Executors;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * Drives the handler over a real loopback socket, because the interesting
 * half -- status codes, bodies, headers -- only exists once HTTP is involved.
 *
 * <p>Port 0 lets the OS pick a free one, so these tests are safe to run in
 * parallel and safe from whatever else is on 8080.
 */
class {name}HandlerTest {{

    private HttpServer server;

    /** A stand-in service: enough behaviour to exercise every status code. */
    private final {name}Handler.Service service = new {name}Handler.Service() {{
        @Override
        public String list(int offset, int limit) {{
            return "[{{\"id\":\"a\"}}]";
        }}

        @Override
        public Optional<String> find(String id) {{
            return id.equals("a") ? Optional.of("{{\"id\":\"a\"}}") : Optional.empty();
        }}

        @Override
        public String create(String body) {{
            if (body.contains("\"invalid\"")) {{
                throw new IllegalArgumentException("id must not be blank");
            }}
            return body;
        }}
    }};

    @BeforeEach
    void start() throws Exception {{
        server = HttpServer.create(new InetSocketAddress(0), 0);
        server.createContext({name}Handler.PATH, new {name}Handler(service));
        server.setExecutor(Executors.newVirtualThreadPerTaskExecutor());
        server.start();
    }}

    @AfterEach
    void stop() {{
        server.stop(0);
    }}

    private HttpResponse<String> send(String path, String body) throws Exception {{
        var uri = URI.create("http://localhost:" + server.getAddress().getPort() + path);
        var request = HttpRequest.newBuilder(uri)
                .method(
                        body == null ? "GET" : "POST",
                        body == null ? HttpRequest.BodyPublishers.noBody() : HttpRequest.BodyPublishers.ofString(body))
                .build();
        try (var client = HttpClient.newHttpClient()) {{
            return client.send(request, HttpResponse.BodyHandlers.ofString());
        }}
    }}

    @Test
    void listsTheCollection() throws Exception {{
        var response = send("{path}", null);

        assertThat(response.statusCode()).isEqualTo(200);
        assertThat(response.body()).contains("\"id\":\"a\"");
    }}

    @Test
    void findsOneById() throws Exception {{
        assertThat(send("{path}/a", null).statusCode()).isEqualTo(200);
    }}

    @Test
    void answersFourOhFourForAnUnknownId() throws Exception {{
        var response = send("{path}/nope", null);

        assertThat(response.statusCode()).isEqualTo(404);
        assertThat(response.body()).contains("not_found");
    }}

    @Test
    void answersFourHundredForABodyThatIsNotJson() throws Exception {{
        var response = send("{path}", "not json");

        assertThat(response.statusCode()).isEqualTo(400);
        assertThat(response.body()).contains("bad_request");
    }}

    /** Well-formed but rejected by the domain is 422, not 400. */
    @Test
    void answersFourTwentyTwoWhenTheDomainRejectsIt() throws Exception {{
        var response = send("{path}", "{{\"invalid\":true}}");

        assertThat(response.statusCode()).isEqualTo(422);
        assertThat(response.body()).contains("unprocessable");
    }}

    @Test
    void answersFourHundredForANonNumericPageWindow() throws Exception {{
        assertThat(send("{path}?offset=x", null).statusCode()).isEqualTo(400);
    }}
}}
"#
    )
}

// ---- repo: a port the application depends on, and the JDBC adapter that
// implements it. The one pattern java.md names by name. ----

fn repository_port(pkg: &str, name: &str, extra: &str) -> String {
    let var = name.to_lowercase();
    format!(
        r#"package {pkg};

{extra}import java.util.List;
import java.util.Optional;

/**
 * Storage for {{@link {name}}}, as the application sees it.
 *
 * <p>A port: no JDBC types, no driver, no dialect. Application code depends on
 * this interface, an adapter implements it, and a test can supply an in-memory
 * one without a database anywhere in sight.
 *
 * <p>{{@code findById}} returns {{@link Optional}} rather than null, and
 * {{@code findAll}} an empty list rather than null, so no caller has to guard.
 */
public interface {name}Repository {{

    Optional<{name}> findById(String id);

    List<{name}> findAll();

    /** Inserts a row. Define conflict behavior explicitly in the SQL adapter. */
    void save({name} {var});

    /** @return true when a row was actually removed. */
    boolean deleteById(String id);
}}
"#
    )
}

fn jdbc_repository(pkg: &str, name: &str, extra: &str) -> String {
    let var = name.to_lowercase();
    let table = format!("{}s", name.to_lowercase());
    format!(
        r#"package {pkg};

{extra}import java.sql.Connection;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.Optional;

/**
 * {{@link {name}Repository}} over plain JDBC. No ORM: the queries are visible,
 * and a {{@code PreparedStatement}} is the whole abstraction.
 *
 * <p>The caller owns the {{@link Connection}} -- this class neither opens nor
 * closes it, so one transaction can span several repositories.
 *
 * <p>{{@link #map}} and {{@link #bind}} are yours to finish: jails knows the
 * columns of exactly nothing. Until then the companion test is disabled.
 */
public final class Jdbc{name}Repository implements {name}Repository {{

    private static final String FIND_BY_ID =
            """
            select *
            from {table}
            where id = ?
            """;
    private static final String FIND_ALL =
            """
            select *
            from {table}
            order by id
            """;
    private static final String INSERT =
            """
            insert into {table} (id)
            values (?)
            """;
    private static final String DELETE_BY_ID =
            """
            delete from {table}
            where id = ?
            """;

    private final Connection connection;

    public Jdbc{name}Repository(Connection connection) {{
        this.connection = Objects.requireNonNull(connection, "connection is required");
    }}

    @Override
    public Optional<{name}> findById(String id) {{
        Objects.requireNonNull(id, "id is required");
        try (var query = connection.prepareStatement(FIND_BY_ID)) {{
            query.setString(1, id);
            try (var rows = query.executeQuery()) {{
                return rows.next() ? Optional.of(map(rows)) : Optional.empty();
            }}
        }} catch (SQLException error) {{
            throw new IllegalStateException("could not read {table} " + id, error);
        }}
    }}

    @Override
    public List<{name}> findAll() {{
        // Ordered explicitly: SQL does not otherwise promise row order.
        try (var query = connection.prepareStatement(FIND_ALL);
                var rows = query.executeQuery()) {{
            var all = new ArrayList<{name}>();
            while (rows.next()) {{
                all.add(map(rows));
            }}
            return List.copyOf(all);
        }} catch (SQLException error) {{
            throw new IllegalStateException("could not read {table}", error);
        }}
    }}

    @Override
    public void save({name} {var}) {{
        Objects.requireNonNull({var}, "{var} is required");
        try (var insert = connection.prepareStatement(INSERT)) {{
            bind(insert, {var});
            insert.executeUpdate();
        }} catch (SQLException error) {{
            throw new IllegalStateException("could not save to {table}", error);
        }}
    }}

    @Override
    public boolean deleteById(String id) {{
        Objects.requireNonNull(id, "id is required");
        try (var delete = connection.prepareStatement(DELETE_BY_ID)) {{
            delete.setString(1, id);
            return delete.executeUpdate() > 0;
        }} catch (SQLException error) {{
            throw new IllegalStateException("could not delete from {table} " + id, error);
        }}
    }}

    /** TODO: build a {name} from the current row. */
    private {name} map(ResultSet rows) throws SQLException {{
        throw new UnsupportedOperationException("TODO: map a {table} row to {name}");
    }}

    /** TODO: set every column the insert above declares. */
    private void bind(java.sql.PreparedStatement insert, {name} {var}) throws SQLException {{
        throw new UnsupportedOperationException("TODO: bind {name} to the insert");
    }}
}}
"#
    )
}

fn jdbc_repository_test(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

/**
 * Configure a real test database, apply the migrations, and exercise the SQL
 * in {{@link Jdbc{name}Repository}}. Keep this as an integration test: mocks
 * cannot prove SQL, constraints, transactions, or row mappings work.
 */
@Disabled("todo: configure the test database and finish the repository SQL mapping")
class Jdbc{name}RepositoryIT {{

    @Test
    void roundTripsThroughTheRealDatabase() {{
        throw new UnsupportedOperationException("todo");
    }}
}}
"#
    )
}

// ---- sealed: the closed set whose cases carry different data, which is the
// one an enum cannot model. ----

fn parse_variants(args: &[String]) -> Result<Vec<String>> {
    if args.is_empty() {
        return Err(
            "a sealed type needs at least one variant, e.g. `generate sealed Result Ok Failed`"
                .to_string(),
        );
    }
    let mut variants: Vec<String> = Vec::new();
    for arg in args {
        let variant = capitalize(arg.trim());
        if variant.is_empty() || !variant.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(format!("'{arg}' is not a usable variant name"));
        }
        if variants.contains(&variant) {
            return Err(format!("duplicate variant '{variant}'"));
        }
        variants.push(variant);
    }
    Ok(variants)
}

fn sealed_java(pkg: &str, name: &str, variants: &[String]) -> String {
    // The variants are nested, so the permits clause has to name them
    // qualified. (It could be omitted entirely for same-file subtypes, but
    // spelling it out is what makes the closed set obvious to a reader.)
    let permits = variants
        .iter()
        .map(|v| format!("{name}.{v}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = format!("package {pkg};\n\n");
    out += "/**\n";
    out += &format!(" * The outcomes a {name} can have.\n");
    out += " *\n";
    out += " * <p>Sealed rather than an enum because each case carries its own data --\n";
    out += " * give a variant the components it needs and no other case has to pretend\n";
    out += " * to have them.\n";
    out += " *\n";
    out += " * <p>A switch over this is checked for exhaustiveness, so leave the\n";
    out += " * {@code default} off: adding a variant should make the compiler point at\n";
    out += " * every place that has to handle it.\n";
    out += " *\n";
    out += " * {@snippet :\n";
    out += &format!(" * var summary = switch (result) {{\n");
    for variant in variants {
        out += &format!(
            " *     case {variant} v -> \"{}\";\n",
            variant.to_lowercase()
        );
    }
    out += " * };\n";
    out += " * }\n";
    out += " */\n";
    out += &format!("public sealed interface {name} permits {permits} {{\n");
    for variant in variants {
        out += &format!("\n    /** TODO: give {variant} the components it carries. */\n");
        out += &format!("    record {variant}() implements {name} {{}}\n");
    }
    out += "}\n";
    out
}

fn sealed_test(pkg: &str, name: &str, variants: &[String]) -> String {
    let mut out = format!("package {pkg};\n\n");
    out += "import org.junit.jupiter.api.Test;\n\n";
    out += "import static org.assertj.core.api.Assertions.assertThat;\n\n";
    out += "/**\n";
    out += " * The switch below has no {@code default} on purpose: adding a variant\n";
    out += " * should break this test at compile time, which is the whole reason to seal\n";
    out += " * the type in the first place.\n";
    out += " */\n";
    out += &format!("class {name}Test {{\n\n");
    out += &format!("    private String describe({name} result) {{\n");
    out += "        return switch (result) {\n";
    for variant in variants {
        out += &format!(
            "            case {name}.{variant} v -> \"{}\";\n",
            variant.to_lowercase()
        );
    }
    out += "        };\n";
    out += "    }\n";

    for variant in variants {
        out += &format!("\n    @Test\n    void describes{variant}() {{\n");
        out += &format!(
            "        assertThat(describe(new {name}.{variant}())).isEqualTo(\"{}\");\n",
            variant.to_lowercase()
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

    let source = fs::read_to_string(dispatcher)
        .map_err(|e| format!("failed to read {}: {e}", dispatcher.display()))?;
    let command_class = format!("{name}Command");
    // Scoped to the registry body, not the whole file: the dispatcher's own
    // Javadoc shows an example `commands.put(...)` line, and a whole-file
    // `contains` matched *that* -- so generating a command with the same name
    // as the example silently skipped registration.
    if registry_body(&source).is_some_and(|body| body.contains(&format!("{command_class}::run"))) {
        println!(
            "  exists  {command_class} is already registered in {}",
            dispatcher.display()
        );
        return Ok(());
    }

    // The dispatcher and the command can be in different packages once
    // `--package` is involved, so the registration may need an import too.
    let dispatcher_pkg = package_of(&source).unwrap_or_else(|| base.to_string());
    let command_pkg = subpackage(base, layout::CLI);

    let Some(spliced) = splice_registration(
        &source,
        &command_class,
        &import_of(&dispatcher_pkg, &command_pkg, &command_class),
    ) else {
        println!(
            "note: could not find the `return commands;` line in {} -- add {command_class} by hand",
            dispatcher.display()
        );
        return Ok(());
    };

    fs::write(dispatcher, spliced)
        .map_err(|e| format!("failed to write {}: {e}", dispatcher.display()))?;
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
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "java") {
                if fs::read_to_string(&path)
                    .map(|s| is_dispatcher(&s))
                    .unwrap_or(false)
                {
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

pub(crate) fn package_of(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        line.trim()
            .strip_prefix("package ")?
            .trim()
            .strip_suffix(';')
            .map(|s| s.trim().to_string())
    })
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
    out.push_str(&format!(
        "{indent}commands.put({command_class}.NAME, {command_class}::run);\n"
    ));
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

// ---- migration: the next forward-only SQL file. ----

fn generate_migration(root: &Path, description: &str) -> Result<()> {
    let description = sql_name(description)?;
    let dir = root.join("src/main/resources/db/migration");
    let version = next_migration_version(&dir)?;
    let path = dir.join(format!("V{version:03}__{description}.sql"));
    write_new_file(
        &path,
        "-- Forward-only migration. Write explicit SQL below.\n",
    )?;
    println!("created migration {}", path.display());
    Ok(())
}

fn next_migration_version(dir: &Path) -> Result<u32> {
    if !dir.exists() {
        return Ok(1);
    }
    let entries =
        fs::read_dir(dir).map_err(|e| format!("failed to read {}: {e}", dir.display()))?;
    let mut highest = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let digits = name
            .strip_prefix('V')
            .and_then(|rest| rest.split_once("__").map(|(version, _)| version))
            .or_else(|| name.split_once('_').map(|(version, _)| version));
        if let Some(version) = digits.and_then(|value| value.parse::<u32>().ok()) {
            highest = highest.max(version);
        }
    }
    highest
        .checked_add(1)
        .ok_or_else(|| "migration version overflow".to_string())
}

fn sql_name(value: &str) -> Result<String> {
    let mut out = String::new();
    let mut previous_was_lower_or_digit = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && previous_was_lower_or_digit && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            previous_was_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else if matches!(ch, '-' | '_' | ' ') {
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            previous_was_lower_or_digit = false;
        } else {
            return Err(format!("'{value}' is not a usable SQL migration name"));
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        Err("a migration needs a description, e.g. `jails g migration create_rewards`".to_string())
    } else {
        Ok(out)
    }
}

// ---- cases: a markdown checklist in, a pending JUnit class out. ----

/// Turn a brief's checklist into a `@Disabled` test class -- the todo list you
/// delete one `@Disabled` at a time.
fn generate_cases(root: &Path, pkg: &str, brief: &Path) -> Result<()> {
    let text = fs::read_to_string(brief)
        .map_err(|e| format!("failed to read {}: {e}", brief.display()))?;
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
    println!(
        "created cases {} ({} case{})",
        path.display(),
        cases.len(),
        if cases.len() == 1 { "" } else { "s" }
    );
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
    const MARKERS: [&str; 5] = [
        "acceptance",
        "criteria",
        "cases",
        "checklist",
        "requirements",
    ];

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
            (!digits.is_empty())
                .then(|| line[digits.len()..].strip_prefix(". "))
                .flatten()
        })?;
    let rest = rest.trim();
    // `- [ ]` / `- [x]` checkboxes: the box is not part of the case.
    let rest = rest
        .strip_prefix("[ ]")
        .or_else(|| rest.strip_prefix("[x]"))
        .or_else(|| rest.strip_prefix("[X]"))
        .unwrap_or(rest);
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
    let stem = brief.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
        format!(
            "{} has no file name to derive a class from",
            brief.display()
        )
    })?;

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
        return Err(format!(
            "cannot derive a class name from {}",
            brief.display()
        ));
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
    out += &format!(
        "@DisplayName(\"{}\")\n",
        clean_markdown(&brief.file_stem().unwrap_or_default().to_string_lossy())
    );
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
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
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
        assert_eq!(field_type("text").unwrap(), ("String", None));
        assert_eq!(field_type("int").unwrap().0, "Integer");
        assert_eq!(field_type("integer").unwrap().0, "Integer");
        assert_eq!(field_type("long").unwrap().0, "Long");
        assert_eq!(field_type("boolean").unwrap().0, "Boolean");
        assert_eq!(field_type("double").unwrap().0, "Double");
        assert_eq!(
            field_type("uuid").unwrap(),
            ("UUID", Some("java.util.UUID"))
        );
        assert_eq!(
            field_type("currency").unwrap(),
            ("Currency", Some("java.util.Currency"))
        );
        assert_eq!(
            field_type("date").unwrap(),
            ("LocalDate", Some("java.time.LocalDate"))
        );
        assert_eq!(
            field_type("datetime").unwrap(),
            ("LocalDateTime", Some("java.time.LocalDateTime"))
        );
    }

    #[test]
    fn field_type_rejects_unknown_tokens() {
        assert!(field_type("nope").is_err());
    }

    #[test]
    fn parse_fields_splits_name_and_type() {
        let fields = parse_fields(&["title:string".to_string(), "body:Text".to_string()]).unwrap();
        assert_eq!(fields[0].name, "title");
        assert_eq!(fields[0].java_type, "String");
        // Capitalised means "a type this project owns", so `Text` is no longer
        // the built-in -- that is the whole point of the rule.
        assert_eq!(fields[1].java_type, "Text");
        assert!(fields[1].owned);
        assert_eq!(
            parse_fields(&["body:text".to_string()]).unwrap()[0].java_type,
            "String"
        );
    }

    /// The Java spellings of the built-in types stay built-in: `id:String`
    /// must not be read as an unknown project type.
    #[test]
    fn parse_fields_treats_java_type_names_as_builtins() {
        let fields = parse_fields(&["id:String".to_string(), "on:LocalDate".to_string()]).unwrap();
        assert!(!fields[0].owned);
        assert_eq!(fields[0].java_type, "String");
        assert!(!fields[1].owned);
        assert!(fields[1].imports.contains(&"java.time.LocalDate"));
    }

    #[test]
    fn resource_path_is_kebab_case_and_plural() {
        assert_eq!(resource_path("WorkItem"), "/work-items");
        assert_eq!(resource_path("Import"), "/imports");
    }

    /// A handler binds, routes and maps outcomes to status codes -- and holds
    /// no rules, so the same service can be driven from the CLI.
    #[test]
    fn handler_maps_outcomes_to_status_codes() {
        let src = handler_java("com.example.demo.api", "WorkItem", "");

        assert!(src.contains("implements HttpHandler"), "{src}");
        assert!(src.contains(r#"PATH = "/work-items""#), "{src}");
        assert!(
            src.contains("private final Service service"),
            "the service is a dependency: {src}"
        );
        assert!(src.contains("error(404"), "{src}");
        assert!(
            src.contains("error(422"),
            "well-formed but rejected is not a 400: {src}"
        );
        assert!(
            src.contains("ApiError"),
            "failures share one envelope: {src}"
        );
        assert!(!src.contains("java.sql"), "no storage in a handler: {src}");
    }

    #[test]
    fn handler_test_drives_it_over_a_real_socket() {
        let test = handler_test("com.example.demo.api", "WorkItem");

        assert!(test.contains("java.net.http.HttpClient"), "{test}");
        assert!(
            test.contains("new InetSocketAddress(0)"),
            "an ephemeral port: {test}"
        );
        assert!(test.contains("isEqualTo(422)"), "{test}");
    }

    /// The whole point of a port: application code must be able to depend on
    /// it without dragging JDBC along -- including in the prose, since a
    /// reader grepping for java.sql should find only the adapter.
    #[test]
    fn repository_port_is_free_of_jdbc() {
        let src = repository_port(
            "com.example.demo.app",
            "Transaction",
            "import com.example.demo.domain.Transaction;\n",
        );

        assert!(
            src.contains("public interface TransactionRepository"),
            "{src}"
        );
        assert!(
            src.contains("Optional<Transaction> findById(String id)"),
            "{src}"
        );
        assert!(src.contains("List<Transaction> findAll()"), "{src}");
        assert!(!src.contains("java.sql"), "not even in a comment: {src}");
    }

    #[test]
    fn jdbc_adapter_uses_plain_jdbc_and_no_orm() {
        let src = jdbc_repository("com.example.demo.adapters", "Transaction", "");

        assert!(src.contains("implements TransactionRepository"), "{src}");
        assert!(src.contains("connection.prepareStatement"), "{src}");
        assert!(src.contains("try (var query"), "try-with-resources: {src}");
        assert!(
            src.contains("order by id"),
            "unordered findAll would flake a test: {src}"
        );
        assert!(
            src.contains("\"\"\""),
            "SQL should be visible in text blocks: {src}"
        );
        for forbidden in ["org.springframework"] {
            assert!(!src.contains(forbidden), "{forbidden} should not appear");
        }
    }

    /// jails cannot know the columns, so map/bind are TODOs -- and a test that
    /// asserts on a TODO is noise until they are written.
    #[test]
    fn jdbc_adapter_test_is_disabled_until_the_mapping_is_written() {
        let test = jdbc_repository_test("com.example.demo.adapters", "Transaction");

        assert!(test.contains("@Disabled"), "{test}");
        assert!(test.contains("class JdbcTransactionRepositoryIT"), "{test}");
        assert!(test.contains("roundTripsThroughTheRealDatabase"), "{test}");
    }

    #[test]
    fn sealed_emits_a_permits_clause_and_a_record_per_variant() {
        let variants = parse_variants(&["verified".to_string(), "timeout".to_string()]).unwrap();
        let src = sealed_java("com.example.demo", "VerificationResult", &variants);

        // Nested variants have to be named qualified in the permits clause.
        assert!(
            src.contains("permits VerificationResult.Verified, VerificationResult.Timeout"),
            "{src}"
        );
        assert!(
            src.contains("record Verified() implements VerificationResult"),
            "{src}"
        );
        assert!(
            src.contains("record Timeout() implements VerificationResult"),
            "{src}"
        );
    }

    /// The companion test switches without a `default`, so adding a variant
    /// breaks it at compile time -- which is the entire reason to seal a type.
    #[test]
    fn sealed_test_switches_exhaustively_without_a_default() {
        let variants = parse_variants(&["ok".to_string(), "failed".to_string()]).unwrap();
        let test = sealed_test("com.example.demo", "Result", &variants);

        assert!(test.contains("switch (result)"), "{test}");
        assert!(test.contains("case Result.Ok v ->"), "{test}");
        assert!(
            !test.contains("default ->"),
            "an exhaustive switch must not have a default: {test}"
        );
    }

    #[test]
    fn parse_variants_rejects_unusable_names() {
        assert!(parse_variants(&[]).is_err());
        assert!(
            parse_variants(&["ok".to_string(), "Ok".to_string()]).is_err(),
            "duplicate after capitalising"
        );
        assert!(parse_variants(&["not a name".to_string()]).is_err());
    }

    #[test]
    fn parse_fields_resolves_collection_types() {
        let fields = parse_fields(&[
            "matched:list<Match>".to_string(),
            "ids:list<string>".to_string(),
            "rates:map<string,double>".to_string(),
            "at:instant".to_string(),
        ])
        .unwrap();

        assert_eq!(fields[0].java_type, "List<Match>");
        assert!(fields[0].collection);
        assert_eq!(fields[1].java_type, "List<String>");
        // Generics cannot hold a primitive, so the element is the wrapper.
        assert_eq!(fields[2].java_type, "Map<String, Double>");
        assert!(fields[2].imports.contains(&"java.util.Map"));
        assert_eq!(fields[3].java_type, "Instant");
        assert!(fields[3].imports.contains(&"java.time.Instant"));
    }

    #[test]
    fn parse_fields_rejects_malformed_collection_types() {
        // A bare `list` would otherwise become List<Object>, silently.
        assert!(parse_fields(&["items:list".to_string()]).is_err());
        assert!(parse_fields(&["items:list<nope>".to_string()]).is_err());
        assert!(parse_fields(&["items:map<string>".to_string()]).is_err());
        assert!(parse_fields(&["items:list<list<string>>".to_string()]).is_err());
        // A collection already models absence; `?` on one is a mistake.
        assert!(parse_fields(&["items:list<string>?".to_string()]).is_err());
    }

    /// A collection component must be copied (so the record is genuinely
    /// immutable) and default to empty (so no consumer has to null-check a
    /// bucket).
    #[test]
    fn collection_components_are_copied_and_default_to_empty() {
        let fields = parse_fields(&[
            "matched:list<Match>".to_string(),
            "rates:map<string,double>".to_string(),
        ])
        .unwrap();
        let src = value_java("com.example.demo", "Result", &fields);

        assert!(src.contains("List<Match> matched"), "{src}");
        assert!(
            src.contains("matched = matched == null ? List.of() : List.copyOf(matched);"),
            "{src}"
        );
        assert!(
            src.contains("rates = rates == null ? Map.of() : Map.copyOf(rates);"),
            "{src}"
        );
        assert!(
            !src.contains("requireNonNull(matched"),
            "a collection is defaulted, not rejected: {src}"
        );
    }

    #[test]
    fn parse_fields_reads_the_optionality_suffixes() {
        let fields = parse_fields(&[
            "id:string!".to_string(),
            "note:string?".to_string(),
            "name:string".to_string(),
            "source:SourceRef?".to_string(),
        ])
        .unwrap();
        assert_eq!(fields[0].optionality, Optionality::NonBlank);
        assert_eq!(fields[1].optionality, Optionality::Nullable);
        assert_eq!(fields[2].optionality, Optionality::Required);
        assert_eq!(fields[3].optionality, Optionality::Nullable);
        assert!(fields[3].owned);
        assert_eq!(fields[3].java_type, "SourceRef");
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
    fn stub_class_emits_a_plain_final_class_with_no_framework_in_it() {
        let src = stub_class("gym", "MoneyMoved");

        assert_eq!(
            src, "package gym;\n\npublic final class MoneyMoved {\n}\n",
            "{src}"
        );
        for forbidden in ["@", "org.springframework", "record "] {
            assert!(
                !src.contains(forbidden),
                "{forbidden} should not appear in a plain class"
            );
        }
    }

    /// The companion test has to compile against the class jails just wrote,
    /// which means constructing it with the implicit no-arg constructor -- the
    /// only one a bare class has.
    #[test]
    fn class_test_constructs_the_class_it_accompanies() {
        let src = class_test("gym", "MoneyMoved");

        assert!(src.contains("class MoneyMovedTest {"), "{src}");
        assert!(
            src.contains("MoneyMoved moneyMoved = new MoneyMoved();"),
            "{src}"
        );
        assert!(src.contains("assertThat(moneyMoved).isNotNull();"), "{src}");
        assert!(src.contains("import org.junit.jupiter.api.Test;"), "{src}");
    }

    #[test]
    fn record_java_emits_a_record_with_a_null_rejecting_compact_constructor() {
        let fields =
            parse_fields(&["amount:long".to_string(), "currency:string".to_string()]).unwrap();
        let src = record_java("com.example.demo", "Money", &fields);

        // Primitive components make null impossible for numeric/boolean values: a
        // `long` cannot be null, so it needs neither the box nor the check.
        assert!(
            src.contains("public record Money(long amount, String currency) {"),
            "{src}"
        );
        assert!(
            src.contains("public Money {"),
            "expected a compact constructor"
        );
        assert!(
            !src.contains("requireNonNull(amount"),
            "a primitive cannot be null"
        );
        assert!(src.contains(r#"Objects.requireNonNull(currency, "currency");"#));
        // Plain Java: no framework persistence annotations.
        for forbidden in ["@", "org.springframework"] {
            assert!(
                !src.contains(forbidden),
                "{forbidden} should not appear in a plain record"
            );
        }
    }

    /// A record whose components are all primitives cannot hold a null, so the
    /// compact constructor would be empty -- and an empty one is noise.
    #[test]
    fn record_java_omits_the_compact_constructor_when_every_component_is_primitive() {
        let fields = parse_fields(&["amount:long".to_string(), "count:int".to_string()]).unwrap();
        let src = record_java("com.example.demo", "Tally", &fields);

        assert!(
            src.contains("public record Tally(long amount, int count) {"),
            "{src}"
        );
        assert!(
            !src.contains("public Tally {"),
            "nothing to validate: {src}"
        );
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
        let fields =
            parse_fields(&["amount:long".to_string(), "currency:string".to_string()]).unwrap();
        let test = record_test(
            Path::new("/nonexistent"),
            "com.example.demo",
            "Money",
            &fields,
        );

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
        let test = record_test(Path::new("/nonexistent"), "com.example.demo", "Marker", &[]);

        assert!(!test.contains("assertThatNullPointerException"));
        assert!(!test.contains(
            "import static org.assertj.core.api.Assertions.assertThatNullPointerException;"
        ));
        assert!(test.contains("new Marker()"));
    }

    #[test]
    fn command_java_returns_an_exit_code_and_never_exits_the_process() {
        let src = command_java("com.example.demo", "Greet");

        assert!(src.contains("public final class GreetCommand"));
        assert!(src.contains(r#"public static final String NAME = "greet";"#));
        assert!(
            src.contains("public static int run(PrintStream out, PrintStream err, String... args)")
        );
        // A CLI command has no business depending on Spring.
        assert!(!src.contains("org.springframework"));

        // The whole point: main owns the exit, so the command stays testable
        // in-process, and output goes to injected streams, not System.out.
        // Only the class body is checked -- the Javadoc deliberately shows a
        // `main` that does call System.exit, since that is where it belongs.
        let body = &src[src.find("public final class").unwrap()..];
        assert!(
            !body.contains("System.exit"),
            "run() must not exit the process"
        );
        assert!(
            !body.contains("System.out"),
            "output should go to the injected stream"
        );
    }

    #[test]
    fn command_test_drives_the_command_through_captured_streams() {
        let test = command_test("com.example.demo", "Greet");

        assert!(test.contains("class GreetCommandTest"));
        assert!(test.contains("ByteArrayOutputStream"));
        assert!(
            test.contains("GreetCommand.run(new PrintStream(out), new PrintStream(err), args)")
        );
        assert!(test.contains("GreetCommand.USAGE_ERROR"));
    }

    #[test]
    fn stub_templates_use_the_package_and_class_name() {
        assert!(
            stub_controller("com.example.blog", "Post").contains("public class PostController")
        );
        assert!(stub_service("com.example.blog", "Post").contains("public class PostService"));
        assert!(
            interface_java("com.example.blog", "PostStore").contains("public interface PostStore")
        );
        assert!(stub_test("com.example.blog", "Post").contains("class PostTest"));
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
        let result = generate(
            ArtifactKind::Scaffold,
            "post",
            &["title:string".to_string()],
            None,
        );
        std::env::set_current_dir(original_cwd).unwrap();
        result.unwrap();

        assert!(
            root.join("src/main/java/com/example/blog/domain/Post.java")
                .is_file()
        );
        assert!(
            root.join("src/test/java/com/example/blog/domain/PostTest.java")
                .is_file()
        );
        assert!(
            root.join("src/main/java/com/example/blog/app/PostRepository.java")
                .is_file()
        );
        assert!(
            root.join("src/main/java/com/example/blog/adapters/JdbcPostRepository.java")
                .is_file()
        );
        assert!(
            root.join("src/test/java/com/example/blog/adapters/JdbcPostRepositoryIT.java")
                .is_file()
        );
        assert!(
            root.join("src/main/java/com/example/blog/service/PostService.java")
                .is_file()
        );
        assert!(
            root.join("src/main/java/com/example/blog/web/PostController.java")
                .is_file()
        );
        assert!(
            root.join("src/test/java/com/example/blog/web/PostControllerTest.java")
                .is_file()
        );

        let adapter = fs::read_to_string(
            root.join("src/main/java/com/example/blog/adapters/JdbcPostRepository.java"),
        )
        .unwrap();
        assert!(
            adapter.contains("import com.example.blog.domain.Post;"),
            "{adapter}"
        );
        assert!(
            adapter.contains("import com.example.blog.app.PostRepository;"),
            "{adapter}"
        );
        assert!(!adapter.contains("org.springframework"), "{adapter}");
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

        assert!(
            root.join("src/main/java/com/example/blog/web/HealthController.java")
                .is_file()
        );
        let test_file = root.join("src/test/java/com/example/blog/web/HealthControllerTest.java");
        assert!(test_file.is_file(), "expected {}", test_file.display());
        assert!(
            fs::read_to_string(test_file)
                .unwrap()
                .contains("class HealthControllerTest")
        );
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

        assert!(
            root.join("src/main/java/com/example/blog/service/BillingService.java")
                .is_file()
        );
        assert!(
            root.join("src/test/java/com/example/blog/service/BillingServiceTest.java")
                .is_file()
        );
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
        let result = generate(ArtifactKind::Repo, "widget", &[], None);
        std::env::set_current_dir(original_cwd).unwrap();
        result.unwrap();

        assert!(
            root.join("src/main/java/com/example/blog/app/WidgetRepository.java")
                .is_file()
        );
        assert!(
            root.join("src/main/java/com/example/blog/adapters/JdbcWidgetRepository.java")
                .is_file()
        );
        assert!(
            root.join("src/test/java/com/example/blog/adapters/JdbcWidgetRepositoryIT.java")
                .is_file()
        );
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
        fs::write(
            src.join("App.java"),
            "package com.example.demo;\n\npublic class App {}\n",
        )
        .unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let record = generate(
            ArtifactKind::Record,
            "money",
            &["amount:long".to_string()],
            None,
        );
        let command = generate(ArtifactKind::Command, "greet", &[], None);
        std::env::set_current_dir(original_cwd).unwrap();
        record.unwrap();
        command.unwrap();

        assert!(
            root.join("src/main/java/com/example/demo/domain/Money.java")
                .is_file()
        );
        assert!(
            root.join("src/test/java/com/example/demo/domain/MoneyTest.java")
                .is_file()
        );
        assert!(
            root.join("src/main/java/com/example/demo/cli/GreetCommand.java")
                .is_file()
        );
        assert!(
            root.join("src/test/java/com/example/demo/cli/GreetCommandTest.java")
                .is_file()
        );
    }

    #[test]
    fn destroy_command_removes_both_of_its_files() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("destroy-command");
        let src = root.join("src/main/java/com/example/demo");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(
            src.join("App.java"),
            "package com.example.demo;\n\npublic class App {}\n",
        )
        .unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        generate(ArtifactKind::Command, "greet", &[], None).unwrap();
        let result = destroy(ArtifactKind::Command, "greet", true, None);
        std::env::set_current_dir(original_cwd).unwrap();

        result.unwrap();
        assert!(!src.join("GreetCommand.java").exists());
        assert!(
            !root
                .join("src/test/java/com/example/demo/GreetCommandTest.java")
                .exists()
        );
        assert!(src.join("App.java").is_file());
    }

    #[test]
    fn duplicate_record_refuses_to_overwrite_the_first() {
        let _guard = CWD_LOCK.lock().unwrap();
        let root = scratch("duplicate-record-paths");
        let src = root.join("src/main/java/com/example/demo");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("pom.xml"), "<project></project>").unwrap();
        fs::write(
            src.join("App.java"),
            "package com.example.demo;\n\npublic class App {}\n",
        )
        .unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        generate(
            ArtifactKind::Record,
            "tag",
            &["name:string".to_string()],
            None,
        )
        .unwrap();
        let clash = generate(
            ArtifactKind::Record,
            "tag",
            &["name:string".to_string()],
            None,
        );
        let result = destroy(ArtifactKind::Record, "tag", true, None);
        std::env::set_current_dir(original_cwd).unwrap();

        assert!(
            clash.is_err(),
            "generate must not overwrite an existing record"
        );
        result.unwrap();
        assert!(!src.join("Tag.java").exists());
        assert!(
            !root
                .join("src/test/java/com/example/demo/TagTest.java")
                .exists()
        );
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
        assert_eq!(
            fs::read_to_string(web.join("CommentController.java")).unwrap(),
            "// already here"
        );
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
        generate(
            ArtifactKind::Record,
            "tag",
            &["name:string".to_string()],
            None,
        )
        .unwrap();
        let result = destroy(ArtifactKind::Record, "tag", true, None);
        std::env::set_current_dir(original_cwd).unwrap();

        result.unwrap();
        assert!(!src.join("Tag.java").is_file());
        assert!(
            !root
                .join("src/test/java/com/example/blog/TagTest.java")
                .exists()
        );
        assert!(src.join("BlogApplication.java").is_file());
    }
}
