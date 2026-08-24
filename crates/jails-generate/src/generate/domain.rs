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

pub fn record_java(pkg: &str, name: &str, fields: &[Field]) -> String {
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

/// A fluent test-data builder whose defaults are derived from the same type
/// table as generated tests and fixtures. Unknown project types remain null
/// and `build()` names them; silently guessing would produce a factory that
/// compiles and lies.
pub fn factory_java(
    project: &Project,
    pkg: &str,
    domain: &str,
    name: &str,
    fields: &[Field],
) -> String {
    let mut imports: Vec<&str> = fields
        .iter()
        .flat_map(|field| field.imports.clone())
        .collect();
    if fields
        .iter()
        .any(|field| field.optionality == Optionality::Nullable)
    {
        imports.push("java.util.Optional");
    }
    imports.sort_unstable();
    imports.dedup();
    let domain_import = import_of(pkg, domain, name);
    let mut out = format!("package {pkg};\n\n{domain_import}");
    for import in imports {
        out.push_str(&format!("import {import};\n"));
    }
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
    out.push_str(&format!(
        "/** Mutable test-data builder for {{@link {name}}}. */\n\
         public final class {name}Factory {{\n"
    ));

    let samples = fields
        .iter()
        .map(|field| sample_value(field, project, domain))
        .collect::<Vec<_>>();
    for (field, sample) in fields.iter().zip(&samples) {
        out.push_str(&format!(
            "    private {} {} = {};\n",
            declared_type(field),
            field.name,
            sample.as_deref().unwrap_or("null")
        ));
    }
    out.push_str(&format!(
        "\n    public static {name}Factory a{name}() {{\n\
             return new {name}Factory();\n\
         }}\n"
    ));
    for field in fields {
        out.push_str(&format!(
            "\n    public {name}Factory with{}({} value) {{\n\
                 this.{} = value;\n\
                 return this;\n\
             }}\n",
            capitalize(&field.name),
            declared_type(field),
            field.name
        ));
    }
    out.push_str(&format!("\n    public {name} build() {{\n"));
    for (field, sample) in fields.iter().zip(&samples) {
        if sample.is_none() {
            out.push_str(&format!(
                "        if ({} == null) throw new IllegalStateException(\"{}Factory needs {} ({})\");\n",
                field.name, name, field.name, field.java_type
            ));
        }
    }
    let arguments = fields
        .iter()
        .map(|field| format!("                {}", field.name))
        .collect::<Vec<_>>()
        .join(",\n");
    out.push_str(&format!(
        "        return new {name}(\n{arguments});\n    }}\n}}\n"
    ));
    out
}

/// A companion test asserting the accessors return what was passed and that
/// the compact constructor actually rejects a null.
pub(super) fn record_test(project: &Project, pkg: &str, name: &str, fields: &[Field]) -> String {
    let mut imports: Vec<&str> = fields.iter().flat_map(|f| f.imports.clone()).collect();
    imports.sort();
    imports.dedup();

    // A component whose type this project owns has no literal jails can write
    // -- unless the type is a record jails can read off disk, in which case it
    // builds the constructor call. Rather than invent one it cannot check, the
    // test is generated in full and disabled, naming exactly what it needs.
    let sampled: Vec<Option<(String, Vec<&'static str>)>> = fields
        .iter()
        .map(|f| sample_in_package(f, project, pkg))
        .collect();
    imports.extend(sampled.iter().flatten().flat_map(|(_, needed)| needed));
    imports.sort();
    imports.dedup();
    let samples: Vec<Option<String>> = sampled
        .iter()
        .map(|s| s.as_ref().map(|(value, _)| value.clone()))
        .collect();
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
    let var = lower_first(name);
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
        out += "        assertThatNullPointerException()\n";
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

/// A literal a generated test can construct the component from.
///
/// `None` means jails cannot fabricate one: a type this project owns could
/// have any constructor at all, and guessing produces a test that does not
/// compile. The one case it *can* solve is an enum -- hence `generate enum`
/// pulling its weight twice.
pub fn sample_value(field: &Field, project: &Project, pkg: &str) -> Option<String> {
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
    project
        .declares_enum(pkg, &field.java_type)
        .then(|| format!("{}.values()[0]", field.java_type))
}

/// `sample_value`, plus the one case it cannot answer for callers outside the
/// type's own package: a component whose type is a **record this project
/// already has on disk**.
///
/// jails has no type model, which is why `sample_value` gives up on an owned
/// type -- but it does have the record, and `fields_from_record` already
/// reads it for `g repo` and eight newer kinds. A `value` generated two
/// intents earlier is not an unknown type; refusing to sample it is the tool
/// forgetting what it just wrote. App D hit exactly that: `Entry` carries an
/// `amount:Money` where `Money` is a jails-generated value object, and the
/// companion test arrived `@Disabled` naming a type jails authored itself.
///
/// **Only for code generated into `pkg` itself.** The rendered call is
/// `new Money(...)` with no qualification, so a caller writing a test into
/// the web or service package would get a class it never imported. Those
/// call sites keep `sample_value` and its honest `null`.
///
/// Returns the expression and any imports its *components* need, since the
/// nested literals are not otherwise visible to the file's import list.
pub fn sample_in_package(
    field: &Field,
    project: &Project,
    pkg: &str,
) -> Option<(String, Vec<&'static str>)> {
    if let Some(direct) = sample_value(field, project, pkg) {
        return Some((direct, Vec::new()));
    }
    let sealed_source =
        fs::read_to_string(main_dir(project.root(), pkg).join(format!("{}.java", field.java_type)))
            .ok();
    sealed_source
        .as_deref()
        .and_then(|source| owned_sealed_sample(source, &field.java_type))
        .or_else(|| owned_record_sample(project, pkg, &field.java_type, 3))
}

/// Construct the first zero-component variant of a sealed type Jails wrote.
///
/// This is not a guess about business meaning: the expression is only a
/// non-null, type-correct sample used while testing the enclosing value's own
/// validation. A hand-written sealed hierarchy whose variants carry state is
/// still refused, because no honest component values can be inferred.
fn owned_sealed_sample(source: &str, type_name: &str) -> Option<(String, Vec<&'static str>)> {
    let info = crate::java::type_info(source)?;
    if info.name != type_name {
        return None;
    }
    let source = crate::java::blanked(source);
    let variant = info
        .supertypes
        .into_iter()
        .find(|variant| source.contains(&format!("record {variant}() implements {type_name}")))?;
    Some((format!("new {type_name}.{variant}()"), Vec::new()))
}

/// Build `new Type(a, b)` from the record on disk, recursively.
///
/// Bounded depth rather than a visited set: three levels is deeper than any
/// value object a reader would write, and a record cannot contain itself
/// anyway -- but a *pair* of records referring to each other can, and an
/// unbounded walk would not return.
fn owned_record_sample(
    project: &Project,
    pkg: &str,
    type_name: &str,
    depth: usize,
) -> Option<(String, Vec<&'static str>)> {
    if depth == 0 {
        return None;
    }
    let components = project.record_in(pkg, type_name)?;
    let mut imports: Vec<&'static str> = Vec::new();
    let mut args: Vec<String> = Vec::new();
    for component in &components {
        // Every component has to be fabricable: one that is not would make
        // the whole expression a guess, and a guess that does not compile is
        // worse than a disabled test that says why.
        let (arg, needed) = match sample_value(component, project, pkg) {
            Some(direct) => (direct, Vec::new()),
            None => owned_record_sample(project, pkg, &component.java_type, depth - 1)?,
        };
        imports.extend(component.imports.iter().copied());
        imports.extend(needed);
        args.push(arg);
    }
    // An `Optional` component is spelled `Optional.empty()` in the call, so
    // the import is needed even though no field declares it.
    if components
        .iter()
        .any(|c| c.optionality == Optionality::Nullable)
    {
        imports.push("java.util.Optional");
    }
    imports.sort_unstable();
    imports.dedup();
    Some((format!("new {type_name}({})", args.join(", ")), imports))
}

/// Resolve the fields for every generator that can be driven either from an
/// explicit spec or from an existing domain record.
///
/// The boolean is true when the record on disk was the source. Keeping that
/// fact lets a spanning generator such as `scaffold` reuse the model without
/// claiming (or later destroying) a file it did not create.
pub fn fields_from_spec_or_record(
    project: &Project,
    pkg: &str,
    name: &str,
    spec: &[String],
) -> Result<(Vec<Field>, bool)> {
    let parsed = parse_fields(spec)?;
    if !parsed.is_empty() {
        return Ok((parsed, false));
    }

    project
        .record_in(pkg, name)
        .map(|fields| (fields, true))
        .ok_or_else(|| {
            format!(
                "no {name} record found under {pkg}, and no field spec was given.\n       \
                 fix: run `jails g record {name} <field:type ...>` first, or pass the fields to this command."
            )
        })
}

/// The first constant of a project enum, for a fixture sample. Reads the
/// file rather than guessing: a made-up constant produces a fixture that
/// looks right and fails on the first `valueOf`.
pub fn first_enum_constant(project: &Project, pkg: &str, type_name: &str) -> Option<String> {
    let source = project.source_of(pkg, type_name)?;
    let source = source.as_str();
    let text = crate::java::blanked(source);
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

/// Whether `<Type>.java` in this package declares an enum. Reading the file is
/// the only honest way to know: jails has no type model, and guessing from the
/// name would be worse than admitting ignorance.
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

pub(super) fn value_test(project: &Project, pkg: &str, name: &str, fields: &[Field]) -> String {
    // Only `!` fields are trimmed and blank-checked, so only they have those
    // behaviours to assert.
    let strings: Vec<&Field> = fields.iter().filter(|f| needs_blank_check(f)).collect();

    let mut imports: Vec<&str> = fields.iter().flat_map(|f| f.imports.clone()).collect();
    imports.sort();
    imports.dedup();

    let sampled: Vec<Option<(String, Vec<&'static str>)>> = fields
        .iter()
        .map(|f| sample_in_package(f, project, pkg))
        .collect();
    imports.extend(sampled.iter().flatten().flat_map(|(_, needed)| needed));
    imports.sort();
    imports.dedup();
    let samples: Vec<Option<String>> = sampled
        .iter()
        .map(|s| s.as_ref().map(|(value, _)| value.clone()))
        .collect();
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
