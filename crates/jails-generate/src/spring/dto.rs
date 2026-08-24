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
pub fn dto_files(slice: &Slice, name: &str, fields: &[crate::generate::Field]) -> Vec<Artifact> {
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

/// The two components `--timestamps` adds, which a client never sends.
///
/// `--timestamps` deliberately expands into ordinary `createdAt`/`updatedAt`
/// components before any recipe sees the flag, so the record, the DDL and the
/// response treat them as spec rather than as a mode. That is right everywhere
/// except the one artifact that says what a *caller* may send: `jails g
/// scaffold --help` promises "the generated create path supplies both", and it
/// did not. They arrived as `@NotNull` wire components, so the documented POST
/// answered 400 naming two fields the caller has no business setting -- and a
/// caller who did set them could backdate a row.
///
/// Recognised by name and type rather than by the flag, because the flag is
/// gone by here on purpose. `generate::with_timestamps` refuses to expand over
/// a hand-declared `createdAt`, so the two spellings cannot mean two different
/// things in one scaffold.
/// Both, or neither.
///
/// The **pair** is `--timestamps`' signature, and `generate::with_timestamps`
/// refuses to expand over a hand-declared `createdAt` or `updatedAt` -- so a
/// scaffold carrying only one of them declared it by hand and means it as data
/// the caller sends. Reading a lone `createdAt` as an audit column would
/// silently drop a component somebody asked for, which is a worse failure than
/// the one this fixes.
pub(crate) fn has_audit_pair(fields: &[crate::generate::Field]) -> bool {
    ["createdAt", "updatedAt"].iter().all(|conventional| {
        fields
            .iter()
            .any(|field| &field.name == conventional && names_an_audit_column(field))
    })
}

fn names_an_audit_column(field: &crate::generate::Field) -> bool {
    matches!(field.name.as_str(), "createdAt" | "updatedAt")
        && field.java_type == "Instant"
        && !is_optional(field)
}

pub(crate) fn is_audit_component(field: &crate::generate::Field, pair: bool) -> bool {
    pair && names_an_audit_column(field)
}

/// The components a client may send: everything the server does not set itself.
pub(crate) fn client_supplied(fields: &[crate::generate::Field]) -> Vec<crate::generate::Field> {
    let pair = has_audit_pair(fields);
    fields
        .iter()
        .filter(|field| !is_audit_component(field, pair))
        .cloned()
        .collect()
}

pub fn request_java_for(
    pkg: &str,
    name: &str,
    fields: &[crate::generate::Field],
    domain_import: &str,
    domain: &str,
) -> String {
    // Imports come from the full spec, not from the wire components: `Instant`
    // is still needed by `Instant.now()` even when no component carries it.
    let imports = dto_imports(fields, true, domain, pkg);
    let optional_import = if needs_optional(fields) {
        "import java.util.Optional;\n"
    } else {
        ""
    };
    let wire = client_supplied(fields);
    let components = components(&wire, true);
    let audited = wire.len() != fields.len();
    let preamble = if audited {
        concat!(
            "        // Audit columns: set here rather than received, and one\n",
            "        // instant for both, so a freshly created row does not look\n",
            "        // already edited.\n",
            "        Instant now = Instant.now();\n",
        )
    } else {
        ""
    };
    let arguments = fields
        .iter()
        .map(|field| {
            if is_audit_component(field, audited) {
                "now".to_string()
            } else {
                write_to_domain(field)
            }
        })
        .map(|a| format!("                {a}"))
        .collect::<Vec<_>>()
        .join(",\n");
    crate::template::render(
        crate::template_here!("spring/request_java_for.java"),
        &[
            ("pkg", pkg),
            ("domain_import", domain_import),
            ("optional_import", optional_import),
            ("imports", &*imports),
            ("name", name),
            ("components", &*components),
            ("preamble", preamble),
            ("arguments", &*arguments),
        ],
    )
}

pub fn response_java_for(
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
        crate::template_here!("spring/response_java_for.java"),
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
    let project = slice.project();
    let pkg: &str = &slice.placed(Layer::Web);
    let domain: &str = &slice.placed(Layer::Domain);
    let var = crate::generate::lower_first(name);
    // The same wire components the request carries -- a sample for one the
    // record does not declare would not compile.
    let fields = &client_supplied(fields)[..];
    // A request component is the *wire* type: an Optional domain component is
    // a plain nullable field here, so `Optional.empty()` would not compile as
    // its sample. `null` is the honest wire-level equivalent.
    let samples: Vec<Option<String>> = fields
        .iter()
        .map(|field| {
            if is_optional(field) {
                Some("null".to_string())
            } else {
                crate::generate::sample_value(field, project, domain)
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
        crate::template_here!("spring/dto_test_java.java"),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(specs: &[&str]) -> Vec<crate::generate::Field> {
        crate::generate::parse_fields(&specs.iter().map(|s| s.to_string()).collect::<Vec<_>>())
            .expect("valid field specs")
    }

    #[test]
    fn the_audit_pair_is_set_by_the_create_path_not_sent_by_the_caller() {
        let java = request_java_for(
            "com.example.demo.web",
            "Note",
            &fields(&[
                "id:uuid@pk",
                "title:string!",
                "createdAt:instant",
                "updatedAt:instant",
            ]),
            "import com.example.demo.domain.Note;\n",
            "com.example.demo.domain",
        );
        // Not a component: `@NotNull Instant createdAt` on the wire is a 400 on
        // the documented POST, and a caller who supplies it backdates the row.
        assert!(!java.contains("Instant createdAt"), "{java}");
        assert!(!java.contains("Instant updatedAt"), "{java}");
        // One instant for both, so a freshly created row does not look edited.
        // Indented as a statement in the method body: a continuation line
        // whose leading whitespace survived into the literal put the comment
        // and the declaration nine columns out, and nothing but reading the
        // generated file would have said so.
        assert!(
            java.contains("\n        Instant now = Instant.now();\n"),
            "{java}"
        );
        for line in java.lines().filter(|line| {
            line.contains("// Audit columns")
                || line.contains("// instant for both")
                || line.contains("// already edited")
        }) {
            assert!(line.starts_with("        //"), "{line:?}");
        }
        assert_eq!(java.matches("                now").count(), 2, "{java}");
        // Still imported: `Instant.now()` needs it even with no component.
        assert!(java.contains("import java.time.Instant;"), "{java}");
    }

    #[test]
    fn a_hand_declared_created_at_alone_is_still_the_callers_to_send() {
        // `--timestamps` writes the pair and refuses to expand over either
        // name, so one on its own was declared by hand and means data.
        let java = request_java_for(
            "com.example.demo.web",
            "Note",
            &fields(&["id:uuid@pk", "title:string!", "createdAt:instant"]),
            "import com.example.demo.domain.Note;\n",
            "com.example.demo.domain",
        );
        assert!(java.contains("Instant createdAt"), "{java}");
        assert!(!java.contains("Instant.now()"), "{java}");
    }

    #[test]
    fn a_scaffold_with_no_timestamps_is_unchanged() {
        let java = request_java_for(
            "com.example.demo.web",
            "Note",
            &fields(&["id:uuid@pk", "title:string!"]),
            "import com.example.demo.domain.Note;\n",
            "com.example.demo.domain",
        );
        assert!(!java.contains("Instant"), "{java}");
        assert!(java.contains("        return new Note("), "{java}");
    }
}
