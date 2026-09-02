//! Linking for standalone main and test source units.

use super::{Linker, collision};
use crate::id::UnitId;
use crate::naming::upper_camel_case;
use crate::source;
use crate::{EndpointMethod, HttpEndpoint, RequestFormat, SourceUnit, UnitKind};
use std::collections::BTreeMap;

pub(super) fn link(
    declarations: BTreeMap<String, source::Unit>,
    base_package: &str,
    linker: &mut Linker,
) -> BTreeMap<UnitId, SourceUnit> {
    let mut linked = BTreeMap::new();
    let mut java_types = BTreeMap::<String, String>::new();
    for (label, unit) in declarations {
        let path = format!("$.units.{label}");
        linker.label(&label, &path);
        linker.register_id(&unit.id, &format!("{path}.id"));
        let id = linker.stable_id::<UnitId>(&unit.id, &format!("{path}.id"));
        let stem = unit.java_name.unwrap_or_else(|| upper_camel_case(&label));
        linker.java_type_and_variable(&stem, &format!("{path}.java_name"));
        let java_type = match unit.kind {
            UnitKind::Service if !stem.ends_with("Service") => format!("{stem}Service"),
            UnitKind::Test if !stem.ends_with("Test") => format!("{stem}Test"),
            UnitKind::IntegrationTest if !stem.ends_with("IT") => format!("{stem}IT"),
            UnitKind::Controller if !stem.ends_with("Controller") => format!("{stem}Controller"),
            _ => stem,
        };
        let mut seen_variants = BTreeMap::<String, String>::new();
        let variants = unit
            .variants
            .into_iter()
            .enumerate()
            .map(|(index, variant)| {
                let variant = upper_camel_case(&variant);
                let variant_path = format!("{path}.variants[{index}]");
                linker.java_type(&variant, &variant_path);
                collision(
                    linker,
                    &mut seen_variants,
                    &variant,
                    &variant_path,
                    "model-sealed-variant-collision",
                    "sealed variant",
                );
                variant
            })
            .collect::<Vec<_>>();
        match unit.kind {
            UnitKind::Sealed | UnitKind::Strategy if variants.is_empty() => linker.problem(
                "model-unit-variants-empty",
                format!("{path}.variants"),
                "a sealed type or strategy needs at least one variant",
                "declare one or more variants",
            ),
            UnitKind::Class
            | UnitKind::Interface
            | UnitKind::Service
            | UnitKind::Test
            | UnitKind::IntegrationTest
            | UnitKind::Controller
                if !variants.is_empty() =>
            {
                linker.problem(
                    "model-unit-unexpected-variants",
                    format!("{path}.variants"),
                    "only a sealed source unit or strategy can declare variants",
                    "remove `variants` or choose `sealed`/`strategy`",
                );
            }
            _ => {}
        }
        match unit.kind {
            UnitKind::Strategy if unit.on.is_none() => linker.problem(
                "model-strategy-on-missing",
                format!("{path}.on"),
                "a strategy needs the Java type it examines",
                "set `on` to a Java type",
            ),
            UnitKind::Class
            | UnitKind::Interface
            | UnitKind::Service
            | UnitKind::Test
            | UnitKind::IntegrationTest
            | UnitKind::Sealed
                if unit.on.is_some() || unit.yields.is_some() =>
            {
                linker.problem(
                    "model-unit-unexpected-strategy-types",
                    path.clone(),
                    "only a strategy or controller can declare `on` or `yields`",
                    "remove those keys or choose `strategy`/`controller`",
                );
            }
            UnitKind::Controller => {}
            _ => {}
        }
        let has_endpoint_shape =
            unit.method.is_some() || unit.path.is_some() || unit.consumes.is_some();
        if unit.kind != UnitKind::Controller && has_endpoint_shape {
            linker.problem(
                "model-unit-unexpected-endpoint",
                path.clone(),
                "only a controller can declare HTTP endpoint fields",
                "remove `method`, `path`, and `consumes`, or set `kind = \"controller\"`",
            );
        }
        let endpoint = if unit.kind == UnitKind::Controller {
            let method = unit.method.unwrap_or(EndpointMethod::Get);
            let consumes = unit.consumes.unwrap_or(RequestFormat::Json);
            let endpoint_path = unit.path.clone().unwrap_or_else(|| format!("/{label}"));
            if !endpoint_path.starts_with('/') || endpoint_path.chars().any(char::is_whitespace) {
                linker.problem(
                    "model-controller-path-invalid",
                    format!("{path}.path"),
                    "an HTTP route must be an absolute path without whitespace",
                    "use a path such as `/v1/orders`",
                );
            }
            if unit.on.is_some() && !method.takes_body() {
                linker.problem(
                    "model-controller-body-method",
                    format!("{path}.on"),
                    "this HTTP method does not carry the declared request body",
                    "use post, put, or patch, or remove `on`",
                );
            }
            if consumes == RequestFormat::Form && unit.on.is_none() {
                linker.problem(
                    "model-controller-form-without-body",
                    format!("{path}.consumes"),
                    "form binding needs a request type",
                    "set `on` or use json",
                );
            }
            Some(HttpEndpoint {
                method,
                path: endpoint_path,
                accepts: unit.on.clone(),
                returns: unit.yields.clone(),
                consumes,
            })
        } else {
            None
        };
        // The derived layer, kept as the layer rather than as its default
        // spelling. The linker has no `[layout]` -- it is a captured fact that
        // reaches the model one pass later -- so a string decided here is a
        // string that cannot be renamed, which would put a sealed type in
        // `domain` on a project whose records live in `core`.
        let derived = match unit.kind {
            UnitKind::Service => Some(crate::Package::Service),
            UnitKind::Sealed | UnitKind::Strategy => Some(crate::Package::Domain),
            UnitKind::Controller => Some(crate::Package::Web),
            UnitKind::Class | UnitKind::Interface | UnitKind::Test | UnitKind::IntegrationTest => {
                Some(crate::Package::Base)
            }
        };
        // Named by the reader wins, and turns the derivation off: they said
        // where it goes, so a later rename of that layer must leave it alone.
        let layer = if unit.package.is_some() {
            None
        } else {
            derived
        };
        let relative_package = unit.package.unwrap_or_else(|| match derived {
            Some(crate::Package::Service) => "service".to_string(),
            Some(crate::Package::Domain) => "domain".to_string(),
            Some(crate::Package::Web) => "web".to_string(),
            _ => String::new(),
        });
        let java_package = if relative_package.is_empty() {
            base_package.to_string()
        } else {
            format!("{base_package}.{relative_package}")
        };
        linker.java_package(&java_package, &format!("{path}.package"));
        collision(
            linker,
            &mut java_types,
            &format!("{java_package}.{java_type}"),
            &path,
            "model-unit-java-type-collision",
            "Java source unit",
        );
        if let Some(id) = id {
            linked.insert(
                id.clone(),
                SourceUnit {
                    id,
                    label,
                    kind: unit.kind,
                    java_type,
                    java_package,
                    layer,
                    variants,
                    on: unit.on,
                    yields: unit.yields,
                    endpoint,
                },
            );
        }
    }
    linked
}
