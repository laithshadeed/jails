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
//!
//! It is *declared* all the same, with a value that is visibly not a secret.
//! `@Value("${app.stripe.secret}")` with nothing declaring the key does not
//! fail safe -- it fails `contextLoads`, so the project does not start at all
//! and the reader is told about a placeholder rather than about a webhook. A
//! line in `application.properties` reading `replace-me` is the same warning
//! delivered where they can act on it.

use super::{Emitted, Package, java, package};
use crate::CompileError;
use crate::emit_java::JavaUnit;
use jails_model::{AppModel, Component};

const VERIFIER: crate::Template = crate::template!("spring/webhook_verifier_java.java");
const CONTROLLER: crate::Template = crate::template!("spring/webhook_controller_java.java");
const TEST: crate::Template = crate::template!("spring/webhook_verifier_test_java.java");

pub(super) fn files(
    model: &AppModel,
    component: &Component,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Vec<Emitted>, CompileError> {
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
    let mut controller = JavaUnit::from_source(
        &CONTROLLER
            .resolve(templates)?
            .replace("{{web}}", &web)
            .replace("{{name}}", name)
            .replace("{{path}}", &path)
            .replace("{{timestamp_header}}", &format!("X-{name}-Timestamp"))
            .replace("{{signature_header}}", &format!("X-{name}-Signature")),
    );
    // Skipped when the two packages are the same, because importing a sibling
    // is a compile error -- which is what `--package ''` produces.
    controller.import_from(&base, &format!("{name}Verifier"));
    Ok(vec![
        java(
            component,
            "verifier",
            &base,
            &format!("{name}Verifier"),
            false,
            true,
            VERIFIER
                .resolve(templates)?
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
            controller,
        )?,
        java(
            component,
            "test",
            &base,
            &format!("{name}VerifierTest"),
            true,
            true,
            TEST.resolve(templates)?
                .replace("{{pkg}}", &base)
                .replace("{{name}}", name),
        )?,
    ])
}

/// The shared secret this webhook verifies against.
///
/// See the module docs: declared so the project starts, with a value nobody
/// could mistake for one.
pub(super) fn properties(component: &Component) -> Vec<super::PropertyEntry> {
    vec![super::PropertyEntry {
        key: format!("app.{}.secret", component.label.replace('_', "-")),
        value: "replace-me-with-the-providers-signing-secret".to_string(),
    }]
}
