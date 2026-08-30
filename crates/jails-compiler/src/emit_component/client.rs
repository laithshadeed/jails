//! `component client <Name>`: an outbound HTTP service this application uses.
//!
//! Three files from one declaration, and the reason all three exist is the
//! same as on the legacy side. The interface is what the reader calls; Spring
//! builds the implementation, so the base URL is configuration rather than
//! code. The config class registers it in a group of its own -- one class per
//! client, because `@ImportHttpServices` carries one group name and a shared
//! registration loses every earlier client's configuration. The test drives it
//! against the JDK's own `HttpServer` on an ephemeral port, so it exercises
//! serialization, status handling and the configured base URL rather than a
//! mock's idea of them.
//!
//! **The dependency is not optional and its absence is silent.**
//! `@ImportHttpServices` builds the client proxies without
//! `spring-boot-starter-restclient` -- that half is Framework, not Boot -- so
//! the project compiles and starts, and the first call dies with `URI with
//! undefined scheme`, a message that says nothing about a missing module.
//! `CLAUDE.md` records the hours that cost once.

use super::{Emitted, Package, java, package};
use crate::CompileError;
use jails_contracts::PropertyEntry;
use jails_model::{AppModel, Component};

/// `spring-boot-starter-webmvc` serves HTTP; this calls it. They are separate
/// modules and the starter does not bring this one in.
pub(super) const DEPENDENCIES: &[(&str, &str)] =
    &[("org.springframework.boot", "spring-boot-starter-restclient")];

const INTERFACE: &str = include_str!("../../../../templates/spring/client_interface_java.java");
const CONFIG: &str = include_str!("../../../../templates/spring/client_config_java.java");
const TEST: &str = include_str!("../../../../templates/spring/client_test_java.java");

pub(super) fn files(model: &AppModel, component: &Component) -> Result<Vec<Emitted>, CompileError> {
    let name = &component.name;
    let pkg = package(model, Package::Clients);
    let group = group(&component.label);
    let path = component
        .route
        .as_ref()
        .map(|route| route.path.clone())
        .unwrap_or_else(|| format!("/{}", group));
    let substitute = |template: &str| {
        template
            .replace("{{pkg}}", &pkg)
            .replace("{{name}}", name)
            .replace("{{group}}", &group)
            .replace("{{path}}", &path)
    };
    Ok(vec![
        java(
            component,
            "interface",
            &pkg,
            &format!("{name}Client"),
            false,
            // The interface is the managed ABI the reader calls; ejecting it
            // would hand them a type the compiler stops maintaining while
            // every generated caller still names it.
            false,
            substitute(INTERFACE),
        )?,
        java(
            component,
            "config",
            &pkg,
            &format!("{name}ClientConfig"),
            false,
            true,
            substitute(CONFIG),
        )?,
        java(
            component,
            "test",
            &pkg,
            &format!("{name}ClientTest"),
            true,
            true,
            substitute(TEST),
        )?,
    ])
}

/// The configuration group a client's settings hang off.
///
/// Derived from the stable label rather than the Java name, so a client keeps
/// its property keys when its type is renamed -- the same reason every other
/// projection here reads the label.
fn group(label: &str) -> String {
    label.replace('_', "-")
}

/// The three settings a client cannot work without.
///
/// The base URL is `.invalid` deliberately: RFC 2606 reserves it, so it can
/// never resolve and is unmistakably a value somebody has to replace. The
/// alternative failure is a first call dying on `URI with undefined scheme`,
/// which says nothing about a missing setting.
///
/// Both timeouts are needed and neither is a default: with none, a stalled
/// dependency holds a request thread until the client gives up, and that is
/// never. Connect covers a host that does not answer; read covers one that
/// answers and then stops.
pub(super) fn properties(component: &Component) -> Vec<PropertyEntry> {
    let group = group(&component.label);
    [
        ("base-url", "https://example.invalid"),
        ("connect-timeout", "2s"),
        ("read-timeout", "5s"),
    ]
    .into_iter()
    .map(|(key, value)| PropertyEntry {
        key: format!("spring.http.serviceclient.{group}.{key}"),
        value: value.to_string(),
    })
    .collect()
}
