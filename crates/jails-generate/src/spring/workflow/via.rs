//! `g usecase --via`: resolving a foreign key on the way in.
//!
//! `POST /customer_api/messages` carries the customer's `email` and the row
//! needs a `user_id`. `g query --via` reads across that reference; nothing
//! wrote across it, so the only expressible shape was an endpoint that trusts
//! the caller for a key that is not theirs to choose.
//!
//! One statement, for the reason the get-or-create half is one: a
//! read-then-insert leaves the window where the parent is deleted between the
//! two, and lets a caller name a key they do not own. The insert *selects* the
//! key from the parent's table, so a row with no parent is not written and the
//! result is empty.

use super::*;

/// What resolving the key needs: the reference itself, and the component of
/// the parent the caller sends instead of it.
pub(super) struct Resolution<'a> {
    pub(super) join: crate::spring::Join,
    /// The command component that names a component of the *parent*.
    pub(super) lookup: &'a crate::generate::Field,
    /// That component's column on the parent's table.
    pub(super) lookup_column: String,
}

/// Which command field looks the parent up, and how the two tables meet.
///
/// Exactly one field may name a parent component, and it may not be the
/// parent's key: a caller who has the key does not need this recipe, and two
/// candidates is an ambiguity jails must not resolve by picking.
pub(super) fn resolve<'a>(
    slice: &Slice,
    name: &str,
    target: &str,
    fields: &'a [crate::generate::Field],
    parent: &str,
) -> jails_support::Result<Resolution<'a>> {
    let target_fields = Target::read(slice, "usecase", name, target)?.fields;
    let join = crate::spring::resolve_join(slice, "usecase", name, target, &target_fields, parent)?;
    // A component of the parent that the target does not also have. The
    // exclusion is what makes the parent's *key* unreachable here, and that is
    // correct rather than an oversight: a record read off disk carries no
    // constraints, so `key_column` falls back to the component called `id` --
    // and `usecase` already refuses a target with no `id`. A caller holding
    // the key needs no lookup, and the field they would have sent is the
    // reference itself, which `usecase` refuses by name as a database-assigned
    // key.
    let candidates: Vec<&crate::generate::Field> = fields
        .iter()
        .filter(|field| {
            join.parent_fields
                .iter()
                .any(|parent_field| parent_field.name == field.name)
                && !target_fields
                    .iter()
                    .any(|target_field| target_field.name == field.name)
        })
        .collect();
    let lookup = match candidates.as_slice() {
        [only] => *only,
        [] => {
            return Err(format!(
                "usecase {name} resolves its key through {parent}, but none of its fields names \
                 a component of {parent}.\n       fix: accept the one the caller actually sends \
                 -- {parent} declares {}.",
                join.parent_fields
                    .iter()
                    .filter(|field| field.name != join.parent_component)
                    .map(|field| format!("`{}`", field.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .into());
        }
        many => {
            return Err(format!(
                "usecase {name} names {} components of {parent}: {}.\n       fix: one identifies \
                 the parent; move the rest onto {target} or drop them. Two lookups would be two \
                 rows this insert could choose between.",
                many.len(),
                many.iter()
                    .map(|field| format!("`{}`", field.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .into());
        }
    };
    let domain: &str = &slice.owned(Layer::Domain);
    let parent_columns =
        crate::sql::columns(&join.parent_fields, slice.project(), domain, "parent");
    let lookup_column = parent_columns
        .iter()
        .find(|column| column.component == lookup.name)
        .map(|column| column.name.clone())
        .ok_or_else(|| {
            format!(
                "usecase {name} cannot map {parent}.{} to a column",
                lookup.name
            )
        })?;
    Ok(Resolution {
        join,
        lookup,
        lookup_column,
    })
}

/// The adapter: one insert whose key comes from the parent's own row.
pub(super) fn resolving_usecase_java(
    slice: &Slice,
    name: &str,
    target: &str,
    defaults: &Defaults,
    resolution: (&Resolution<'_>, &[crate::sql::Column]),
) -> String {
    let (resolution, target_columns) = resolution;
    let join = &resolution.join;
    let pkg: &str = &slice.owned(Layer::Adapters);
    let service: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    // A database-assigned key is not in the insert, the same rule every other
    // write path here follows: the column is `generated always as identity`,
    // so naming it is the caller working around the policy.
    let generated = crate::sql::generated_key(target_columns).map(|column| column.name.clone());
    let inserted: Vec<(usize, &crate::sql::Column)> = target_columns
        .iter()
        .enumerate()
        .filter(|(_, column)| Some(&column.name) != generated.as_ref())
        .collect();
    let columns = inserted
        .iter()
        .map(|(_, column)| column.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    // The join column is *selected* from the parent; every other value is a
    // parameter. That is the whole of the resolution, and it is why there is
    // no window: the row cannot be written unless the select matched.
    let selected = inserted
        .iter()
        .map(|(_, column)| {
            if column.name == join.child_column {
                format!("{}.{}", join.parent_table, join.parent_column)
            } else {
                format!(":{}", column.name)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    // `sql::Column::write` renders a storage expression against a receiver --
    // `value.senderType().name()` for an enum -- so the substitution is the
    // receiver, not the whole expression. The same one-replacement trick the
    // pins and the transition's selector use, and for the same reason:
    // rendering it a second time is how two renderings of one column drift.
    let bindings = inserted
        .iter()
        .filter(|(_, column)| column.name != join.child_column)
        .map(|(index, column)| {
            let write = column.write.as_deref().expect("mapped insert column");
            let expression = &defaults.expressions[*index];
            format!(
                "                .param(\"{}\", {})",
                column.name,
                write.replacen(&format!("value.{}()", column.component), expression, 1)
            )
        })
        .chain(std::iter::once(format!(
            "                .param(\"{}\", command.{}())",
            resolution.lookup_column, resolution.lookup.name
        )))
        .collect::<Vec<_>>()
        .join("\n");
    let select = target_columns
        .iter()
        .map(|column| column.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let map_args = target_columns
        .iter()
        .map(|column| {
            format!(
                "                {}",
                column.read.as_deref().expect("mapped target column")
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let mut imports = crate::sql::imports(target_columns)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .chain(
            defaults
                .imports
                .iter()
                .map(|import| format!("import {import};\n")),
        )
        .collect::<Vec<_>>();
    imports.retain(|import| import != "import java.util.Optional;\n");
    imports.sort();
    imports.dedup();
    crate::template::render(
        crate::template_here!("spring/resolving_usecase_java.java"),
        &[
            ("pkg", pkg),
            (
                "target_import",
                &crate::generate::import_of(pkg, domain, target),
            ),
            (
                "command_import",
                &crate::generate::import_of(pkg, service, &format!("{name}Command")),
            ),
            (
                "port_import",
                &crate::generate::import_of(pkg, service, &format!("{name}UseCase")),
            ),
            ("imports", &imports.concat()),
            ("name", name),
            ("target", target),
            ("parent", &join.parent),
            ("parent_table", &join.parent_table),
            ("child_column", &join.child_column),
            ("lookup", &resolution.lookup.name),
            ("lookup_column", &resolution.lookup_column),
            ("preamble", defaults.preamble),
            ("table", &crate::sql::table_name(target)),
            ("columns", &columns),
            ("selected", &selected),
            ("bindings", &bindings),
            ("select", &select),
            ("map_args", &map_args),
        ],
    )
}

/// The proof: the empty result is what stops a row being written against a key
/// the caller invented, and nothing else observes it.
pub(super) fn resolving_usecase_it_java(
    slice: &Slice,
    name: &str,
    fields: &[crate::generate::Field],
    resolution: &Resolution<'_>,
) -> String {
    let join = &resolution.join;
    let project = slice.project();
    let pkg: &str = &slice.owned(Layer::Adapters);
    let service: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let app: &str = &slice.owned(Layer::App);
    let parent: &str = &join.parent;
    let command_samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, project, domain))
        .collect::<Option<Vec<_>>>();
    let parent_samples = join
        .parent_fields
        .iter()
        .map(|field| crate::generate::sample_value(field, project, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = command_samples.is_none() || parent_samples.is_none();
    let command_args = command_samples
        .unwrap_or_default()
        .join(",\n                ");
    let parent_args = parent_samples
        .unwrap_or_default()
        .join(",\n                ");
    let imports = java_literal_imports(&join.parent_fields, domain)
        .into_iter()
        .chain(java_literal_imports(fields, domain))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    crate::template::render(
        crate::template_here!("spring/resolving_usecase_it_java.java"),
        &[
            ("pkg", pkg),
            (
                "parent_import",
                &crate::generate::import_of(pkg, domain, parent),
            ),
            (
                "command_import",
                &crate::generate::import_of(pkg, service, &format!("{name}Command")),
            ),
            (
                "port_import",
                &crate::generate::import_of(pkg, service, &format!("{name}UseCase")),
            ),
            (
                "parent_repository_import",
                &crate::generate::import_of(pkg, app, &format!("{parent}Repository")),
            ),
            ("imports", &imports),
            (
                "disabled_import",
                if disabled {
                    "import org.junit.jupiter.api.Disabled;\n"
                } else {
                    ""
                },
            ),
            (
                "annotation",
                if disabled {
                    "@Disabled(\"todo: supply samples jails cannot fabricate\")\n"
                } else {
                    ""
                },
            ),
            ("name", name),
            ("parent", parent),
            ("command_args", &command_args),
            ("parent_args", &parent_args),
            ("child_component", &join.child_component),
            ("parent_component", &join.parent_component),
        ],
    )
}
