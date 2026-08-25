//! `add security` and `add cors`: who may reach what, spelled out.
//!
//! One subject, and one reason they are together: CORS is only meaningful once
//! there is a filter chain to put it in, and a chain written without it fails
//! at the browser rather than in any log.
//!
//! Two things worth knowing before editing. The CORS methods are named
//! **explicitly** rather than through `applyPermitDefaultValues()`, which
//! permits only GET/HEAD/POST and no credentials -- the classic "works until
//! mark-as-read becomes a PUT". And `ScopeAuthorizer` is what makes `@scope`
//! work: `spring::require_scope_authorizer` refuses any scoped operation when
//! this capability has not written one, which is how tenancy exists in jails
//! without the word "tenant" appearing anywhere in core.

use super::*;

/// The security slice.
///
/// Adding `spring-boot-starter-security` on its own changes the application
/// immediately and drastically: every endpoint requires authentication, a
/// login form appears, and a generated password is printed to the log once at
/// startup. That is a safe default and a bewildering one -- the usual first
/// reaction is to search for how to turn it off, which is how applications
/// end up with `permitAll()` on everything.
///
/// So this writes the filter chain explicitly. An explicit chain is readable,
/// reviewable, and testable, and the generated test asserts both directions:
/// anonymous requests are rejected, authenticated ones are not.
pub fn security_slice(slice: &Slice) -> Change {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.root_package();
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    Change {
        deps: vec![SECURITY_STARTER, OAUTH2_RESOURCE_SERVER, SECURITY_TEST],
        files: vec![
            artifact(main.join("SecurityConfig.java"), security_config_java(pkg)),
            artifact(
                main.join("ProductionSecurityConfig.java"),
                production_security_config_java(pkg),
            ),
            artifact(
                main.join("ScopeAuthorizer.java"),
                scope_authorizer_java(pkg),
            ),
            artifact(
                test.join("SecurityConfigTest.java"),
                security_test_java(pkg, slice.project().webmvc_test_import()),
            ),
            artifact(
                test.join("ScopeAuthorizerTest.java"),
                scope_authorizer_test_java(pkg),
            ),
        ],
        properties: Vec::new(),
        ..Change::default()
    }
}

pub fn cors_slice(slice: &Slice) -> Change {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.root_package();
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    Change {
        deps: Vec::new(),
        files: vec![
            artifact(main.join("CorsConfig.java"), cors_config_java(pkg)),
            artifact(
                test.join("CorsConfigTest.java"),
                cors_config_test_java(
                    pkg,
                    slice.project().mockmvc_autoconfigure_import(),
                    slice.project().boot_major(),
                ),
            ),
        ],
        properties: vec![
            "# Exact browser origins; never use `*` together with credentials.".to_string(),
            format!("app.cors.allowed-origins={PLACEHOLDER_ORIGIN}"),
        ],
        ..Change::default()
    }
}

fn cors_config_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/cors_config_java.java"),
        &[("pkg", pkg)],
    )
}

/// The origin `add cors` writes, and the one its test asserts.
///
/// `.invalid` is reserved by RFC 2606 and can never resolve, so this is
/// unmistakably a value somebody has to replace. The previous placeholder was
/// `http://localhost:3000`, which is worse than useless: it looks like a real
/// setting and survives review, and 3000 is the *application's own port* --
/// never a browser origin, so it could not have been right anywhere.
pub(crate) const PLACEHOLDER_ORIGIN: &str = "https://example.invalid";

fn cors_config_test_java(pkg: &str, mockmvc_import: &str, boot_major: u32) -> String {
    // Same threshold and same reason as the controller stub: `MockMvcTester`
    // is Spring Framework 6.2, and the classic entry point compiles against
    // every version jails supports.
    let template = match boot_major >= crate::generate::MOCKMVC_TESTER_BOOT_MAJOR {
        true => crate::template_here!("spring/cors_config_test_java.java"),
        false => crate::template_here!("spring/cors_config_test_classic_java.java"),
    };
    crate::template::render(
        template,
        &[
            ("pkg", pkg),
            ("mockmvc_import", mockmvc_import),
            // One source for the origin: a test asserting a different value
            // from the one the properties declare would fail on a fresh
            // project, which is the first thing anybody runs.
            ("origin", PLACEHOLDER_ORIGIN),
        ],
    )
}

fn security_config_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/security_config_java.java"),
        &[("pkg", pkg)],
    )
}

fn production_security_config_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/production_security_config_java.java"),
        &[("pkg", pkg)],
    )
}

fn security_test_java(pkg: &str, webmvc_test_import: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/security_test_java.java"),
        &[("pkg", pkg), ("webmvc_test_import", webmvc_test_import)],
    )
}

fn scope_authorizer_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/scope_authorizer_java.java"),
        &[("pkg", pkg)],
    )
}

fn scope_authorizer_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/scope_authorizer_test_java.java"),
        &[("pkg", pkg)],
    )
}
