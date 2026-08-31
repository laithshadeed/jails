//! The HTTP surface: the bare `controller`/`service` stubs and the
//! `handler` that gives one resource a real, thin endpoint.
//!
//! Both stubs are package-private. Spring instantiates and calls them by
//! reflection, so `public` buys nothing and only widens what other packages
//! can compile against.

use jails_spec::spec::kind::HttpMethod;

// ---- standalone stub templates (ported from springgen.nvim) ----

pub(super) fn interface_java(pkg: &str, name: &str) -> String {
    format!("package {pkg};\n\npublic interface {name} {{\n}}\n")
}

pub(super) fn integration_test_java(pkg: &str, name: &str) -> String {
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

/// One route's shape, decided once and read by both the controller and its
/// test.
///
/// A parameter object rather than four arguments threaded twice, and the
/// reason is the failure it removes: the controller and the test have to agree
/// on the verb, the path and whether the test can run at all, and two
/// independent derivations of that is how `g handler Category` came to serve
/// `/categorys` against a table called `categories`.
pub(super) struct Endpoint<'a> {
    pub method: HttpMethod,
    /// What the handler returns. `None` is the stub jails has always emitted:
    /// a `String` echoing the resource name, which is a route that works.
    pub returns: Option<&'a str>,
    /// The `@RequestBody` type, when the verb carries one.
    pub accepts: Option<&'a str>,
    /// `import` lines for the types above, already resolved.
    ///
    /// The `extra` parameter `scaffold` takes, and for the same reason: this
    /// module knows the shape of a route, not where the project keeps its
    /// records. Deciding it here would mean this file second-guessing the
    /// per-layer renames `Config::layers()` owns.
    pub extra: String,
    /// The route this endpoint answers.
    ///
    /// Resolved by the caller rather than derived here: `--path` is a fixed
    /// external contract when there is one and the derived shape when there is
    /// not, and this module should not have to know which. `missing.md` M8.
    pub path: String,
}

impl Endpoint<'_> {
    /// Whether the generated test can actually run.
    ///
    /// The `sample_value` rule, applied to a route: jails has no type model,
    /// so it cannot build a `Verification` to return or a `VerifyRequest` to
    /// post. When either is named the test is emitted whole and `@Disabled`
    /// naming what to do -- a guess would not compile, and emitting nothing
    /// would silently drop the coverage.
    fn is_executable(&self) -> bool {
        self.returns.is_none() && self.body_type().is_none()
    }

    /// The request body type, which is `--on` and only on a verb that has one.
    ///
    /// A body on GET or DELETE is not forbidden by HTTP and is dropped by most
    /// of the stack between the caller and the handler, so a parameter that
    /// silently never binds is worse than no parameter.
    fn body_type(&self) -> Option<&str> {
        self.accepts.filter(|_| self.method.takes_a_body())
    }
}

/// The route a stub controller answers: the caller's, or the one this kind has
/// always derived. `missing.md` M8.
pub(crate) fn route(named: Option<&str>, name: &str) -> String {
    named
        .map(str::to_string)
        .unwrap_or_else(|| format!("/{}", name.to_lowercase()))
}

pub(super) fn stub_controller(pkg: &str, name: &str, endpoint: &Endpoint<'_>) -> String {
    let mut imports = vec![
        format!(
            "import org.springframework.web.bind.annotation.{};",
            endpoint.method.mapping()
        ),
        "import org.springframework.web.bind.annotation.RestController;".to_string(),
    ];
    let parameters = match endpoint.body_type() {
        Some(ty) => {
            imports.push("import org.springframework.web.bind.annotation.RequestBody;".to_string());
            format!("@RequestBody {ty} request")
        }
        None => String::new(),
    };
    let (returns, body) = match endpoint.returns {
        Some(ty) => (
            ty.to_string(),
            format!(
                "throw new UnsupportedOperationException(\n                \"todo: build the \
                 {ty} this route answers with\");"
            ),
        ),
        None => ("String".to_string(), format!("return \"{name}\";")),
    };
    let imports = format!("{}{}", endpoint.extra, imports.join("\n"));
    crate::template::render(
        crate::template_here!("generate/stub_controller.java"),
        &[
            ("pkg", pkg),
            ("name", name),
            ("path", &endpoint.path),
            // Order does not matter: `write_new_file` normalises every import
            // block, which is why templates must never hand-order them.
            ("imports", &imports),
            ("mapping", endpoint.method.mapping()),
            ("handler", endpoint.method.handler_name()),
            ("returns", &returns),
            ("parameters", &parameters),
            ("body", &body),
        ],
    )
}

pub(super) fn stub_service(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

import org.springframework.stereotype.Component;

/**
 * Package-private: Spring injects this by type, and nothing outside this
 * package should be compiling against it. Widen it when something genuinely
 * outside needs it, not before.
 */
@Component
class {name}Service {{
}}
"#
    )
}

pub(super) fn stub_class(pkg: &str, name: &str) -> String {
    format!(
        r#"package {pkg};

public final class {name} {{
}}
"#
    )
}

/// The companion test for `generate class`.
///
/// Constructing the class and asserting `isNotNull()` is three bad things at
/// once: it **passes while the class is entirely broken** (one real project
/// reported 39 green tests over a repository that could not read or write), it
/// inflates the count so the suite looks covered, and passing `null` for a
/// constructor argument teaches that as the pattern. `java.md`
/// §7 -- "don't test getters, records' `equals`, or Spring's wiring" -- is the
/// same rule stated generally.
///
/// So it is `@Disabled` with a name that says what to prove. That is jails'
/// existing idiom for "you have to finish this" (the field-spec sample
/// problem emits `@Disabled` tests for the same reason), and it fixes every
/// one of the three defects: a disabled test is reported as skipped rather
/// than counted as green, so it is visible in the surefire output and cannot
/// masquerade as coverage.
///
/// Deliberately **not** a failing test, which was the other candidate: `jails
/// new` followed by `jails check` would then be red on a project where
/// nothing is wrong, and a red build that is expected is a red build nobody
/// reads.
///
/// The construction is kept. A bare class has an implicit no-arg constructor,
/// so this compiles the moment it is written, and stops compiling the day a
/// real constructor arrives -- which is the prompt to write the real
/// assertion.
pub(super) fn class_test(pkg: &str, name: &str) -> String {
    let victim = lower_first(name);
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

class {name}Test {{

    @Test
    @Disabled("todo: state what {name} is supposed to do, then assert it")
    void todo() {{
        {name} {victim} = new {name}();

        // Replace this with the behaviour {name} exists for. Asserting that
        // it is not null would pass while the class is entirely broken.
    }}
}}
"#
    )
}

pub fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub(super) fn stub_test(pkg: &str, name: &str) -> String {
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

/// The controller's companion test, written against `MockMvcTester` rather
/// than plain `MockMvc`.
///
/// `MockMvcTester` is Spring's AssertJ entry point (`@AutoConfigureMockMvc`
/// contributes one whenever AssertJ is on the classpath, which
/// `spring-boot-starter-test` guarantees). Three things it buys over
/// `mockMvc.perform(get(...)).andExpect(status().isOk())`: the request and
/// the assertions are one fluent chain instead of two families of static
/// imports, an unresolved exception is reported as a failed assertion
/// instead of being thrown, and the test method needs no `throws Exception`
/// -- which is what makes the generated body a thing you extend rather than
/// a thing you first have to reshape.
/// The Spring Boot major at which `MockMvcTester` can be relied on.
///
/// It arrived in Spring Framework 6.2, which is Boot 3.4 -- but `boot_major`
/// reads a major and nothing finer, so the threshold is drawn at 4 rather than
/// guessed at 3-point-something. A Boot 3.4+ project therefore gets the classic
/// entry point it does not strictly need, which costs a fluent chain; a Boot 2
/// project that got the fluent one instead would not compile, and the error
/// would name a package rather than a version.
pub(crate) const MOCKMVC_TESTER_BOOT_MAJOR: u32 = 4;

pub(super) fn controller_stub_test(
    pkg: &str,
    name: &str,
    mockmvc_import: &str,
    endpoint: &Endpoint<'_>,
    boot_major: u32,
) -> String {
    if boot_major < MOCKMVC_TESTER_BOOT_MAJOR {
        return controller_stub_test_classic(pkg, name, mockmvc_import, endpoint);
    }
    let executable = endpoint.is_executable();
    let assertion = match executable {
        true => format!(
            "                .hasStatusOk()\n                .bodyText()\n                \
             .isEqualTo(\"{name}\")"
        ),
        // Status only. The body is whatever the reader writes, and asserting a
        // shape jails invented would be a test of jails' guess.
        false => "                .hasStatus2xxSuccessful()".to_string(),
    };
    crate::template::render(
        crate::template_here!("generate/controller_stub_test.java"),
        &[
            ("pkg", pkg),
            ("name", name),
            ("mockmvc_import", mockmvc_import),
            ("path", &endpoint.path),
            ("handler", endpoint.method.handler_name()),
            ("assertion", &assertion),
            (
                "disabled",
                match executable {
                    true => "",
                    false => concat!(
                        "    @Disabled(\"todo: implement the handler, then delete this ",
                        "@Disabled\")\n"
                    ),
                },
            ),
            (
                "disabled_import",
                match executable {
                    true => "",
                    false => "import org.junit.jupiter.api.Disabled;\n",
                },
            ),
        ],
    )
}

/// The same test for a project whose Spring Framework predates
/// `MockMvcTester`.
///
/// `perform(...).andExpect(...)` is the entry point that has existed since
/// Spring 3 and is still present in 7, so this is the one shape that compiles
/// everywhere -- which is why it is the fallback rather than the other way
/// round. `throws Exception` comes back with it, and that is the honest cost:
/// `perform` declares it.
fn controller_stub_test_classic(
    pkg: &str,
    name: &str,
    mockmvc_import: &str,
    endpoint: &Endpoint<'_>,
) -> String {
    let executable = endpoint.is_executable();
    let assertion = match executable {
        true => format!(
            "\n                .andExpect(status().isOk())\n                \
             .andExpect(content().string(\"{name}\"))"
        ),
        false => "\n                .andExpect(status().is2xxSuccessful())".to_string(),
    };
    let mut rendered = crate::template::render(
        crate::template_here!("generate/controller_stub_test_classic.java"),
        &[
            ("pkg", pkg),
            ("name", name),
            ("mockmvc_import", mockmvc_import),
            ("path", &endpoint.path),
            ("handler", endpoint.method.handler_name()),
            ("assertion", &assertion),
            (
                "disabled",
                match executable {
                    true => "",
                    false => concat!(
                        "    @Disabled(\"todo: implement the handler, then delete this ",
                        "@Disabled\")\n"
                    ),
                },
            ),
            (
                "disabled_import",
                match executable {
                    true => "",
                    false => "import org.junit.jupiter.api.Disabled;\n",
                },
            ),
        ],
    );
    // `content()` is a second static import, and only the executable shape
    // asserts a body. Added here rather than as a placeholder so the template
    // stays a file an editor can compile.
    if executable {
        rendered = rendered.replace(
            "import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;",
            "import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.content;\n\
             import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;",
        );
    }
    rendered
}

pub(super) fn service_stub_test(pkg: &str, name: &str) -> String {
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

// ---- handler: HTTP for one resource, thin by construction. ----

/// `WorkItem` -> `/work-items`. The URL convention is kebab-case and plural,
/// and deriving it beats making every caller remember to type it.
///
/// Through `sql::table_name`, not a second pluraliser: this function used to
/// append a bare `s`, so `g handler Category` served `/categorys` while the
/// very same resource's table was `categories` -- and the Spring scaffold's
/// controller, which does go through `table_name`, disagreed with the
/// framework-free handler about the URL of the same thing.
pub(crate) fn resource_path(name: &str) -> String {
    format!("/{}", crate::sql::table_name(name).replace('_', "-"))
}

pub(super) fn handler_java(pkg: &str, name: &str, extra: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/handler_java.java"),
        &[
            ("pkg", pkg),
            ("name", name),
            ("extra", extra),
            ("path", &resource_path(name)),
        ],
    )
}

pub(super) fn handler_test(pkg: &str, name: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/handler_test_java.java"),
        &[("pkg", pkg), ("name", name), ("path", &resource_path(name))],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(src.contains("import org.junit.jupiter.api.Test;"), "{src}");
        // The three defects of the old `isNotNull()` body: it passed while
        // the class was broken, it counted as coverage, and it taught `null`
        // as a constructor argument.
        assert!(
            !src.contains("isNotNull"),
            "a test that passes over a broken class is worse than no test: {src}"
        );
        assert!(src.contains("@Disabled("), "{src}");
        assert!(
            src.contains("todo: state what MoneyMoved is supposed to do"),
            "the disabled reason has to say what to prove: {src}"
        );
    }

    /// The default endpoint: what `g controller Post` emits with no flags.
    fn plain_endpoint() -> crate::generate::web::Endpoint<'static> {
        crate::generate::web::Endpoint {
            method: jails_spec::spec::kind::HttpMethod::Get,
            returns: None,
            accepts: None,
            extra: String::new(),
            path: "/post".to_string(),
        }
    }

    /// Every verb reaches the annotation, the handler name and the test that
    /// calls it -- and the three agree, which is the whole reason
    /// `web::Endpoint` is one value rather than three derivations.
    #[test]
    fn a_controller_answers_the_method_it_was_asked_for() {
        use jails_spec::spec::kind::HttpMethod;
        for (method, mapping) in [
            (HttpMethod::Post, "PostMapping"),
            (HttpMethod::Put, "PutMapping"),
            (HttpMethod::Patch, "PatchMapping"),
            (HttpMethod::Delete, "DeleteMapping"),
        ] {
            let endpoint = crate::generate::web::Endpoint {
                method,
                ..plain_endpoint()
            };
            let source = stub_controller("com.example.blog", "Post", &endpoint);
            assert!(
                source.contains(&format!("@{mapping}(\"/post\")")),
                "{source}"
            );
            assert!(
                source.contains(&format!(
                    "import org.springframework.web.bind.annotation.{mapping};"
                )),
                "{source}"
            );
            let test = controller_stub_test("com.example.blog", "Post", "x.Y", &endpoint, 4);
            assert!(
                test.contains(&format!("mvc.{}().uri(\"/post\")", method.label())),
                "{test}"
            );
        }
    }

    /// A verb that carries no body must not be given a `@RequestBody`
    /// parameter: it is not forbidden by HTTP and it never binds, so a
    /// parameter there would be a silent nothing.
    #[test]
    fn only_a_verb_with_a_body_takes_a_request_body() {
        use jails_spec::spec::kind::HttpMethod;
        for (method, expected) in [(HttpMethod::Post, true), (HttpMethod::Get, false)] {
            let endpoint = crate::generate::web::Endpoint {
                method,
                accepts: Some("Verify"),
                ..plain_endpoint()
            };
            assert_eq!(
                stub_controller("com.example.blog", "Post", &endpoint).contains("@RequestBody"),
                expected,
                "{method:?}"
            );
        }
    }

    /// The `sample_value` rule, applied to a route: jails cannot build a
    /// `Verification`, so the test is emitted whole and `@Disabled` naming
    /// what to do rather than asserting a body jails invented.
    #[test]
    fn a_route_returning_a_project_type_is_tested_but_disabled() {
        let endpoint = crate::generate::web::Endpoint {
            returns: Some("Verification"),
            ..plain_endpoint()
        };
        let test = controller_stub_test("com.example.blog", "Post", "x.Y", &endpoint, 4);
        assert!(test.contains("@Disabled"), "{test}");
        assert!(
            test.contains("import org.junit.jupiter.api.Disabled;"),
            "{test}"
        );
        assert!(!test.contains("bodyText()"), "{test}");

        let plain = controller_stub_test("com.example.blog", "Post", "x.Y", &plain_endpoint(), 4);
        assert!(!plain.contains("@Disabled"), "{plain}");
        assert!(plain.contains("bodyText()"), "{plain}");
    }

    #[test]
    fn stub_templates_use_the_package_and_class_name() {
        assert!(
            stub_controller("com.example.blog", "Post", &plain_endpoint())
                .contains("class PostController")
        );
        // Package-private: Spring wires these by reflection, so `public` only
        // widens what other packages can compile against.
        assert!(
            stub_service("com.example.blog", "Post").contains("\n@Component\nclass PostService")
        );
        assert!(
            !stub_service("com.example.blog", "Post").contains("public class"),
            "spring.md §2: public only where the type is module API"
        );
        assert!(
            !stub_controller("com.example.blog", "Post", &plain_endpoint())
                .contains("public class"),
            "spring.md §2: public only where the type is module API"
        );
        assert!(
            interface_java("com.example.blog", "PostStore").contains("public interface PostStore")
        );
        assert!(stub_test("com.example.blog", "Post").contains("class PostTest"));
    }
}
