//! `component socket <Name>`: a WebSocket endpoint.
//!
//! A handler, its registration and a test. Both main files sit in the `web`
//! layer because a socket endpoint is an inbound HTTP surface, and the
//! registration is separate from the handler for the reason every Spring
//! registration here is: the class the reader edits should not also be the
//! class that decides where it is mounted.

use super::{Emitted, Package, java, package};
use crate::CompileError;
use jails_model::{AppModel, Component};

/// Spring's WebSocket support is not in the web starter.
pub(super) const DEPENDENCIES: &[(&str, &str)] =
    &[("org.springframework.boot", "spring-boot-starter-websocket")];

const HANDLER: crate::Template = crate::template!("spring/socket_handler_java.java");
const CONFIG: crate::Template = crate::template!("spring/socket_config_java.java");
const TEST: crate::Template = crate::template!("spring/socket_handler_test_java.java");

pub(super) fn files(
    model: &AppModel,
    component: &Component,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Vec<Emitted>, CompileError> {
    let name = &component.name;
    let pkg = package(model, Package::Web);
    let path = component
        .route
        .as_ref()
        .map(|route| route.path.clone())
        .unwrap_or_else(|| format!("/ws/{}", component.label.replace('_', "-")));
    let substitute = |template: crate::Template| -> Result<String, CompileError> {
        let template = template.resolve(templates)?;
        Ok(template
            .replace("{{pkg}}", &pkg)
            .replace("{{name}}", name)
            .replace("{{path}}", &path))
    };
    Ok(vec![
        java(
            component,
            "handler",
            &pkg,
            &format!("{name}SocketHandler"),
            false,
            true,
            substitute(HANDLER)?,
        )?,
        java(
            component,
            "config",
            &pkg,
            &format!("{name}SocketConfig"),
            false,
            true,
            substitute(CONFIG)?,
        )?,
        java(
            component,
            "test",
            &pkg,
            &format!("{name}SocketHandlerTest"),
            true,
            true,
            substitute(TEST)?,
        )?,
    ])
}
