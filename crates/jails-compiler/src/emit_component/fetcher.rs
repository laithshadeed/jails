//! `component fetcher <Name>`: bounded, SSRF-safe outbound bytes.
//!
//! A port, a safe adapter and an adversarial test. The adapter is the whole
//! point: fetching a URL a caller supplies is the one outbound call that can
//! be aimed at the host it runs on, so the generated implementation pins a
//! connect and response timeout, a maximum response size, a redirect limit and
//! an allowed content-type list, and refuses anything else. The test is
//! adversarial rather than happy-path for the same reason -- a fetcher that
//! passes "it downloads a page" tells you nothing about the case it exists for.
//!
//! **Its settings are `@Value` defaults in the adapter, not properties.**
//! Every one has a working default, so a project that writes none still gets
//! the bounds; a property file entry would be a value a reader has to keep in
//! step with a class that already states it. That is a deliberate difference
//! from `client`, whose base URL has no default that could work.

use super::{Emitted, Package, java, package};
use crate::CompileError;
use jails_model::{AppModel, Component};

/// Micrometer for the fetch counters the adapter records, and Apache
/// HttpClient because the JDK client follows a redirect to a private address
/// without asking -- the bound the adapter needs is only expressible on a
/// client that lets it inspect each hop.
pub(super) const DEPENDENCIES: &[(&str, &str)] = &[
    ("org.apache.httpcomponents.client5", "httpclient5"),
    ("org.springframework.boot", "spring-boot-starter-actuator"),
];

const PORT: &str = include_str!("../../../../templates/spring/fetcher_port_java.java");
const ADAPTER: &str = include_str!("../../../../templates/spring/safe_fetcher_java.java");
const TEST: &str = include_str!("../../../../templates/spring/safe_fetcher_test_java.java");

pub(super) fn files(model: &AppModel, component: &Component) -> Result<Vec<Emitted>, CompileError> {
    let name = &component.name;
    let pkg = package(model, Package::Clients);
    let property = component.label.replace('_', "-");
    let substitute = |template: &str| {
        template
            .replace("{{pkg}}", &pkg)
            .replace("{{name}}", name)
            .replace("{{property}}", &property)
    };
    Ok(vec![
        java(
            component,
            "port",
            &pkg,
            &format!("{name}Fetcher"),
            false,
            // The port is managed ABI: every generated caller names it.
            false,
            substitute(PORT),
        )?,
        java(
            component,
            "adapter",
            &pkg,
            &format!("Safe{name}Fetcher"),
            false,
            true,
            substitute(ADAPTER),
        )?,
        java(
            component,
            "test",
            &pkg,
            &format!("Safe{name}FetcherTest"),
            true,
            true,
            substitute(TEST),
        )?,
    ])
}
