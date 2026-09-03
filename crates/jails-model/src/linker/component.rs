//! Linking component declarations: names, references, routes and units.
//!
//! Takes the parsed `source::Component` map and produces validated
//! `Component`s plus the `SourceUnit`s their emitters will be placed by,
//! reporting every problem rather than the first.
//!
//! **Which fields a kind may carry is a table, not a pile of `if`s.**
//! `registry.rs` states, per `ComponentKind`, whether `on`, `yields`, `route`
//! and `source` are forbidden, optional or required, and the match over the
//! enum is exhaustive — so a new kind does not compile until somebody has
//! answered those questions for it. That is the difference between a
//! declaration whose constraints were decided and one whose constraints are
//! whatever the linker happened not to check.
//!
//! Names are projections (`upper_camel_case`, `lower_camel_case`) and
//! collisions are refused here rather than downstream, because two components
//! projecting to one Java type is a build failure in generated code the reader
//! did not write.

mod registry;

use super::{Linker, collision};
use crate::id::{ComponentId, ComponentVariantId, EntityId, OperationId};
use crate::model::{Entity, TypeRef};
use crate::naming::{lower_camel_case, upper_camel_case};
use crate::operation::{
    BindingSource, OperationRoute, ParameterBinding, ParameterConstraints, Value,
};
use crate::source;
use crate::{
    Component, ComponentKind, ComponentParameter, ComponentReference, ComponentVariant,
    EndpointMethod, HttpEndpoint, LengthRange, Operation, RequestFormat, SourceUnit, UnitId,
    UnitKind,
};
use registry::{rule, validate_presence, validate_route};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn link(
    declarations: BTreeMap<String, source::Component>,
    entities: &BTreeMap<EntityId, Entity>,
    operations: &BTreeMap<OperationId, Operation>,
    base_package: &str,
    units: &mut BTreeMap<UnitId, SourceUnit>,
    routes: &mut BTreeMap<String, String>,
    linker: &mut Linker,
) -> BTreeMap<ComponentId, Component> {
    let mut ids = BTreeMap::new();
    for (label, component) in &declarations {
        let path = format!("$.components.{label}");
        linker.label(label, &path);
        linker.register_id(&component.id, &format!("{path}.id"));
        if let Some(id) = linker.stable_id::<ComponentId>(&component.id, &format!("{path}.id")) {
            ids.insert(label.clone(), id);
        }
    }
    let component_symbols = declarations
        .iter()
        .filter_map(|(label, component)| {
            ids.get(label)
                .map(|id| (id.clone(), (component.kind, component.name.clone())))
        })
        .collect::<BTreeMap<_, _>>();

    let entity_labels = entities
        .iter()
        .map(|(id, entity)| (entity.label.clone(), id.clone()))
        .collect::<BTreeMap<_, _>>();
    let operation_labels = operations
        .iter()
        .map(|(id, operation)| (operation.label.clone(), id.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut linked = BTreeMap::new();
    let mut java_types = units
        .values()
        .map(|unit| {
            (
                format!("{}.{}", unit.java_package, unit.java_type),
                "legacy source unit".to_string(),
            )
        })
        .collect::<BTreeMap<_, _>>();

    for (label, declaration) in declarations {
        let path = format!("$.components.{label}");
        let Some(id) = ids.get(&label).cloned() else {
            continue;
        };
        let rule = rule(declaration.kind);
        linker.java_type(&declaration.name, &format!("{path}.name"));
        if rule
            .forbidden_suffix
            .is_some_and(|suffix| declaration.name.ends_with(suffix))
        {
            linker.problem(
                "model-component-redundant-suffix",
                format!("{path}.name"),
                format!(
                    "component {} `{}` repeats its generated `{}` suffix",
                    declaration.kind.label(),
                    declaration.name,
                    rule.forbidden_suffix.expect("checked above")
                ),
                "declare the name without its generated suffix",
            );
        }
        let parameters = link_parameters(declaration.parameters, &path, linker);
        validate_presence(
            declaration.on.is_some(),
            rule.on,
            "on",
            declaration.kind,
            &path,
            linker,
        );
        validate_presence(
            declaration.yields.is_some(),
            rule.yields,
            "yields",
            declaration.kind,
            &path,
            linker,
        );
        validate_presence(
            declaration.route.is_some(),
            rule.route,
            "route",
            declaration.kind,
            &path,
            linker,
        );
        validate_presence(
            declaration.source.is_some(),
            rule.source,
            "source",
            declaration.kind,
            &path,
            linker,
        );
        if !rule.bindings && !declaration.bindings.is_empty() {
            linker.problem(
                "model-component-bindings-forbidden",
                format!("{path}.bindings"),
                format!(
                    "component {} does not accept request binding overrides",
                    declaration.kind.label()
                ),
                "remove the bind statements",
            );
        }
        validate_route(
            declaration.kind,
            declaration.route.as_ref(),
            declaration.on.is_some(),
            &path,
            routes,
            linker,
        );
        validate_bindings(&declaration.bindings, &parameters, &path, linker);
        let variants = link_variants(declaration.kind, declaration.variants, &path, linker);
        if let Some(source_path) = &declaration.source
            && (source_path.starts_with('/')
                || source_path.split('/').any(|segment| segment == ".."))
        {
            linker.problem(
                "model-component-source-path",
                format!("{path}.source"),
                "a component source must be a project-relative path without `..`",
                "use a path rooted in the project",
            );
        }
        let on = declaration.on.as_deref().and_then(|reference| {
            link_reference(
                reference,
                &format!("{path}.on"),
                &entity_labels,
                &operation_labels,
                &ids,
                linker,
            )
        });
        let yields = declaration.yields.as_deref().and_then(|reference| {
            link_reference(
                reference,
                &format!("{path}.yields"),
                &entity_labels,
                &operation_labels,
                &ids,
                linker,
            )
        });
        let route = declaration.route.map(link_route);
        let bindings = declaration.bindings.into_iter().map(link_binding).collect();
        let component = Component {
            id: id.clone(),
            label: label.clone(),
            name: declaration.name.clone(),
            kind: declaration.kind,
            parameters,
            on,
            yields,
            route,
            bindings,
            variants,
            source: declaration.source,
        };
        if let Some(unit) = compatibility_unit(
            &component,
            entities,
            operations,
            &component_symbols,
            base_package,
            linker,
        ) {
            collision(
                linker,
                &mut java_types,
                &format!("{}.{}", unit.java_package, unit.java_type),
                &path,
                "model-unit-java-type-collision",
                "Java source unit",
            );
            units.insert(unit.id.clone(), unit);
        }
        linked.insert(id, component);
    }
    linked
}

fn link_parameters(
    parameters: Vec<source::ComponentParameter>,
    path: &str,
    linker: &mut Linker,
) -> Vec<ComponentParameter> {
    let mut names = BTreeSet::new();
    parameters
        .into_iter()
        .filter_map(|parameter| {
            let parameter_path = format!("{path}.parameters.{}", parameter.name);
            if !names.insert(parameter.name.clone()) {
                linker.problem(
                    "model-component-parameter-collision",
                    &parameter_path,
                    format!("component parameter `{}` is repeated", parameter.name),
                    "give every component parameter a unique name",
                );
                return None;
            }
            let ty = match TypeRef::parse(&parameter.type_name) {
                Ok(ty) => ty,
                Err(message) => {
                    linker.problem(
                        "model-component-parameter-type",
                        &parameter_path,
                        message,
                        "use a builtin type or declared Java type",
                    );
                    return None;
                }
            };
            Some(ComponentParameter {
                name: parameter.name,
                ty,
                required: parameter.required,
                constraints: link_constraints(parameter.constraints),
            })
        })
        .collect()
}

fn link_variants(
    kind: ComponentKind,
    variants: Vec<source::ComponentVariant>,
    path: &str,
    linker: &mut Linker,
) -> Vec<ComponentVariant> {
    let supports = matches!(kind, ComponentKind::Sealed | ComponentKind::Strategy);
    if supports && variants.is_empty() {
        linker.problem(
            "model-component-variants-empty",
            format!("{path}.variants"),
            format!("component {} needs at least one variant", kind.label()),
            "declare one or more variants",
        );
    } else if !supports && !variants.is_empty() {
        linker.problem(
            "model-component-variants-forbidden",
            format!("{path}.variants"),
            format!("component {} does not accept variants", kind.label()),
            "remove the variant declarations",
        );
    }
    let mut names = BTreeSet::new();
    variants
        .into_iter()
        .filter_map(|variant| {
            let variant_path = format!("{path}.variants.{}", variant.name);
            linker.register_id(&variant.id, &format!("{variant_path}.id"));
            let id = linker
                .stable_id::<ComponentVariantId>(&variant.id, &format!("{variant_path}.id"))?;
            if !names.insert(variant.name.clone()) {
                linker.problem(
                    "model-component-variant-collision",
                    format!("{path}.variants"),
                    format!("component variant `{}` is repeated", variant.name),
                    "give every variant a unique name",
                );
                return None;
            }
            linker.java_type(&variant.name, &format!("{path}.variants.{}", variant.name));
            let parameters = link_parameters(variant.parameters, path, linker);
            if kind == ComponentKind::Strategy && !parameters.is_empty() {
                linker.problem(
                    "model-strategy-variant-payload",
                    format!("{path}.variants.{}", variant.name),
                    "strategy variants cannot declare payload parameters",
                    "remove the parameters or use a sealed component",
                );
            }
            Some(ComponentVariant {
                id,
                name: variant.name,
                parameters,
            })
        })
        .collect()
}

fn link_reference(
    reference: &str,
    path: &str,
    entities: &BTreeMap<String, EntityId>,
    operations: &BTreeMap<String, OperationId>,
    components: &BTreeMap<String, ComponentId>,
    linker: &mut Linker,
) -> Option<ComponentReference> {
    if let Some(id) = entities.get(reference) {
        return Some(ComponentReference::Entity(id.clone()));
    }
    if let Some(id) = operations.get(reference) {
        return Some(ComponentReference::Operation(id.clone()));
    }
    if let Some(id) = components.get(reference) {
        return Some(ComponentReference::Component(id.clone()));
    }
    linker.problem(
        "model-component-reference",
        path,
        format!("`{reference}` does not name an entity, operation, or component"),
        "reference a declaration that exists",
    );
    None
}

fn validate_bindings(
    bindings: &[source::ParameterBinding],
    parameters: &[ComponentParameter],
    path: &str,
    linker: &mut Linker,
) {
    let names = parameters
        .iter()
        .map(|parameter| parameter.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut bound = BTreeSet::new();
    for binding in bindings {
        if !names.contains(binding.parameter.as_str()) {
            linker.problem(
                "model-component-binding-parameter",
                format!("{path}.bindings"),
                format!(
                    "binding references undeclared parameter `{}`",
                    binding.parameter
                ),
                "bind a declared component parameter",
            );
        }
        if !bound.insert(binding.parameter.as_str()) {
            linker.problem(
                "model-component-binding-collision",
                format!("{path}.bindings"),
                format!("parameter `{}` is bound more than once", binding.parameter),
                "keep one binding source per parameter",
            );
        }
    }
}

fn compatibility_unit(
    component: &Component,
    entities: &BTreeMap<EntityId, Entity>,
    operations: &BTreeMap<OperationId, Operation>,
    component_symbols: &BTreeMap<ComponentId, (ComponentKind, String)>,
    base_package: &str,
    linker: &mut Linker,
) -> Option<SourceUnit> {
    let kind = match component.kind {
        ComponentKind::Class => UnitKind::Class,
        ComponentKind::Interface => UnitKind::Interface,
        ComponentKind::Service => UnitKind::Service,
        ComponentKind::Controller => UnitKind::Controller,
        ComponentKind::Sealed => UnitKind::Sealed,
        ComponentKind::Strategy => UnitKind::Strategy,
        ComponentKind::Test => UnitKind::Test,
        ComponentKind::IntegrationTest => UnitKind::IntegrationTest,
        _ => return None,
    };
    let id = UnitId::parse(component.id.to_string()).unwrap_or_else(|message| {
        linker.problem(
            "model-component-unit-id",
            format!("$.components.{}.id", component.label),
            message,
            "use a stable component id",
        );
        unreachable!("component and unit IDs share the same syntax")
    });
    let java_type = match kind {
        UnitKind::Service => format!("{}Service", component.name),
        UnitKind::Controller => format!("{}Controller", component.name),
        UnitKind::Test => format!("{}Test", component.name),
        UnitKind::IntegrationTest => format!("{}IT", component.name),
        _ => component.name.clone(),
    };
    // The layer, not its default spelling: see `SourceUnit::layer`. A
    // component-derived unit is always derived -- a `component` declaration
    // carries no package of its own -- so this is never `None` here.
    let layer = Some(match kind {
        UnitKind::Service => crate::Package::Service,
        UnitKind::Sealed | UnitKind::Strategy => crate::Package::Domain,
        UnitKind::Controller => crate::Package::Web,
        _ => crate::Package::Base,
    });
    let relative_package = match kind {
        UnitKind::Service => "service",
        UnitKind::Sealed | UnitKind::Strategy => "domain",
        UnitKind::Controller => "web",
        _ => "",
    };
    let java_package = if relative_package.is_empty() {
        base_package.to_string()
    } else {
        format!("{base_package}.{relative_package}")
    };
    let on = component.on.as_ref().and_then(|reference| {
        reference_java_type(reference, entities, operations, component_symbols)
    });
    let yields = component.yields.as_ref().and_then(|reference| {
        reference_java_type(reference, entities, operations, component_symbols)
    });
    let endpoint = (kind == UnitKind::Controller).then(|| {
        let route = component.route.as_ref();
        HttpEndpoint {
            method: route.map_or(EndpointMethod::Get, |route| route.method),
            path: route.map_or_else(
                || format!("/{}", lower_camel_case(&component.name)),
                |route| route.path.clone(),
            ),
            accepts: on.clone(),
            returns: yields.clone(),
            consumes: route
                .and_then(|route| route.consumes)
                .unwrap_or(RequestFormat::Json),
        }
    });
    Some(SourceUnit {
        id,
        label: component.label.clone(),
        kind,
        java_type,
        java_package,
        layer,
        variants: component
            .variants
            .iter()
            .map(|variant| upper_camel_case(&variant.name))
            .collect(),
        on,
        yields,
        endpoint,
    })
}

fn reference_java_type(
    reference: &ComponentReference,
    entities: &BTreeMap<EntityId, Entity>,
    operations: &BTreeMap<OperationId, Operation>,
    components: &BTreeMap<ComponentId, (ComponentKind, String)>,
) -> Option<String> {
    match reference {
        ComponentReference::Entity(id) => entities
            .get(id)
            .map(|entity| entity.names.java_type.clone()),
        ComponentReference::Operation(id) => operations
            .get(id)
            .map(|operation| operation.names.java_type.clone()),
        ComponentReference::Component(id) => components
            .get(id)
            .map(|(kind, name)| kind.primary_type(name)),
    }
}

fn link_constraints(source: source::ParameterConstraints) -> ParameterConstraints {
    ParameterConstraints {
        default: source.default.map(link_value),
        non_blank: source.non_blank,
        length: if source.min_length.is_some() || source.max_length.is_some() {
            Some(LengthRange {
                min: source.min_length,
                max: source.max_length,
            })
        } else {
            None
        },
        positive: source.positive,
        nonnegative: source.nonnegative,
    }
}

fn link_value(value: source::Value) -> Value {
    match value {
        source::Value::String(value) => Value::String(value),
        source::Value::Integer(value) => Value::Integer(value),
        source::Value::Decimal(value) => Value::Decimal(value),
        source::Value::Boolean(value) => Value::Boolean(value),
        source::Value::EnumConstant(value) => Value::EnumConstant(value),
        source::Value::Function(call) => Value::Function {
            name: call.name,
            arguments: call.arguments.into_iter().map(link_value).collect(),
        },
    }
}

fn link_route(route: source::OperationRoute) -> OperationRoute {
    OperationRoute {
        method: route.method,
        path: route.path,
        consumes: route.consumes,
    }
}

fn link_binding(binding: source::ParameterBinding) -> ParameterBinding {
    ParameterBinding {
        parameter: binding.parameter,
        source: match binding.source {
            source::BindingSource::Path => BindingSource::Path,
            source::BindingSource::Query => BindingSource::Query,
            source::BindingSource::Header => BindingSource::Header,
            source::BindingSource::Claim => BindingSource::Claim,
            source::BindingSource::Form => BindingSource::Form,
        },
        wire_name: binding.wire_name,
    }
}
