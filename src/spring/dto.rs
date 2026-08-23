//! `generate dto`: the request and response shapes for a domain type.
//!
//! One secret, and it is why this is a module rather than thirteen helpers:
//! **how a domain record becomes a wire type, and back**. Optionality, the
//! primitive/boxed split, which validation annotation a field earns, and the
//! two directions of the mapping all follow from one field spec — which is the
//! same reason `sql.rs` owns one column list rather than five. A hand-written
//! request and response drift, and the drift compiles.
//!
//! `g dto` splices `spring-boot-starter-validation` (or the pinned
//! `jakarta.validation-api`, which is the artifact the annotations actually
//! come from), because handing the reader a compile error for a line they did
//! not write is exactly the plumbing this tool exists to remove.

use super::*;

/// Request/response records for a domain type, plus the mapping between them.
///
/// This is the most-typed, least-thought-about code in a Spring service, and
/// skipping it is worse than writing it: exposing a domain record directly as
/// the API contract means every internal rename is a breaking change for
/// clients, and every new field is published whether or not anyone meant to.
///
/// The request carries bean-validation annotations derived from the field
/// spec jails already has -- a non-null component becomes `@NotNull`, a
/// non-blank one `@NotBlank` -- so a malformed request is rejected at the
/// edge and reported by `add api`'s handler as a 400 naming the field.
pub(crate) fn dto_files(
    slice: &Slice,
    name: &str,
    fields: &[crate::generate::Field],
) -> Vec<Artifact> {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Web);
    let domain: &str = &slice.placed(Layer::Domain);
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    let domain_import = crate::generate::import_of(pkg, domain, name);
    vec![
        Artifact {
            kind: "request",
            path: main.join(format!("{name}Request.java")),
            contents: request_java_for(pkg, name, fields, &domain_import, domain),
        },
        Artifact {
            kind: "response",
            path: main.join(format!("{name}Response.java")),
            contents: response_java_for(pkg, name, fields, &domain_import, domain),
        },
        Artifact {
            kind: "dto test",
            path: test.join(format!("{name}DtoTest.java")),
            contents: dto_test_java(slice, name, fields, &domain_import),
        },
    ]
}

/// Which validation annotation a component earns, from the optionality jails
/// already parsed. Returns the annotation and the import it needs.
fn validation_for(field: &crate::generate::Field) -> Option<(&'static str, &'static str)> {
    use crate::generate::Optionality;
    // A primitive cannot be null, so @NotNull on one is noise at best -- and
    // Hibernate Validator rejects some constraint/type pairings outright.
    if is_primitive(&field.java_type) {
        return None;
    }
    match field.optionality {
        // `!` means non-blank, which only applies to text -- and @NotBlank
        // implies @NotNull, so one annotation covers both.
        Optionality::NonBlank => Some(("@NotBlank", "jakarta.validation.constraints.NotBlank")),
        Optionality::Required => Some(("@NotNull", "jakarta.validation.constraints.NotNull")),
        // `?` is explicitly optional: constraining it would contradict the
        // field spec.
        Optionality::Nullable => None,
    }
}

fn is_primitive(java_type: &str) -> bool {
    matches!(
        java_type,
        "int" | "long" | "double" | "float" | "boolean" | "char" | "byte" | "short"
    )
}

/// The DTO's own component type. An `Optional<T>` domain component becomes a
/// plain nullable `T` on the wire: JSON has `null` and no notion of an
/// absent-vs-null-valued Optional, and Jackson serialising an `Optional`
/// without the JDK8 module produces `{"present":true}` rather than the value.
fn wire_type(field: &crate::generate::Field) -> String {
    // `java_type` is always the inner type; `optionality` says whether the
    // record wraps it. The wire type is the inner one either way.
    field.java_type.clone()
}

/// Imports for the DTO's own components.
///
/// `owner`/`user` are the domain package and the DTO's package: a component
/// whose type the project declares (an enum, most often) needs importing from
/// wherever the domain lives, and `field.imports` cannot carry that because
/// jails only knows the built-in types' packages. Missing it produces a
/// record that names a type it cannot see, which javac catches and no
/// template review does.
fn dto_imports(
    fields: &[crate::generate::Field],
    with_validation: bool,
    owner: &str,
    user: &str,
) -> String {
    let mut imports: Vec<String> = Vec::new();
    for field in fields {
        if field.owned {
            let import = crate::generate::import_of(user, owner, &field.java_type);
            if !import.is_empty() {
                imports.push(
                    import
                        .trim()
                        .trim_start_matches("import ")
                        .trim_end_matches(';')
                        .to_string(),
                );
            }
        }
        for import in &field.imports {
            // Optional itself never reaches the wire type, so its import
            // would be unused -- and an unused import fails `jails check`
            // under a strict formatter.
            if *import == "java.util.Optional" {
                continue;
            }
            imports.push((*import).to_string());
        }
        if with_validation && let Some((_, import)) = validation_for(field) {
            imports.push(import.to_string());
        }
    }
    imports.sort();
    imports.dedup();
    imports
        .into_iter()
        .map(|i| format!("import {i};\n"))
        .collect()
}

fn components(fields: &[crate::generate::Field], with_validation: bool) -> String {
    fields
        .iter()
        .map(|field| {
            let annotation = if with_validation {
                validation_for(field)
                    .map(|(a, _)| format!("{a} "))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            format!("        {annotation}{} {}", wire_type(field), field.name)
        })
        .collect::<Vec<_>>()
        .join(",\n")
}

/// `x.name()` for a plain component, `x.name().orElse(null)` for an Optional
/// one -- the wire type is nullable, so the Optional is unwrapped exactly
/// once, here, rather than at every call site.
fn read_from_domain(field: &crate::generate::Field, receiver: &str) -> String {
    let accessor = format!("{receiver}.{}()", field.name);
    if is_optional(field) {
        format!("{accessor}.orElse(null)")
    } else {
        accessor
    }
}

fn is_optional(field: &crate::generate::Field) -> bool {
    field.optionality == crate::generate::Optionality::Nullable
}

/// The reverse: a nullable wire component becomes an `Optional` again. The
/// generated record's compact constructor normalises a null Optional, so
/// `ofNullable` is enough.
fn write_to_domain(field: &crate::generate::Field) -> String {
    if is_optional(field) {
        format!("Optional.ofNullable({})", field.name)
    } else {
        field.name.clone()
    }
}

fn needs_optional(fields: &[crate::generate::Field]) -> bool {
    fields.iter().any(is_optional)
}

pub(crate) fn request_java_for(
    pkg: &str,
    name: &str,
    fields: &[crate::generate::Field],
    domain_import: &str,
    domain: &str,
) -> String {
    let imports = dto_imports(fields, true, domain, pkg);
    let optional_import = if needs_optional(fields) {
        "import java.util.Optional;\n"
    } else {
        ""
    };
    let components = components(fields, true);
    let arguments = fields
        .iter()
        .map(write_to_domain)
        .map(|a| format!("                {a}"))
        .collect::<Vec<_>>()
        .join(",\n");
    crate::template::render(
        crate::template::template!("spring/request_java_for.java"),
        &[
            ("pkg", pkg),
            ("domain_import", domain_import),
            ("optional_import", optional_import),
            ("imports", &*imports),
            ("name", name),
            ("components", &*components),
            ("arguments", &*arguments),
        ],
    )
}

pub(crate) fn response_java_for(
    pkg: &str,
    name: &str,
    fields: &[crate::generate::Field],
    domain_import: &str,
    domain: &str,
) -> String {
    let imports = dto_imports(fields, false, domain, pkg);
    let components = components(fields, false);
    let arguments = fields
        .iter()
        .map(|field| read_from_domain(field, &crate::generate::lower_first(name)))
        .map(|a| format!("                {a}"))
        .collect::<Vec<_>>()
        .join(",\n");
    let var = crate::generate::lower_first(name);
    crate::template::render(
        crate::template::template!("spring/response_java_for.java"),
        &[
            ("pkg", pkg),
            ("domain_import", domain_import),
            ("imports", &*imports),
            ("name", name),
            ("components", &*components),
            ("var", &*var),
            ("arguments", &*arguments),
        ],
    )
}

/// The round-trip test.
///
/// jails follows one rule for a test it cannot fully write: emit it whole and
/// `@Disabled`, naming what is missing. Emitting a guess would produce a test
/// that does not compile; emitting nothing would drop the coverage silently.
/// Here the guess would be a sample value for a component whose type jails
/// has no model of, so the sample is attempted per component and the whole
/// test is disabled only if some component defeats it.
fn dto_test_java(
    slice: &Slice,
    name: &str,
    fields: &[crate::generate::Field],
    domain_import: &str,
) -> String {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.placed(Layer::Web);
    let domain: &str = &slice.placed(Layer::Domain);
    let var = crate::generate::lower_first(name);
    // A request component is the *wire* type: an Optional domain component is
    // a plain nullable field here, so `Optional.empty()` would not compile as
    // its sample. `null` is the honest wire-level equivalent.
    let samples: Vec<Option<String>> = fields
        .iter()
        .map(|field| {
            if is_optional(field) {
                Some("null".to_string())
            } else {
                crate::generate::sample_value(field, root, domain)
            }
        })
        .collect();
    let unsampleable: Vec<&str> = fields
        .iter()
        .zip(&samples)
        .filter(|(_, sample)| sample.is_none())
        .map(|(field, _)| field.java_type.as_str())
        .collect();

    let disabled = if unsampleable.is_empty() {
        String::new()
    } else {
        format!(
            "@Disabled(\"todo: supply a sample for {} -- jails cannot know how to build one\")\n",
            unsampleable.join(", ")
        )
    };
    let disabled_import = if unsampleable.is_empty() {
        String::new()
    } else {
        "import org.junit.jupiter.api.Disabled;\n".to_string()
    };
    let arguments = fields
        .iter()
        .zip(&samples)
        .map(|(field, sample)| {
            format!(
                "                {}",
                sample
                    .clone()
                    .unwrap_or_else(|| format!("null /* {} */", field.java_type))
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    // The sample literals need the same imports the wire types do
    // (`UUID.fromString`, `Instant.parse`, ...), and `dto_imports` already
    // computes exactly that set with Optional filtered out.
    let sample_imports = dto_imports(fields, false, domain, pkg);

    crate::template::render(
        crate::template::template!("spring/dto_test_java.java"),
        &[
            ("pkg", pkg),
            ("domain_import", domain_import),
            ("sample_imports", &*sample_imports),
            ("disabled_import", &*disabled_import),
            ("disabled", &*disabled),
            ("name", name),
            ("var", &*var),
            ("arguments", &*arguments),
        ],
    )
}
