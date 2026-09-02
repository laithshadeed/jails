//! `component auth <Name>`: signed tokens this application issues and reads.
//!
//! A config, an issuer and a test, all framework-adjacent but in the base
//! package: the token is the application's, not the web layer's.
//!
//! **The test is the artifact that matters.** `JwtTimestampValidator` accepts
//! a token with no `exp` claim, so a token factory that forgets to set one
//! produces credentials that never expire and nothing anywhere reports it —
//! the application works. The generated issuer sets it and the generated test
//! is what keeps the fix in place, because removing it changes no behaviour
//! any other test can observe.

use super::{Emitted, Package, java, package};
use crate::CompileError;
use jails_model::{AppModel, Component};

const CONFIG: crate::Template = crate::template!("spring/auth_config_java.java");
const TOKENS: crate::Template = crate::template!("spring/auth_tokens_java.java");
const TEST: crate::Template = crate::template!("spring/auth_tokens_test_java.java");

pub(super) fn files(
    model: &AppModel,
    component: &Component,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Vec<Emitted>, CompileError> {
    // The encoder, the decoder and the filter chain that reads the token are
    // one story, and `cap security` is where the other two live. Checked
    // against the model rather than the pom: in one transition the capability
    // this same model declares has not been written to the build yet.
    if !model
        .capabilities
        .values()
        .any(|capability| capability.kind == "security")
    {
        return Err(CompileError::new(format!(
            "component auth `{}` needs Spring Security: the encoder, the decoder and the filter chain that reads the token are one story\n       fix: declare `cap security` in the model, or run `jails add security`",
            component.name
        )));
    }
    let name = &component.name;
    let pkg = package(model, Package::Base);
    let issuer = format!("urn:{}", model.project.base_package);
    let substitute = |template: crate::Template| -> Result<String, CompileError> {
        let template = template.resolve(templates)?;
        Ok(template
            .replace("{{pkg}}", &pkg)
            .replace("{{name}}", name)
            .replace("{{issuer}}", &issuer))
    };
    Ok(vec![
        java(
            component,
            "config",
            &pkg,
            &format!("{name}TokenConfig"),
            false,
            true,
            substitute(CONFIG)?,
        )?,
        java(
            component,
            "tokens",
            &pkg,
            &format!("{name}Tokens"),
            false,
            true,
            substitute(TOKENS)?,
        )?,
        java(
            component,
            "test",
            &pkg,
            &format!("{name}TokensTest"),
            true,
            true,
            substitute(TEST)?,
        )?,
    ])
}

/// The one property the token configuration reads.
///
/// **A generator that emits code and not the setting it needs hands the reader
/// a project that compiles and refuses to start**, which is the same rule that
/// makes `g client` splice its starter and `g dto` its validation dependency.
/// `ApiTokenConfig` resolves `${app.auth.secret}` in its constructor, so a
/// project with an auth component and no such property fails context
/// initialisation with `Could not resolve placeholder` -- and the test that
/// notices is `contextLoads`, which every generated project ships.
///
/// The value is a placeholder the reader must replace, and it says so: an
/// empty default would be a *silent* weak secret, which is worse than one that
/// is obviously not a secret.
pub(super) fn properties() -> Vec<super::PropertyEntry> {
    vec![super::PropertyEntry {
        key: "app.auth.secret".to_string(),
        value: "replace-me-with-a-32-byte-secret".to_string(),
    }]
}
