//! The plain-Java data types: `record`, `value`, `enum`, `sealed` and
//! `strategy`, plus the sample values their companion tests need.
//!
//! `sealed` and `strategy` are counterparts and belong together: the closed
//! set the compiler checks exhaustively, and the open one Spring collects
//! into a `List<Port>`.

use super::*;

// ---- record: an immutable plain-Java data carrier. Same field:type parsing,
// no framework annotations, and a compact constructor so an invalid value cannot be
// constructed in the first place. ----

pub(super) fn record_java(pkg: &str, name: &str, fields: &[Field]) -> String {
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
pub(super) fn record_test(root: &Path, pkg: &str, name: &str, fields: &[Field]) -> String {
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
    // Disabled is needed for an unfabricable sample *or* for the
    // no-validation `todo` body below.
    if !unfabricable.is_empty() || first_reference.is_none() {
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

    // No accessor round-trip test. `assertThat(m.amount()).isEqualTo(1L)` on
    // a record tests that javac generated an accessor, which `java.md` §7
    // names directly ("don't test getters, records' `equals`"). It cannot
    // fail for any reason a reader would want to know about.
    //
    // What *is* worth pinning is the compact constructor: the validation the
    // field spec asked for, which is real behaviour and really can regress.
    // When the record has none, there is nothing honest to assert, so the
    // test says so rather than manufacturing a green tick -- the same
    // reasoning as `class_test`.
    if first_reference.is_none() {
        out += &format!(
            "    @Test\n    @Disabled(\"todo: state what {name} guarantees, then assert it\")\n"
        );
        out += "    void todo() {\n";
        out += &format!("        {name} {var} = new {name}({args});\n\n");
        out += &format!(
            "        // {name} has no validation to pin, so assert on what it is\n        // *for*. Asserting that an accessor returns what was passed in\n        // only tests that javac generated the accessor.\n"
        );
        out += "    }\n";
    }

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


pub(crate) fn sample_value(field: &Field, root: &Path, pkg: &str) -> Option<String> {
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
    is_enum_type(root, pkg, &field.java_type).then(|| format!("{}.values()[0]", field.java_type))
}

/// Whether `<Type>.java` in this package declares an enum. Reading the file is
/// the only honest way to know: jails has no type model, and guessing from the
/// name would be worse than admitting ignorance.
/// The components of a record that already exists on disk, as `Field`s.
///
/// This is what makes `jails g repo Reward` useful on a type you wrote
/// yourself: the record already states every component and its type, so the
/// adapter can be derived from it instead of being handed back as a pile of
/// TODOs. Returns `None` when there is no such file, or when it declares no
/// components -- both mean jails has nothing to derive from and should say
/// so rather than invent columns.
pub(crate) fn fields_from_record(root: &Path, pkg: &str, name: &str) -> Option<Vec<Field>> {
    let path = main_dir(root, pkg).join(format!("{name}.java"));
    let source = fs::read_to_string(path).ok()?;
    let info = crate::java::type_info(&source)?;
    if info.constructor_params.is_empty() {
        return None;
    }
    let fields: Vec<Field> = info
        .constructor_params
        .iter()
        .map(|param| {
            // An `Optional<T>` component is jails' `?` optionality; the rest
            // of the type resolves through the same table `parse_fields` uses,
            // so a hand-written record and a generated one map identically.
            let (java_type, optionality) = match param
                .raw_type
                .strip_prefix("Optional<")
                .and_then(|rest| rest.strip_suffix('>'))
            {
                Some(inner) => (inner.to_string(), Optionality::Nullable),
                None => (param.raw_type.clone(), Optionality::Required),
            };
            let builtin = builtin_by_java_name(&java_type);
            Field {
                name: param.name.clone(),
                // The *inner* type, exactly as `parse_fields` records it:
                // optionality lives in `optionality`, and `component_type`
                // is the one place that wraps it back into an `Optional`.
                // Two representations of the same thing is how a template
                // that works for one source of fields breaks for the other.
                java_type: java_type.clone(),
                imports: builtin.and_then(|(_, import)| import).into_iter().collect(),
                optionality,
                // A record read off disk carries no table markers: the Java
                // type cannot say what the column is. `g repo` on an existing
                // record therefore derives no constraints, which is honest --
                // guessing a primary key from a component called `id` is how
                // a schema ends up with one nobody asked for.
                constraints: Constraints::default(),
                owned: builtin.is_none(),
                collection: java_type.starts_with("List") || java_type.starts_with("Map"),
            }
        })
        .collect();
    Some(fields)
}

/// The first constant of a project enum, for a fixture sample. Reads the
/// file rather than guessing: a made-up constant produces a fixture that
/// looks right and fails on the first `valueOf`.
pub(crate) fn first_enum_constant(root: &Path, pkg: &str, type_name: &str) -> Option<String> {
    let source = fs::read_to_string(main_dir(root, pkg).join(format!("{type_name}.java"))).ok()?;
    let text = crate::java::blanked(&source);
    let body = text.find(&format!("enum {type_name}"))?;
    let open = text[body..].find('{')? + body + 1;
    // Constants come first in an enum body and end at the first `;` or `}`.
    let end = text[open..]
        .find([';', '}'])
        .map(|o| open + o)
        .unwrap_or(text.len());
    source
        .get(open..end)?
        .split(',')
        .map(str::trim)
        .find(|token| {
            !token.is_empty()
                && token
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_uppercase() || c == '_')
        })
        .map(|token| {
            // `GBP("British Pound")` -- the constant is the name, not the
            // whole declaration.
            token
                .split(['(', ' ', '{'])
                .next()
                .unwrap_or(token)
                .to_string()
        })
}

pub(crate) fn is_enum_type(root: &Path, pkg: &str, type_name: &str) -> bool {
    fs::read_to_string(main_dir(root, pkg).join(format!("{type_name}.java")))
        .map(|src| src.contains(&format!("enum {type_name}")))
        .unwrap_or(false)
}

pub(super) fn sample_literal(java_type: &str) -> &'static str {
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

pub(super) fn value_java(pkg: &str, name: &str, fields: &[Field]) -> String {
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

pub(super) fn value_test(root: &Path, pkg: &str, name: &str, fields: &[Field]) -> String {
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
pub(super) fn parse_constants(args: &[String]) -> Result<Vec<String>> {
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

pub(super) fn enum_java(pkg: &str, name: &str, constants: &[String]) -> String {
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

pub(super) fn enum_test(pkg: &str, name: &str, constants: &[String]) -> String {
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


// ---- sealed: the closed set whose cases carry different data, which is the
// one an enum cannot model. ----

pub(super) fn parse_variants(args: &[String]) -> Result<Vec<String>> {
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

pub(super) fn sealed_java(pkg: &str, name: &str, variants: &[String]) -> String {
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

pub(super) fn sealed_test(pkg: &str, name: &str, variants: &[String]) -> String {
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

// ---- strategy: the open set, and the counterpart to `sealed`. ----
//
// `sealed` is a closed set the compiler checks: add a variant and every switch
// stops compiling. `strategy` is the open one -- a port interface, a bean per
// implementation, and Spring collecting them into a `List<Port>` that the
// caller iterates without knowing what is in it.
//
// It earns a generator because the pattern is entirely boilerplate and its
// failure is silent. The interface, the implementations, the annotation on
// each and the `List<Port>` constructor parameter are four things that have to
// agree, and when one implementation is missing its annotation nothing fails:
// the list is simply shorter, the rule never fires, and the first person to
// notice asks why that case is not being rewarded. `~/code/bank/rewards`
// hand-wrote exactly this shape.

/// A type jails knows the Java spelling of, so it is never "missing".
pub(super) fn is_builtin_java_type(ty: &str) -> bool {
    builtin_by_java_name(ty).is_some()
}

/// Which of the named types the project does not declare anywhere under
/// `src/main/java`.
///
/// Deliberately a whole-source-tree check by simple name rather than a
/// package-aware resolve: the generated code and the type it names are in the
/// same package in the common case, and a false "missing" note on a type that
/// is really there would be worse than the compile error it replaces. Java's
/// own built-ins are skipped for the same reason.
pub(super) fn missing_types<'a>(
    root: &Path,
    names: impl IntoIterator<Item = Option<&'a str>>,
) -> Vec<String> {
    let wanted: Vec<&str> = names
        .into_iter()
        .flatten()
        .filter(|n| !is_builtin_java_type(n))
        .collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    let declared: std::collections::HashSet<String> =
        crate::java::source_files(&root.join("src/main/java"))
            .iter()
            .filter_map(|p| fs::read_to_string(p).ok())
            .filter_map(|s| crate::java::type_info(&s).map(|t| t.name))
            .collect();
    wanted
        .into_iter()
        .filter(|n| !declared.contains(*n))
        .map(str::to_string)
        .collect()
}

/// A variant's class name: `Coffee` + `RewardRule` -> `CoffeeRewardRule`.
///
/// A variant that already carries the interface's name keeps it rather than
/// doubling it, for the same reason `strip_redundant_suffix` exists: typing
/// the name the class will actually have is the obvious thing to do.
pub(super) fn strategy_class(variant: &str, name: &str) -> String {
    if variant == name || variant.ends_with(name) {
        variant.to_string()
    } else {
        format!("{variant}{name}")
    }
}

/// The method every implementation overrides.
///
/// With `yields`, the strategy answers "what does this earn?" and returns an
/// `Optional` -- empty is how an implementation declines, which is what lets
/// every implementation see every input. Without it the strategy is a
/// predicate and returns `boolean`.
pub(super) fn strategy_method(on: &str, yields: Option<&str>) -> (String, String, String) {
    let param = lower_first(on);
    match yields {
        Some(out) => (
            format!("Optional<{out}>"),
            "apply".to_string(),
            format!("{on} {param}"),
        ),
        None => ("boolean".to_string(), "matches".to_string(), format!("{on} {param}")),
    }
}

pub(super) fn strategy_interface_java(
    pkg: &str,
    name: &str,
    variants: &[String],
    on: &str,
    yields: Option<&str>,
) -> String {
    let (ret, method, param) = strategy_method(on, yields);
    let param_name = lower_first(on);
    let mut out = format!("package {pkg};\n\n");
    if yields.is_some() {
        out += "import java.util.Optional;\n\n";
    }
    out += "/**\n";
    out += &format!(" * One reason a {param_name} produces a result.\n");
    out += " *\n";
    out += &format!(
        " * <p>An open set: every implementation is a bean, and Spring collects them\n \
         * into a {{@code List<{name}>}}. Implementations are independent and each one\n \
         * sees every input, so more than one may answer.\n"
    );
    out += " *\n";
    out += &format!(
        " * <p>Take the whole set as a constructor parameter rather than naming\n \
         * implementations one by one -- that is what makes adding one a matter of\n \
         * writing the class and nothing else:\n"
    );
    out += " *\n";
    out += " * {@snippet :\n";
    out += &format!(" * private final List<{name}> {}s;\n", lower_first(name));
    out += &format!(
        " * Evaluator(List<{name}> {}s) {{ this.{}s = List.copyOf({}s); }}\n",
        lower_first(name),
        lower_first(name),
        lower_first(name)
    );
    out += " * }\n";
    out += " *\n";
    out += &format!(
        " * <p>Evaluation should be pure -- no clock beyond one the implementation was\n \
         * built with, no database, no network -- so the same {param_name} always\n \
         * yields the same answer.\n"
    );
    out += " */\n";
    out += &format!("public interface {name} {{\n\n");
    if yields.is_some() {
        out += &format!(
            "    /** What this grants, or empty when the {param_name} does not qualify. */\n"
        );
    } else {
        out += &format!("    /** Whether this applies to the given {param_name}. */\n");
    }
    out += &format!("    {ret} {method}({param});\n");
    out += "}\n";
    let _ = variants;
    out
}

pub(super) fn strategy_impl_java(
    pkg: &str,
    name: &str,
    class: &str,
    on: &str,
    yields: Option<&str>,
    spring: bool,
) -> String {
    let (ret, method, param) = strategy_method(on, yields);
    let param_name = lower_first(on);
    let mut out = format!("package {pkg};\n\n");
    if yields.is_some() {
        out += "import java.util.Optional;\n";
    }
    if spring {
        out += "import org.springframework.stereotype.Component;\n";
    }
    out += "\n/**\n";
    out += &format!(" * TODO: say what makes a {param_name} qualify under {class}.\n");
    if spring {
        out += " *\n";
        out += &format!(
            " * <p>The {{@code @Component}} is load-bearing and its absence is silent:\n \
             * without it this class is simply not in the {{@code List<{name}>}}, so it\n \
             * never runs and nothing reports a problem.\n"
        );
    }
    out += " */\n";
    if spring {
        out += "@Component\n";
    }
    out += &format!("public final class {class} implements {name} {{\n\n");
    out += "    @Override\n";
    out += &format!("    public {ret} {method}({param}) {{\n");
    match yields {
        Some(_) => {
            out += &format!(
                "        // TODO: decide whether {param_name} qualifies, and what it earns.\n"
            );
            out += "        return Optional.empty();\n";
        }
        None => {
            out += &format!("        // TODO: decide whether {param_name} qualifies.\n");
            out += "        return false;\n";
        }
    }
    out += "    }\n";
    out += "}\n";
    out
}

/// A `@Disabled` test naming what to prove, per the rule the audit settled:
/// a generated test that passes over an unwritten class inflates the count and
/// teaches the pattern, and a failing one makes a fresh project red.
pub(super) fn strategy_impl_test(pkg: &str, name: &str, class: &str, on: &str, yields: Option<&str>) -> String {
    // A verb per mode rather than the method name with an `s` glued on:
    // `apply` + `s` reads `applys`, and a generated test whose name is
    // misspelled is the first thing anyone sees of the pattern.
    let verb = if yields.is_some() { "grants" } else { "matches" };
    let param_name = lower_first(on);
    let mut out = format!("package {pkg};\n\n");
    out += "import org.junit.jupiter.api.Disabled;\n";
    out += "import org.junit.jupiter.api.Test;\n\n";
    out += "/**\n";
    out += &format!(
        " * {class} is a pure function of its {param_name}, so it needs no context,\n \
         * no container and no mocks -- construct it and call it.\n"
    );
    out += " */\n";
    out += &format!("class {class}Test {{\n\n");
    out += &format!(
        "    @Disabled(\"write {class} first: this names what to prove, it does not prove it\")\n"
    );
    out += "    @Test\n";
    out += &format!("    void {verb}WhenThe{on}Qualifies() {{\n");
    out += &format!("        var {} = new {class}();\n", lower_first(class));
    out += &format!(
        "        // TODO: build a qualifying {param_name} and assert what {class} answers.\n"
    );
    out += "    }\n";
    out += "\n";
    out += &format!(
        "    @Disabled(\"write {class} first: this names what to prove, it does not prove it\")\n"
    );
    out += "    @Test\n";
    out += &format!("    void declinesWhenThe{on}DoesNot() {{\n");
    out += &format!("        var {} = new {class}();\n", lower_first(class));
    out += &format!("        // TODO: assert {class} declines a {param_name} it should not match.\n");
    out += "    }\n";
    out += &format!("}}\n");
    let _ = name;
    out
}

