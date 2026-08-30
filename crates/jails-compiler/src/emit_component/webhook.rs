//! `component webhook <Name>`: an inbound call somebody else makes.
//!
//! A verifier, a controller and a test. The split is the whole design: the
//! verifier is a plain class with no framework in it, so the signature check
//! can be tested without starting a context, and the controller is the thin
//! layer that reads the two headers and hands the raw body over.
//!
//! **The shared secret is a property with no default**, derived from the
//! declaration rather than asked for -- `stripe` becomes `app.stripe.secret`.
//! Derived so `destroy` can find it and two projects spell it the same way,
//! and without a default because a webhook whose secret silently defaults is a
//! webhook anybody can call.

use super::{Emitted, Package, java, package};
use crate::CompileError;
use jails_model::{AppModel, Component};

const VERIFIER: &str = include_str!("../../../../templates/spring/webhook_verifier_java.java");
const CONTROLLER: &str = include_str!("../../../../templates/spring/webhook_controller_java.java");
const TEST: &str = include_str!("../../../../templates/spring/webhook_verifier_test_java.java");

pub(super) fn files(model: &AppModel, component: &Component) -> Result<Vec<Emitted>, CompileError> {
    let name = &component.name;
    // The verifier is framework-free, so it goes in the base package; the
    // controller is an inbound HTTP surface and goes in `web`.
    let base = package(model, Package::Base);
    let web = package(model, Package::Web);
    let property = component.label.replace('_', "-");
    let path = component
        .route
        .as_ref()
        .map(|route| route.path.clone())
        .unwrap_or_else(|| property.clone());
    // Empty when the two packages are the same, because importing a sibling is
    // a compile error -- which is what `--package ''` produces.
    let verifier_import = if web == base {
        String::new()
    } else {
        format!("import {base}.{name}Verifier;\n")
    };
    Ok(vec![
        java(
            component,
            "verifier",
            &base,
            &format!("{name}Verifier"),
            false,
            true,
            VERIFIER
                .replace("{{pkg}}", &base)
                .replace("{{name}}", name)
                .replace("{{property}}", &property),
        )?,
        java(
            component,
            "controller",
            &web,
            &format!("{name}WebhookController"),
            false,
            true,
            CONTROLLER
                .replace("{{web}}", &web)
                .replace("{{name}}", name)
                .replace("{{verifier_import}}", &verifier_import)
                .replace("{{path}}", &path)
                .replace("{{timestamp_header}}", &format!("X-{name}-Timestamp"))
                .replace("{{signature_header}}", &format!("X-{name}-Signature")),
        )?,
        java(
            component,
            "test",
            &base,
            &format!("{name}VerifierTest"),
            true,
            true,
            TEST.replace("{{pkg}}", &base).replace("{{name}}", name),
        )?,
    ])
}
