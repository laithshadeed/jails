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
//! any other test can observe. `CLAUDE.md` records the same shape for the
//! scheduler pool size.

use super::{Emitted, Package, java, package};
use crate::CompileError;
use jails_model::{AppModel, Component};

const CONFIG: &str = include_str!("../../../../templates/spring/auth_config_java.java");
const TOKENS: &str = include_str!("../../../../templates/spring/auth_tokens_java.java");
const TEST: &str = include_str!("../../../../templates/spring/auth_tokens_test_java.java");

pub(super) fn files(model: &AppModel, component: &Component) -> Result<Vec<Emitted>, CompileError> {
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
    let substitute = |template: &str| {
        template
            .replace("{{pkg}}", &pkg)
            .replace("{{name}}", name)
            .replace("{{issuer}}", &issuer)
    };
    Ok(vec![
        java(
            component,
            "config",
            &pkg,
            &format!("{name}TokenConfig"),
            false,
            true,
            substitute(CONFIG),
        )?,
        java(
            component,
            "tokens",
            &pkg,
            &format!("{name}Tokens"),
            false,
            true,
            substitute(TOKENS),
        )?,
        java(
            component,
            "test",
            &pkg,
            &format!("{name}TokensTest"),
            true,
            true,
            substitute(TEST),
        )?,
    ])
}
