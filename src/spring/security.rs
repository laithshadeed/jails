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
pub(crate) fn security_slice(slice: &Slice) -> Change {
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
                security_test_java(pkg),
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

pub(crate) fn cors_slice(slice: &Slice) -> Change {
    let root: &Path = slice.project().root();
    let pkg: &str = &slice.root_package();
    let main = crate::generate::main_dir(root, pkg);
    let test = crate::generate::test_dir(root, pkg);
    Change {
        deps: Vec::new(),
        files: vec![
            artifact(main.join("CorsConfig.java"), cors_config_java(pkg)),
            artifact(test.join("CorsConfigTest.java"), cors_config_test_java(pkg)),
        ],
        properties: vec![
            "# Exact browser origins; never use `*` together with credentials.".to_string(),
            "app.cors.allowed-origins=http://localhost:3000".to_string(),
        ],
        ..Change::default()
    }
}

fn cors_config_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/cors_config_java.java"),
        &[("pkg", pkg)],
    )
}

fn cors_config_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/cors_config_test_java.java"),
        &[("pkg", pkg)],
    )
}

fn security_config_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/security_config_java.java"),
        &[("pkg", pkg)],
    )
}

fn production_security_config_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/production_security_config_java.java"),
        &[("pkg", pkg)],
    )
}

fn security_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/security_test_java.java"),
        &[("pkg", pkg)],
    )
}

fn scope_authorizer_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/scope_authorizer_java.java"),
        &[("pkg", pkg)],
    )
}

fn scope_authorizer_test_java(pkg: &str) -> String {
    crate::template::render(
        crate::template::template!("spring/scope_authorizer_test_java.java"),
        &[("pkg", pkg)],
    )
}
