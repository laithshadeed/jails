//! The closed and open sets: `enum`, `sealed`, `strategy`.
//!
//! Three kinds and one question — *how many alternatives are there, and who is
//! allowed to add one?* An `enum` is the closed set whose cases carry no data;
//! `sealed` is the closed set whose cases carry different data, which is what
//! makes exhaustive `switch` worth having; `strategy` is the open set, where
//! the compiler cannot help and Spring collects the implementations into a
//! `List<Port>` instead.
//!
//! `strategy`'s failure mode is the quiet kind and the generated Javadoc says
//! so: an implementation missing `@Component` is simply not in the list, so it
//! never runs and nothing reports a problem. Its `destroy` reads implementations
//! off disk rather than from a stored list, which is deliberately *better* —
//! one added by hand after the generate call is still one of this strategy's
//! classes, and leaving it behind implementing a deleted interface stops the
//! project compiling.

use super::*;
use jails_protocol::declaration::ConstantSpec;
use jails_support::Result;

// ---- enum: the closed set of alternatives, and the one owned type whose
// shape jails can work out without being told. ----

/// Enum constants are `SCREAMING_SNAKE_CASE` by convention, and a generated
/// file that ignores the convention is one the reader has to think about.
/// The constants an `enum` recipe declares, through the one parser.
///
/// `ConstantSpec::parse` is where a token becomes `(name, wire)` and where
/// `gbp` becomes `GBP` -- the same parser the ledger's `IntentArguments` uses,
/// so a recorded constant and a generated one cannot be spelled differently.
/// `parse_fields` and `FieldSpec` have exactly this shape and for exactly this
/// reason (`pending.md` §6.3 is what two parsers of one syntax cost).
pub(super) fn parse_constants(args: &[String]) -> Result<Vec<ConstantSpec>> {
    if args.is_empty() {
        return Err(jails_support::Failure::Told(
            "an enum needs at least one constant, e.g. `generate enum Currency GBP EUR`"
                .to_string(),
        ));
    }
    let mut constants: Vec<ConstantSpec> = Vec::new();
    for arg in args {
        let constant = ConstantSpec::parse(arg)?;
        if constants.iter().any(|held| held.name == constant.name) {
            return Err(format!("duplicate enum constant '{}'", constant.name).into());
        }
        constants.push(constant);
    }
    Ok(constants)
}

pub(super) fn enum_java(pkg: &str, name: &str, constants: &[ConstantSpec]) -> String {
    let wired = constants.iter().any(|constant| constant.wire.is_some());
    let mut out = format!("package {pkg};\n\n");
    if wired {
        out += "import com.fasterxml.jackson.annotation.JsonCreator;\n";
        out += "import com.fasterxml.jackson.annotation.JsonValue;\n\n";
    }
    out += "/**\n";
    out += &format!(" * The {name} values this application understands.\n");
    out += " *\n";
    out += " * <p>A closed set, so a switch over it is checked for exhaustiveness and\n";
    out += " * adding a constant makes the compiler point at every place that has to\n";
    out += " * handle it.\n";
    if wired {
        out += " *\n";
        out += " * <p>The name and the wire value are two different things: the database\n";
        out += " * stores the name and the check constraint lists those, while a client\n";
        out += " * sees what {@code wire()} returns.\n";
    }
    out += " */\n";
    out += &format!("public enum {name} {{\n");
    if !wired {
        out += &format!(
            "    {}\n",
            constants
                .iter()
                .map(|constant| constant.name.to_string())
                .collect::<Vec<_>>()
                .join(",\n    ")
        );
        out += "}\n";
        return out;
    }
    out += &format!(
        "    {};\n",
        constants
            .iter()
            .map(|constant| format!("{}(\"{}\")", constant.name, constant.wire_value()))
            .collect::<Vec<_>>()
            .join(",\n    ")
    );
    out += "\n    private final String wire;\n\n";
    out += &format!("    {name}(String wire) {{\n        this.wire = wire;\n    }}\n\n");
    out += "    /** What this constant is called outside the application. */\n";
    out += "    @JsonValue\n";
    out += "    public String wire() {\n        return this.wire;\n    }\n\n";
    out += "    /**\n";
    out += &format!("     * The {name} a client named, by wire value.\n");
    out += "     *\n";
    out += "     * <p>An unknown value throws, listing what it would have taken. A null\n";
    out += "     * return here would be a request body that binds to null and fails\n";
    out += "     * somewhere else entirely.\n";
    out += "     */\n";
    out += "    @JsonCreator\n";
    out += &format!("    public static {name} fromWire(String value) {{\n");
    out += &format!("        for ({name} candidate : values()) {{\n");
    out += "            if (candidate.wire.equals(value)) {\n";
    out += "                return candidate;\n";
    out += "            }\n        }\n";
    out += &format!(
        "        throw new IllegalArgumentException(\n                \"no {name} with wire \
         value '\" + value + \"'; expected one of {}\");\n",
        constants
            .iter()
            .map(|constant| constant.wire_value().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    out += "    }\n";
    out += "}\n";
    out
}

/// The bean that lets a wire value arrive as anything other than a JSON body.
///
/// `@JsonValue` is Jackson's, and Jackson is not what binds a form field, a
/// path variable or a query parameter -- Spring's own conversion service is,
/// and its `StringToEnumConverterFactory` calls `Enum.valueOf`. So a form
/// carrying `status=open` at an endpoint expecting `IssueStatus` is **400**,
/// with a message about a binding failure rather than about the value.
/// Measured against a running server before this existed.
///
/// `None` off Spring, and `None` for an enum whose constants are called their
/// own names: `valueOf` already does that, and a converter restating it is a
/// bean with nothing to do.
pub(super) fn enum_converter_java(
    web: &str,
    domain: &str,
    name: &str,
    constants: &[ConstantSpec],
    spring: bool,
) -> Option<String> {
    if !spring || !constants.iter().any(|constant| constant.wire.is_some()) {
        return None;
    }
    let import = import_of(web, domain, name);
    Some(format!(
        r#"package {web};

{import}import org.springframework.core.convert.converter.Converter;
import org.springframework.stereotype.Component;

/**
 * Reads a {name} from the value a client sends.
 *
 * <p>{{@code @JsonValue}} covers a JSON body and nothing else: a form field, a
 * path variable and a query parameter all go through Spring's conversion
 * service, whose enum converter calls {{@code valueOf}} and therefore knows
 * only the Java names. Without this bean, a request carrying a wire value is a
 * 400 whose message is about binding rather than about the value.
 */
@Component
public final class {name}Converter implements Converter<String, {name}> {{

    @Override
    public {name} convert(String source) {{
        return {name}.fromWire(source);
    }}
}}
"#
    ))
}

pub(super) fn enum_test(pkg: &str, name: &str, constants: &[ConstantSpec]) -> String {
    let first = constants[0].name.to_string();
    let first = &first;
    let wire = wire_round_trip_test(name, constants);
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
{wire}}}
"#,
        count = constants.len()
    )
}

/// The wire round trip, for an enum that has one.
///
/// Empty for an enum whose constants are called their own names -- there is
/// nothing there that `valueOf` does not already cover, and a test asserting
/// `X.wire()` equals `"X"` would pin a method that does not exist.
fn wire_round_trip_test(name: &str, constants: &[ConstantSpec]) -> String {
    if !constants.iter().any(|constant| constant.wire.is_some()) {
        return String::new();
    }
    let mut out = String::new();
    out += "\n    /** The name is what the database stores; this is what a client sees. */\n";
    out += "    @Test\n";
    out += "    void roundTripsEveryWireValue() {\n";
    for constant in constants {
        out += &format!(
            "        assertThat({name}.{}.wire()).isEqualTo(\"{}\");\n",
            constant.name,
            constant.wire_value()
        );
    }
    out += &format!("        for ({name} constant : {name}.values()) {{\n");
    out +=
        &format!("            assertThat({name}.fromWire(constant.wire())).isEqualTo(constant);\n");
    out += "        }\n    }\n\n";
    out += "    /** An unknown wire value throws rather than binding to null. */\n";
    out += "    @Test\n";
    out += "    void rejectsAnUnknownWireValue() {\n";
    out += &format!(
        "        assertThatIllegalArgumentException().isThrownBy(() ->          {name}.fromWire(\"nope\"));\n"
    );
    out += "    }\n";
    out
}

// ---- sealed: the closed set whose cases carry different data, which is the
// one an enum cannot model. ----

pub(super) fn parse_variants(args: &[String]) -> Result<Vec<String>> {
    if args.is_empty() {
        return Err(jails_support::Failure::Told(
            "a sealed type needs at least one variant, e.g. `generate sealed Result Ok Failed`"
                .to_string(),
        ));
    }
    let mut variants: Vec<String> = Vec::new();
    for arg in args {
        let variant = capitalize(arg.trim());
        if variant.is_empty() || !variant.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(format!("'{arg}' is not a usable variant name").into());
        }
        if variants.contains(&variant) {
            return Err(format!("duplicate variant '{variant}'").into());
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
    out += " * var summary = switch (result) {\n";
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
        None => (
            "boolean".to_string(),
            "matches".to_string(),
            format!("{on} {param}"),
        ),
    }
}

pub(super) fn strategy_interface_java(
    pkg: &str,
    name: &str,
    variants: &[String],
    on: &str,
    yields: Option<&str>,
    extra: &str,
) -> String {
    let (ret, method, param) = strategy_method(on, yields);
    let param_name = lower_first(on);
    let mut out = format!("package {pkg};\n\n");
    if yields.is_some() {
        out += "import java.util.Optional;\n";
    }
    // The signature names types this file does not own, and until `--package`
    // existed they were always siblings. `import_of` is empty when they still
    // are, so the ordinary layout is unchanged and the overridden one compiles
    // instead of failing on `cannot find symbol` for a line nobody wrote.
    out += extra;
    if yields.is_some() || !extra.is_empty() {
        out += "\n";
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
        " * <p>{{@link {name}Evaluator}} is where the whole set is taken as one\n \
         * constructor parameter, which is what makes adding an implementation a\n \
         * matter of writing the class and nothing else.\n"
    );
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

/// `g strategy`: the port, the evaluator, and one bean per implementation.
///
/// Extracted whole from `artifacts_for`'s arm, which is the only kind whose
/// arm carried this many decisions -- where the beans live against where the
/// port lives, which types the signature names that jails did not write, and
/// now the order the injected list arrives in.
pub(super) fn strategy_artifacts(
    slice: &crate::model::Slice<'_>,
    name: &str,
    variants: &[String],
    strategy_on: Option<&str>,
    strategy_yields: Option<&str>,
) -> Result<Vec<Artifact>> {
    let project = slice.project();
    let root: &Path = project.root();

    let domain = slice.placed(Layer::Domain);
    let on = strategy_on.ok_or_else(|| {
        format!(
            "`generate strategy` needs the type the strategy examines, e.g. \
                 `jails g strategy {name} Coffee Large --on Transaction --yields Reward`.\n\n\
                 Without it jails would have to invent the one method every \
                 implementation overrides, and every implementation would then have \
                 to be rewritten."
        )
    })?;
    // `project.flavor()`, not `pom::read`: the second opens `pom.xml`
    // whatever the build tool is, so on a Gradle project it answers a
    // confident "not Spring" and every implementation loses its `@Component`
    // -- which is the *silent* half of this kind's failure mode.
    let spring = slice.flavor() == crate::pom::Flavor::SpringBoot;
    // Where `--on` and `--yields` already live. They are somebody
    // else's types, so their home is the conventional one whatever
    // `--package` says about this call's own classes.
    let owner = slice.owned(Layer::Domain);
    let signature = |user: &str| {
        let mut imports = crate::generate::import_of(user, &owner, on);
        if let Some(yields) = strategy_yields {
            imports += &crate::generate::import_of(user, &owner, yields);
        }
        imports
    };
    // A `@Component` in `domain` violates the ArchUnit rule
    // `g scaffold` writes, and the annotation is load-bearing: without
    // it the bean is silently absent from the injected `List<Port>`.
    // Two first-party generators cannot disagree about where the
    // domain boundary is, so the beans live a layer up and the port --
    // which needs no framework at all -- stays where it belongs. On a
    // plain-Maven project there is no annotation and no rule, but the
    // placement stays the same, because one layout is easier to
    // explain than one that depends on the build file.
    let beans = slice.placed(Layer::Service);
    let mut artifacts = vec![Artifact {
        kind: "strategy",
        path: main_dir(root, &domain).join(format!("{name}.java")),
        contents: strategy_interface_java(
            &domain,
            name,
            variants,
            on,
            strategy_yields,
            &signature(&domain),
        ),
    }];
    let mut extra = crate::generate::import_of(&beans, &domain, name);
    extra += &signature(&beans);
    artifacts.push(Artifact {
        kind: "strategy evaluator",
        path: main_dir(root, &beans).join(format!("{name}Evaluator.java")),
        contents: strategy_evaluator_java(&beans, name, on, strategy_yields, spring, &extra),
    });
    for (position, variant) in variants.iter().enumerate() {
        let class = strategy_class(variant, name);
        artifacts.push(Artifact {
            kind: "strategy implementation",
            path: main_dir(root, &beans).join(format!("{class}.java")),
            contents: strategy_impl_java(
                &beans,
                name,
                &class,
                on,
                strategy_yields,
                Bean {
                    spring,
                    order: position + 1,
                },
                &extra,
            ),
        });
        artifacts.push(Artifact {
            kind: "strategy implementation test",
            path: test_dir(root, &beans).join(format!("{class}Test.java")),
            contents: strategy_impl_test(&beans, name, &class, on, strategy_yields),
        });
    }
    Ok(artifacts)
}

/// A Java field name for "all the {name}s", through the one pluraliser.
///
/// `sql::table_name` is it -- `web::resource_path` already delegates there for
/// the same reason. Gluing an `s` on gave `eligibilitys`, and a second
/// pluraliser is how a resource came to be served at `/categorys` out of a
/// table called `categories`.
fn collection_name(name: &str) -> String {
    let plural = crate::sql::table_name(name);
    let mut words = plural.split('_');
    let mut out = words.next().unwrap_or_default().to_string();
    for word in words {
        let mut characters = word.chars();
        if let Some(initial) = characters.next() {
            out.extend(initial.to_uppercase());
            out.push_str(characters.as_str());
        }
    }
    out
}

/// The evaluator the port's Javadoc used to describe and leave to the reader.
///
/// `missing.md`'s smaller entry: `--yields` makes the return shape
/// unambiguous, so the fold is derivable, and every project wrote it by hand.
/// It has no companion test for the same reason `{Name}ClientConfig` has none
/// -- the body is a stream over injected beans, and the logic each rule
/// carries is tested in that rule's own test. What this file adds is the
/// order, which no single rule's test can see.
pub(super) fn strategy_evaluator_java(
    pkg: &str,
    name: &str,
    on: &str,
    yields: Option<&str>,
    spring: bool,
    extra: &str,
) -> String {
    let (_, method, _) = strategy_method(on, yields);
    let param_name = lower_first(on);
    let field = collection_name(name);
    let mut out = format!("package {pkg};\n\nimport java.util.List;\n");
    if yields.is_some() {
        out += "import java.util.Optional;\n";
    }
    out += extra;
    if spring {
        out += "import org.springframework.stereotype.Component;\n";
    }
    out += &format!("\n/**\n * Every {name}, asked about the same {param_name} in one place.\n");
    out += " *\n";
    if spring {
        out += " * <p>The whole set arrives as one constructor parameter, so adding an\n \
                 * implementation is writing the class. The order is {@code @Order}'s and it\n \
                 * decides the answer: a rule that responds to everything has to come last,\n \
                 * or nothing after it is ever reached.\n";
    } else {
        out += " * <p>The whole set arrives as one constructor parameter, in the caller's\n \
                 * order -- which decides the answer: a rule that responds to everything has\n \
                 * to come last, or nothing after it is ever reached.\n";
    }
    out += " */\n";
    if spring {
        out += "@Component\n";
    }
    out += &format!("public final class {name}Evaluator {{\n\n");
    out += &format!("    private final List<{name}> {field};\n\n");
    out += &format!("    public {name}Evaluator(List<{name}> {field}) {{\n");
    out += &format!("        this.{field} = List.copyOf({field});\n");
    out += "    }\n\n";
    match yields {
        Some(out_type) => {
            out += &format!(
                "    /** What the first {name} to answer grants, or empty when none does. */\n"
            );
            out += &format!(
                "    public Optional<{out_type}> first({on} {param_name}) {{\n\
                 \x20       return {field}.stream()\n\
                 \x20               .map(rule -> rule.{method}({param_name}))\n\
                 \x20               .flatMap(Optional::stream)\n\
                 \x20               .findFirst();\n    }}\n\n"
            );
            out += &format!("    /** What every {name} that answers grants, in order. */\n");
            out += &format!(
                "    public List<{out_type}> all({on} {param_name}) {{\n\
                 \x20       return {field}.stream()\n\
                 \x20               .map(rule -> rule.{method}({param_name}))\n\
                 \x20               .flatMap(Optional::stream)\n\
                 \x20               .toList();\n    }}\n"
            );
        }
        None => {
            out += &format!("    /** Whether any {name} matches. */\n");
            out += &format!(
                "    public boolean anyMatch({on} {param_name}) {{\n\
                 \x20       return {field}.stream().anyMatch(rule -> rule.{method}({param_name}));\n    }}\n\n"
            );
            out += &format!("    /** Every {name} that matches, in order. */\n");
            out += &format!(
                "    public List<{name}> matching({on} {param_name}) {{\n\
                 \x20       return {field}.stream().filter(rule -> rule.{method}({param_name})).toList();\n    }}\n"
            );
        }
    }
    out += "}\n";
    out
}

/// How one implementation reaches the injected `List<Port>`: whether it is a
/// bean at all, and where in the list it sits.
///
/// The two travel together because they are one decision -- a plain-Maven
/// project has neither, and a Spring one cannot have the first without the
/// second without leaving the order to component scanning.
#[derive(Clone, Copy)]
pub(super) struct Bean {
    pub spring: bool,
    pub order: usize,
}

pub(super) fn strategy_impl_java(
    pkg: &str,
    name: &str,
    class: &str,
    on: &str,
    yields: Option<&str>,
    bean: Bean,
    extra: &str,
) -> String {
    let Bean { spring, order } = bean;
    let (ret, method, param) = strategy_method(on, yields);
    let param_name = lower_first(on);
    let mut out = format!("package {pkg};\n\n");
    if yields.is_some() {
        out += "import java.util.Optional;\n";
    }
    // The port and the two signature types. On Spring this file is a layer
    // away from all three, which is the point: the bean annotation stays out
    // of `domain` and the port stays framework-free, so the ArchUnit rule
    // `g scaffold` writes and the `@Component` this class needs stop
    // contradicting each other.
    out += extra;
    if spring {
        out += "import org.springframework.core.annotation.Order;\n";
        out += "import org.springframework.stereotype.Component;\n";
    }
    out += "\n/**\n";
    out += &format!(" * TODO: say what makes a {param_name} qualify under {class}.\n");
    if spring {
        out += " *\n";
        out += &format!(
            " * <p>The {{@code @Component}} is load-bearing and its absence is silent:\n \
             * without it this class is simply not in the {{@code List<{name}>}}, so it\n \
             * never runs and nothing reports a problem. {{@code @Order}} is why the list\n \
             * has a defined order at all -- without one it is whatever component scanning\n \
             * happened to produce, so a rule that answers everything can silently come\n \
             * first.\n"
        );
    }
    out += " */\n";
    if spring {
        out += "@Component\n";
        out += &format!("@Order({})\n", order * 10);
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
pub(super) fn strategy_impl_test(
    pkg: &str,
    name: &str,
    class: &str,
    on: &str,
    yields: Option<&str>,
) -> String {
    // A verb per mode rather than the method name with an `s` glued on:
    // `apply` + `s` reads `applys`, and a generated test whose name is
    // misspelled is the first thing anyone sees of the pattern.
    let verb = if yields.is_some() {
        "grants"
    } else {
        "matches"
    };
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
    out +=
        &format!("        // TODO: assert {class} declines a {param_name} it should not match.\n");
    out += "    }\n";
    out += "}\n";
    let _ = name;
    out
}

/// Migrations that re-state a table's `check (col in (…))` after the enum
/// behind it gained a constant.
///
/// **The follow-on question P5.1 creates, answered rather than avoided.**
/// Once the schema carries the closed set, adding a constant to the Java enum
/// and stopping leaves a column that refuses a value every other layer
/// accepts -- a failure at `insert`, in production, about a change that
/// looked like it only touched Java. plan.md P5.2.
///
/// Nothing is emitted when the enum is new: `create_table` carries the set
/// for a table generated after it. Nothing is emitted when the constants are
/// unchanged either, so a re-run is still idempotent.
pub(super) fn closed_set_widening(
    project: &Project,
    domain: &str,
    name: &str,
    constants: &[String],
) -> Result<Vec<Artifact>> {
    let Some(previous) = crate::generate::enum_constants(project, domain, name) else {
        return Ok(Vec::new());
    };
    if previous == constants {
        return Ok(Vec::new());
    }
    // **A removal is refused rather than migrated.** A row may still hold the
    // dropped constant, and jails cannot ask the database from here -- so the
    // `add constraint` would fail at `flyway migrate`, on whichever machine
    // runs it first, about a command that reported success.
    let removed = previous
        .iter()
        .filter(|constant| !constants.contains(constant))
        .cloned()
        .collect::<Vec<_>>();
    if !removed.is_empty() {
        return Err(format!(
            "`{name}` currently allows {}, and this drops {}. A stored row may still hold \
             {}, which jails cannot check from here.\n       \
             fix: keep the constant and stop writing it, or write the migration that proves \
             no row holds it and then re-declare the enum.",
            previous.join(", "),
            removed.join(", "),
            if removed.len() == 1 { "it" } else { "one" }
        )
        .into());
    }

    let values = constants
        .iter()
        .map(|constant| format!("'{constant}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut artifacts = Vec::new();
    for (table, column) in columns_typed_as(project, name) {
        artifacts.push(Artifact {
            kind: "closed-set migration",
            path: crate::generate::migration_file(
                project,
                &format!("allow_{table}_{column}_{}", constants.len()),
            )?,
            contents: format!(
                "-- Forward-only migration: `{name}` gained a constant, and the column that\n\
                 -- stores it has to allow the value before anything writes one.\n\
                 alter table {table}\n  \
                 drop constraint if exists {table}_{column}_allowed;\n\n\
                 alter table {table}\n  \
                 add constraint {table}_{column}_allowed\n  \
                 check ({column} in ({values}));\n"
            ),
        });
    }
    Ok(artifacts)
}

/// Every `(table, column)` that stores this enum, from the records that name
/// it and the migrations that created their tables.
///
/// Read off the source rather than the ledger, for the same reason
/// `destroy strategy` reads the source: a record somebody wrote by hand
/// against a generated table is still a column with this constraint on it.
/// The migration directory is what says a record has a table at all --
/// without that check this would emit `alter table` for a plain `g record`,
/// which is unappliable everywhere and reported nowhere.
fn columns_typed_as(project: &Project, name: &str) -> Vec<(String, String)> {
    let created = project.projected_names_in("src/main/resources/db/migration");
    let mut found = Vec::new();
    for (path, source) in project.projected_main_sources() {
        let Some(stem) = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
        else {
            continue;
        };
        let Some(info) = crate::java::type_info(&source) else {
            continue;
        };
        let table = crate::sql::table_name(&stem);
        if !created
            .iter()
            .any(|migration| migration.ends_with(&format!("__create_{table}.sql")))
        {
            continue;
        }
        for parameter in &info.constructor_params {
            // The simple name, which is what every other projection question
            // here matches on: jails holds no type model, and two types of
            // one name in two packages is not a shape it can tell apart.
            let declared = parameter
                .raw_type
                .strip_prefix("Optional<")
                .and_then(|rest| rest.strip_suffix('>'))
                .unwrap_or(&parameter.raw_type);
            if declared == name {
                found.push((table.clone(), crate::sql::snake_case(&parameter.name)));
            }
        }
    }
    found.sort();
    found.dedup();
    found
}
