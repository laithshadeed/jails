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
use jails_support::Result;

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
    out += " * <p>Take the whole set as a constructor parameter rather than naming\n \
         * implementations one by one -- that is what makes adding one a matter of\n \
         * writing the class and nothing else:\n";
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
