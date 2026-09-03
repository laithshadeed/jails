//! Standalone main and test source-unit backend.
//!
//! **Six of the eight kinds are `Recipe` rows** over real `.java` templates in
//! [`crate::emit_java::unit`], which is where the [`Node`] impl, the six
//! recipes and the sealed hierarchy's four fragments live. What is left here
//! is the walk, and the two kinds the criterion in `docs/60-abstraction.md`
//! keeps out of the row table: `strategy`, whose file count depends on how
//! many variants the model declares, and `controller`, whose companion test
//! reaches [`crate::emit_mockmvc`] for a dialect the captured Boot version
//! decides.
//!
//! [`Node`]: crate::recipe::Node

use crate::Diagnostic;
use crate::emit_java::JavaUnit;
use crate::emit_java::unit::{placed, recipe_for};
use jails_contracts::{
    FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree, WorkspaceSnapshot,
};
use jails_model::{AppModel, SourceUnit, StableId, UnitKind};
use std::collections::BTreeSet;

const JAVA_MAIN_ROOT: &str = jails_contracts::SourceRoot::MainJava.path();
const JAVA_TEST_ROOT: &str = jails_contracts::SourceRoot::TestJava.path();

pub(crate) fn emit(
    model: &AppModel,
    output: &mut RenderedTree,
    snapshot: &WorkspaceSnapshot,
) -> Result<(), Diagnostic> {
    let spring_boot = snapshot.project.spring_boot.as_deref();
    for source in model.units.values() {
        if let Some(recipe) = recipe_for(source.kind) {
            crate::recipe::render(model, source, recipe, snapshot, output)?;
            continue;
        }
        for unit in lower(source, model, spring_boot)? {
            output
                .insert(unit.path, unit.file)
                .map_err(crate::refuse::duplicate_emission)?;
        }
    }
    Ok(())
}

struct Unit {
    path: ProjectPath,
    file: RenderedFile,
}

/// The two kinds a recipe row cannot spell.
fn lower(
    source: &SourceUnit,
    model: &AppModel,
    spring_boot: Option<&str>,
) -> Result<Vec<Unit>, Diagnostic> {
    match source.kind {
        UnitKind::Strategy => lower_strategy(source, model, spring_boot.is_some()),
        UnitKind::Controller => lower_controller(source, model, spring_boot),
        UnitKind::Class
        | UnitKind::Interface
        | UnitKind::Service
        | UnitKind::Sealed
        | UnitKind::Test
        | UnitKind::IntegrationTest => unreachable!("every other kind is a recipe row"),
    }
}

/// A strategy port that is not in the domain layer.
///
/// One sentence, so one code and one constructor: `lower_strategy` asks the
/// question twice on one path and a code names one refusal.
fn strategy_package(source: &SourceUnit) -> Diagnostic {
    Diagnostic::without_a_fix(
        "compile-strategy-package-not-canonical",
        format!("$.units.{}", source.label),
        format!(
            "strategy `{}` must use the domain package",
            source.java_type
        ),
    )
}

/// **A strategy is the one Spring-shaped unit a plain project also gets.**
///
/// `@Component` on each implementation is how Spring collects them into the
/// evaluator's `List<Port>`; without Spring the reader passes the list to the
/// constructor themselves, which the evaluator already accepts. Nothing else
/// about the shape changes: plain-Maven projects get the same layout with no
/// annotation, because one placement is easier to explain than one that
/// depends on the build file.
///
/// `refuse.rs` rejects `Service` and `Controller` without Spring, and must not
/// group `Strategy` with them. That is right for the other two, whose whole
/// body is an annotation, and wrong here: it would make `g strategy` refuse
/// on exactly the plain projects it supports.
///
/// The domain-package rule is checked twice on one path -- against the
/// projection, and again when that package is split to name the `service`
/// package beside it -- and both are the one refusal [`strategy_package`]
/// spells.
fn lower_strategy(
    source: &SourceUnit,
    model: &AppModel,
    spring_boot: bool,
) -> Result<Vec<Unit>, Diagnostic> {
    let id = source.id.as_str();
    let domain = &placed(model, source);
    // Compared against the *projection*, so a project that renames `domain`
    // to `core` is asked for `core` rather than refused for not saying
    // `domain`. The rule stands -- a strategy's port belongs in the domain
    // layer, because `g scaffold` writes an ArchUnit rule forbidding Spring
    // inside it -- and only its spelling follows the reader's.
    if domain != &model.project.package_for(jails_model::Package::Domain) {
        return Err(strategy_package(source));
    }
    let base = domain
        .strip_suffix(".domain")
        .ok_or_else(|| strategy_package(source))?;
    let service = format!("{base}.service");
    let name = &source.java_type;
    let on = source.on.as_deref().ok_or_else(|| {
        Diagnostic::without_a_fix(
            "compile-strategy-without-input-type",
            format!("$.units.{}", source.label),
            format!("strategy `{name}` has no input Java type"),
        )
    })?;
    let (return_type, method, empty) = match source.yields.as_deref() {
        Some(yields) => (
            format!("Optional<{yields}>"),
            "evaluate",
            "Optional.empty()",
        ),
        None => ("boolean".to_string(), "appliesTo", "false"),
    };

    let abi_artifact = format!("art_{id}_abi");
    let abi_imports = source
        .yields
        .as_ref()
        .map(|_| BTreeSet::from(["java.util.Optional".to_string()]))
        .unwrap_or_default();
    let abi_body = format!(
        "public interface {name} {{\n\n    {return_type} {method}({on} value);\n\n    // Reader-owned ABI extensions belong below this stable boundary.\n}}"
    );
    let mut units = vec![file(
        JAVA_MAIN_ROOT,
        domain,
        name,
        FileKind::JavaMain,
        JavaUnit::new(domain, &abi_imports, &abi_body).render(&abi_artifact),
        abi_artifact,
        false,
        id,
    )?];

    let evaluator_artifact = format!("art_{id}_evaluator");
    let mut evaluator_imports = BTreeSet::from([
        "java.util.List".to_string(),
        format!("{domain}.{name}"),
        format!("{domain}.{on}"),
    ]);
    if spring_boot {
        evaluator_imports.insert("org.springframework.stereotype.Component".to_string());
    }
    if let Some(yields) = source.yields.as_deref() {
        evaluator_imports.insert("java.util.Optional".to_string());
        evaluator_imports.insert(format!("{domain}.{yields}"));
    }
    let plural = format!("{}s", lower_first(name));
    let evaluation = if source.yields.is_some() {
        format!(
            "return {plural}.stream().flatMap(strategy -> strategy.{method}(value).stream()).findFirst();"
        )
    } else {
        format!("return {plural}.stream().anyMatch(strategy -> strategy.{method}(value));")
    };
    let evaluator = format!("{name}Evaluator");
    let bean = match spring_boot {
        true => "@Component\n",
        false => "",
    };
    let evaluator_body = format!(
        "{bean}public final class {evaluator} {{\n\n    private final List<{name}> {plural};\n\n    public {evaluator}(List<{name}> {plural}) {{\n        this.{plural} = List.copyOf({plural});\n    }}\n\n    public {return_type} {method}({on} value) {{\n        {evaluation}\n    }}\n\n    // Reader-owned evaluator methods belong below this stable boundary.\n}}"
    );
    units.push(file(
        JAVA_MAIN_ROOT,
        &service,
        &evaluator,
        FileKind::JavaMain,
        JavaUnit::new(&service, &evaluator_imports, &evaluator_body).render(&evaluator_artifact),
        evaluator_artifact,
        true,
        id,
    )?);

    for (position, variant) in source.variants.iter().enumerate() {
        let implementation = format!("{variant}{name}");
        let variant_id = lower_first(variant);
        let implementation_artifact = format!("art_{id}_impl_{variant_id}");
        let mut imports = BTreeSet::from([format!("{domain}.{name}"), format!("{domain}.{on}")]);
        if spring_boot {
            imports.insert("org.springframework.core.annotation.Order".to_string());
            imports.insert("org.springframework.stereotype.Component".to_string());
        }
        if let Some(yields) = source.yields.as_deref() {
            imports.insert("java.util.Optional".to_string());
            imports.insert(format!("{domain}.{yields}"));
        }
        // Order is only expressible as an annotation, so without Spring the
        // list arrives in the order the reader passes it -- which the
        // evaluator's Javadoc already says decides the answer.
        let ordering = match spring_boot {
            true => format!("@Component\n@Order({})\n", position + 1),
            false => String::new(),
        };
        let implementation_body = format!(
            "{ordering}public final class {implementation} implements {name} {{\n\n    @Override\n    public {return_type} {method}({on} value) {{\n        return {empty};\n    }}\n\n    // Reader-owned implementation methods belong below this stable boundary.\n}}"
        );
        units.push(file(
            JAVA_MAIN_ROOT,
            &service,
            &implementation,
            FileKind::JavaMain,
            JavaUnit::new(&service, &imports, &implementation_body)
                .render(&implementation_artifact),
            implementation_artifact,
            true,
            id,
        )?);

        let test_artifact = format!("art_{id}_test_{variant_id}");
        let assertion = if source.yields.is_some() {
            format!("assertTrue(new {implementation}().{method}(null).isEmpty());")
        } else {
            format!("assertFalse(new {implementation}().{method}(null));")
        };
        let assertion_import = if source.yields.is_some() {
            "org.junit.jupiter.api.Assertions.assertTrue"
        } else {
            "org.junit.jupiter.api.Assertions.assertFalse"
        };
        let test_body = format!(
            "import org.junit.jupiter.api.Test;\n\nimport static {assertion_import};\n\nclass {implementation}Test {{\n\n    @Test\n    void startsWithNoMatch() {{\n        {assertion}\n    }}\n\n    // Reader-owned tests belong below this stable boundary.\n}}"
        );
        units.push(file(
            JAVA_TEST_ROOT,
            &service,
            &format!("{implementation}Test"),
            FileKind::JavaTest,
            format!(
                "// Generated by jails from {test_artifact}. Clean hand edits survive regeneration.\npackage {service};\n\n{test_body}\n"
            ),
            test_artifact,
            true,
            id,
        )?);
    }
    Ok(units)
}

/// What the generated controller test needs to know about its endpoint.
///
/// A parameter object because six positional arguments of which four are
/// `&str` is the shape this file already gets wrong elsewhere, and because
/// `exercisable` and `spring_boot` are the two that decide everything.
struct ControllerTest<'a> {
    stem: &'a str,
    handler: &'a str,
    path: &'a str,
    /// Whether the test can drive the route and assert its answer.
    exercisable: bool,
    /// The captured Spring Boot version, which picks the MockMvc entry point.
    spring_boot: Option<&'a str>,
}

/// The controller's companion test: a request through the real dispatcher.
///
/// **This asserts the route answers, not that the annotation says so.** A
/// test that reads the mapping back off the handler by reflection holds
/// whenever the annotation is present -- including when the application
/// cannot start, the path collides with another controller, or the method is
/// never dispatched -- and that is the weaker check.
///
/// The request itself and the status it must answer with go through
/// [`crate::emit_mockmvc`], which owns both spellings; this decides only what
/// is asked of *this* route.
fn controller_test(test: ControllerTest<'_>) -> (BTreeSet<String>, String) {
    let ControllerTest {
        stem,
        handler,
        path,
        exercisable,
        spring_boot,
    } = test;
    let boot_major = crate::emit_capability::boot_major(spring_boot);
    let dialect = crate::emit_mockmvc::Dialect::of(spring_boot);

    let mut imports = BTreeSet::from([
        "org.junit.jupiter.api.Test".to_string(),
        "org.springframework.beans.factory.annotation.Autowired".to_string(),
        "org.springframework.boot.test.context.SpringBootTest".to_string(),
        crate::emit_capability::AUTOCONFIGURE_MOCKMVC
            .resolve(boot_major)
            .to_string(),
    ]);
    if !exercisable {
        imports.insert("org.junit.jupiter.api.Disabled".to_string());
    }
    let disabled = if exercisable {
        String::new()
    } else {
        "    @Disabled(\"todo: implement the handler, then delete this @Disabled\")\n".to_string()
    };

    let mvc_type = dialect.tester(&mut imports);
    let throws = dialect.throws();
    // A route whose handler the reader still has to write is held to a status
    // and nothing else: the body is theirs, and asserting a shape jails
    // invented would test jails' guess.
    let invocation = dialect.drive(
        &crate::emit_mockmvc::Drive {
            verb: handler,
            uri: path,
            uri_arguments: "",
            extras: "",
            status: match exercisable {
                true => crate::emit_mockmvc::Status::Ok,
                false => crate::emit_mockmvc::Status::Successful,
            },
            body_text: exercisable.then_some(stem),
            indent: "        ",
        },
        &mut imports,
    );

    let body = format!(
        "@SpringBootTest\n@AutoConfigureMockMvc\nclass {stem}ControllerTest {{\n\n    @Autowired private {mvc_type} mvc;\n\n    @Test\n{disabled}    void {handler}Answers(){throws} {{\n{invocation}\n    }}\n\n    // Reader-owned tests belong below this stable boundary.\n}}"
    );
    (imports, body)
}

fn lower_controller(
    source: &SourceUnit,
    model: &AppModel,
    spring_boot: Option<&str>,
) -> Result<Vec<Unit>, Diagnostic> {
    let endpoint = source.endpoint.as_ref().ok_or_else(|| {
        Diagnostic::without_a_fix(
            "compile-controller-without-endpoint",
            format!("$.units.{}", source.label),
            format!("controller `{}` has no HTTP endpoint", source.java_type),
        )
    })?;
    let id = source.id.as_str();
    let package = &placed(model, source);
    let type_name = &source.java_type;
    let stem = type_name.strip_suffix("Controller").unwrap_or(type_name);
    let domain = model.project.package_for(jails_model::Package::Domain);
    let mapping = match endpoint.method {
        jails_model::EndpointMethod::Get => "GetMapping",
        jails_model::EndpointMethod::Post => "PostMapping",
        jails_model::EndpointMethod::Put => "PutMapping",
        jails_model::EndpointMethod::Patch => "PatchMapping",
        jails_model::EndpointMethod::Delete => "DeleteMapping",
    };
    let handler = match endpoint.method {
        jails_model::EndpointMethod::Get => "get",
        jails_model::EndpointMethod::Post => "post",
        jails_model::EndpointMethod::Put => "put",
        jails_model::EndpointMethod::Patch => "patch",
        jails_model::EndpointMethod::Delete => "delete",
    };
    let mut imports = BTreeSet::from([
        "org.springframework.web.bind.annotation.RestController".to_string(),
        format!("org.springframework.web.bind.annotation.{mapping}"),
    ]);
    let parameter = match endpoint.accepts.as_deref() {
        Some(request) => {
            let binding = match endpoint.consumes {
                jails_model::RequestFormat::Json => "RequestBody",
                jails_model::RequestFormat::Form => "ModelAttribute",
            };
            imports.insert(format!("org.springframework.web.bind.annotation.{binding}"));
            imports.insert(format!("{domain}.{request}"));
            format!("@{binding} {request} request")
        }
        None => String::new(),
    };
    let (return_type, body) = match endpoint.returns.as_deref() {
        Some(response) => {
            imports.insert(format!("{domain}.{response}"));
            (
                response.to_string(),
                format!(
                    "throw new UnsupportedOperationException(\"todo: build the {response} this route returns\");"
                ),
            )
        }
        None => ("String".to_string(), format!("return \"{stem}\";")),
    };
    let consumes = match (endpoint.accepts.as_ref(), endpoint.consumes) {
        (Some(_), jails_model::RequestFormat::Json) => {
            imports.insert("org.springframework.http.MediaType".to_string());
            ", consumes = MediaType.APPLICATION_JSON_VALUE"
        }
        (Some(_), jails_model::RequestFormat::Form) => {
            imports.insert("org.springframework.http.MediaType".to_string());
            ", consumes = MediaType.APPLICATION_FORM_URLENCODED_VALUE"
        }
        (None, _) => "",
    };
    let ejection_id = format!("art_{id}_http");
    let controller_artifact = format!("{ejection_id}_controller");
    let controller_body = format!(
        "@RestController\npublic final class {type_name} {{\n\n    @{mapping}(path = \"{}\"{consumes})\n    public {return_type} {handler}({parameter}) {{\n        {body}\n    }}\n\n    // Reader-owned controller methods belong below this stable boundary.\n}}",
        endpoint.path
    );
    let mut controller = file(
        JAVA_MAIN_ROOT,
        package,
        type_name,
        FileKind::JavaMain,
        JavaUnit::new(package, &imports, &controller_body).render(&controller_artifact),
        controller_artifact,
        true,
        id,
    )?;
    controller.file.provenance.ejection_id = Some(ejection_id.clone());

    let (test_imports, test_body) = controller_test(ControllerTest {
        stem,
        handler,
        path: &endpoint.path,
        // The endpoint is exercisable exactly when jails can build the
        // request and predict the response: no declared return type, so the
        // handler returns the stem it was generated with, and no request body,
        // which jails has no way to construct. Either one present and the test
        // is emitted whole and `@Disabled` -- a guessed `Verification` would
        // not compile, and emitting nothing would drop the coverage silently.
        exercisable: endpoint.returns.is_none() && endpoint.accepts.is_none(),
        spring_boot,
    });
    let test_artifact = format!("{ejection_id}_test");
    let mut test = file(
        JAVA_TEST_ROOT,
        package,
        &format!("{stem}ControllerTest"),
        FileKind::JavaTest,
        JavaUnit::new(package, &test_imports, &test_body).render(&test_artifact),
        test_artifact,
        true,
        id,
    )?;
    test.file.provenance.ejection_id = Some(ejection_id);
    Ok(vec![controller, test])
}

#[allow(clippy::too_many_arguments)]
fn file(
    root: &str,
    package: &str,
    type_name: &str,
    kind: FileKind,
    rendered: String,
    artifact_id: String,
    ejectable: bool,
    semantic_id: &str,
) -> Result<Unit, Diagnostic> {
    let package_path = package.replace('.', "/");
    let path = crate::refuse::project_path(format!("{root}/{package_path}/{type_name}.java"))?;
    Ok(Unit {
        path,
        file: RenderedFile {
            kind,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable,
                semantic_ids: BTreeSet::from([semantic_id.to_string()]),
                compiler_pass: "java-source-units".to_string(),
            },
        },
    })
}

fn lower_first(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_lowercase().to_string() + characters.as_str()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(exercisable: bool, spring_boot: Option<&str>) -> (BTreeSet<String>, String) {
        controller_test(ControllerTest {
            stem: "Foo",
            handler: "post",
            path: "/foo",
            exercisable,
            spring_boot,
        })
    }

    /// **The test drives the route; it does not read the annotation back.**
    ///
    /// A reflection check asserting
    /// `handler.getAnnotation(PostMapping.class).path()[0]` holds whenever
    /// the annotation is present -- including when the application cannot
    /// start, when two controllers claim the path, and when the method is
    /// never dispatched -- so it is the weaker check.
    #[test]
    fn a_controller_test_issues_a_request_rather_than_reading_the_annotation() {
        let (imports, body) = rendered(true, Some("4.0.0"));
        assert!(body.contains("mvc.post().uri(\"/foo\")"), "{body}");
        assert!(!body.contains("getAnnotation"), "{body}");
        assert!(!body.contains("getDeclaredMethod"), "{body}");
        assert!(imports.contains("org.springframework.test.web.servlet.assertj.MockMvcTester"));
    }

    /// Spring Framework 6.2 is where `MockMvcTester` arrived, so anything
    /// older gets `perform(...)` -- the entry point that has existed since
    /// Spring 3 and still does in 7. `throws Exception` comes with it, because
    /// `perform` declares it.
    #[test]
    fn a_project_older_than_the_assertj_entry_point_gets_the_classic_one() {
        let (imports, body) = rendered(true, Some("2.7.18"));
        assert!(body.contains("mvc.perform(post(\"/foo\"))"), "{body}");
        assert!(body.contains("throws Exception"), "{body}");
        assert!(imports.contains("org.springframework.test.web.servlet.MockMvc"));
        assert!(!imports.contains("org.springframework.test.web.servlet.assertj.MockMvcTester"));
    }

    /// **An unreadable version takes the shape that compiles everywhere.**
    /// `MockMvcTester` in a project that does not have it fails with a missing
    /// package, which names neither the version nor the cause.
    #[test]
    fn an_unknown_version_falls_back_to_the_classic_entry_point() {
        let (_, body) = rendered(true, None);
        assert!(body.contains("mvc.perform(post(\"/foo\"))"), "{body}");
    }

    /// Boot 4 moved `@AutoConfigureMockMvc` with no shim, so the import is
    /// sniffed rather than fixed.
    #[test]
    fn the_autoconfigure_import_follows_the_boot_version() {
        let (modern, _) = rendered(true, Some("4.0.0"));
        assert!(
            modern.contains(
                "org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc"
            )
        );
        let (classic, _) = rendered(true, Some("3.3.5"));
        assert!(classic.contains(
            "org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc"
        ));
    }

    /// A route jails cannot drive is emitted whole and `@Disabled`, asserting
    /// status only.
    ///
    /// The handler throws, or takes a body jails has no way to construct, so
    /// the body is whatever the reader writes -- asserting a shape jails
    /// invented would be a test of jails' guess. Emitting nothing would drop
    /// the coverage silently, and guessing a value would not compile.
    #[test]
    fn a_route_jails_cannot_drive_is_disabled_and_asserts_status_only() {
        for version in ["4.0.0", "2.7.18"] {
            let (imports, body) = rendered(false, Some(version));
            assert!(
                body.contains("@Disabled(\"todo: implement the handler"),
                "{body}"
            );
            assert!(
                imports.contains("org.junit.jupiter.api.Disabled"),
                "{version}"
            );
            assert!(!body.contains("isEqualTo(\"Foo\")"), "{body}");
            assert!(!body.contains("content().string"), "{body}");
            assert!(body.contains("2xxSuccessful"), "{body}");
        }
    }

    /// `content()` is a second static import and only the exercisable shape
    /// asserts a body, so it must not be imported unused.
    #[test]
    fn the_classic_body_matcher_is_imported_only_when_a_body_is_asserted() {
        const CONTENT: &str =
            "static org.springframework.test.web.servlet.result.MockMvcResultMatchers.content";
        let (exercisable, _) = rendered(true, Some("2.7.18"));
        assert!(exercisable.contains(CONTENT));
        let (disabled, _) = rendered(false, Some("2.7.18"));
        assert!(!disabled.contains(CONTENT));
    }
}
