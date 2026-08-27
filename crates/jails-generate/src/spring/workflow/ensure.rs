//! `g usecase --on-conflict <component>`: get-or-create, as one statement.
//!
//! `missing.md` M6 counted this as the first line of three of the six ported
//! projects -- `User.get_or_create_from_email(email)`, `User.upsert({...})`,
//! `Conversation.objects.create() if not conv_id else ...` -- against a
//! generator whose only create verb always inserted. On a `@unique` column the
//! second call was a constraint violation rather than a fetch.
//!
//! The statement is the one `g explain idempotency` already describes
//! verbatim: `insert ... on conflict (...) do nothing returning`. What is
//! deliberate here is that it is **not** the repository. A port with a
//! `save(T)` cannot express it, and select-then-insert reopens the window
//! where two callers both see nothing and both proceed -- which is the whole
//! reason the statement is one statement.
//!
//! So `--on-conflict` replaces `Storing{X}UseCase` with `Ensuring{X}UseCase`,
//! a `JdbcClient` adapter in `adapters` implementing the same port. That is
//! the shape `g transition` already uses for the same reason: an operation
//! whose atomicity lives in SQL is written where the SQL is.

use super::*;

/// The column `on conflict` names, with everything the renderers need about
/// it resolved once.
pub(super) struct Conflict {
    /// The Java component, as the command and the record spell it.
    pub(super) component: String,
    /// Its column, as the table spells it.
    pub(super) column: String,
    /// What `on conflict (...)` names, which is the index's expression and
    /// not always the column: an `@unique` email is indexed on
    /// `lower(email)`, and `on conflict (email)` finds no index to arbitrate
    /// against -- PostgreSQL refuses the whole statement. Derived through
    /// `sql::case_insensitive`, the same function the DDL uses, so the two
    /// cannot disagree about which shape the index took.
    pub(super) target: String,
    /// The `where` clause that reads the row somebody else already has, in
    /// whichever case-sensitivity the index has.
    pub(super) predicate: String,
    /// The expression that binds it, with the candidate record as receiver.
    pub(super) write: String,
}

/// Resolve `--on-conflict <component>`, or refuse and say what is wrong.
///
/// Two checks, and one deliberate non-check:
///
/// - **The component must exist on the target.** Otherwise the generated SQL
///   names a column the table does not have and fails at run time.
/// - **The command must carry it.** A get-or-create keyed on a value the
///   caller does not supply is a create with an extra clause: every call
///   invents a new key and nothing ever conflicts.
/// - **Whether the column is unique is not checked here, because jails cannot
///   see it.** A record read off disk carries no constraints -- `Person.java`
///   says `String email`, not `@unique` -- and the alternatives are worse:
///   re-reading the migration jails wrote is the guessing `build.rs` refuses
///   to do with a build file, and taking the caller's word for it is a fact
///   nothing verifies. The generated IT is what verifies it: without a unique
///   index the second call inserts a second row, and
///   `twoCallsWithTheSameKeyAreOneRow` fails against a real PostgreSQL saying
///   so. Same shape as `g auth`'s expiry validator -- the default is wrong in
///   a way nothing reports, so the test is the thing that keeps it right.
pub(super) fn conflict_column(
    name: &str,
    target: &str,
    target_fields: &[crate::generate::Field],
    target_columns: &[crate::sql::Column],
    command_fields: &[crate::generate::Field],
    component: &str,
) -> jails_support::Result<Conflict> {
    // **`on conflict ... do nothing returning` is PostgreSQL's**, and H2 has
    // no form of it: `Syntax error ... [*]on conflict`, measured against a
    // real H2 2.4.240. H2's `merge` is an upsert but has no `returning`, so
    // the one-round-trip atomic claim this whole kind is built on would become
    // a merge and then a select -- a different design, not a translation.
    //
    // Refused by name rather than emitted and left to fail at the first
    // request, which is the rule `require_jakarta_spring` follows for a type
    // that does not exist.
    if let Some(column) = target_columns.first()
        && column.dialect == jails_spec::spec::kind::Dialect::H2
    {
        return Err(format!(
            "usecase {name} uses `--on-conflict`, which generates PostgreSQL's `insert ... on \
             conflict ... do nothing returning`, and this project's driver is H2 -- which has \
             no form of it.\n       fix: drop `--on-conflict` and let the caller handle a \
             duplicate, or point the project at PostgreSQL (`jails add db`)."
        )
        .into());
    }
    if !target_fields
        .iter()
        .any(|candidate| candidate.name == component)
    {
        return Err(format!(
            "usecase {name} conflicts on `{component}`, but {target} has no component with that \
             name.\n       fix: name one of: {}.",
            target_fields
                .iter()
                .map(|candidate| candidate.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
        .into());
    }
    if !command_fields
        .iter()
        .any(|candidate| candidate.name == component)
    {
        return Err(format!(
            "usecase {name} conflicts on `{component}`, which its command does not carry. Every \
             call would invent a new key and nothing would ever conflict.\n       fix: add \
             `{component}:<type>` to `jails g usecase {name} ...`."
        )
        .into());
    }
    let Some(column) = target_columns
        .iter()
        .find(|candidate| candidate.component == component)
    else {
        return Err(format!(
            "usecase {name} cannot map `{component}` to a column.\n       fix: use a component \
             jails has a storage mapping for."
        )
        .into());
    };
    let insensitive = crate::sql::case_insensitive(column);
    Ok(Conflict {
        component: component.to_string(),
        column: column.name.clone(),
        target: if insensitive {
            format!("lower({})", column.name)
        } else {
            column.name.clone()
        },
        predicate: if insensitive {
            format!("lower({name_}) = lower(:{name_})", name_ = column.name)
        } else {
            format!("{name_} = :{name_}", name_ = column.name)
        },
        write: column
            .write
            .clone()
            .ok_or_else(|| {
                format!(
                    "usecase {name} cannot bind `{component}`.\n       fix: use a component jails \
                     has a storage mapping for."
                )
            })?
            .replace("value.", "candidate."),
    })
}

/// The adapter: build the candidate the same way `Storing…` does, then let one
/// statement decide whether it is the row.
pub(super) fn ensuring_usecase_java(
    slice: &Slice,
    name: &str,
    target: &str,
    defaults: &Defaults,
    target_columns: &[crate::sql::Column],
    conflict: &Conflict,
) -> String {
    let pkg: &str = &slice.owned(Layer::Adapters);
    let service: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let candidate_columns: Vec<crate::sql::Column> = target_columns
        .iter()
        .map(|column| {
            let mut column = column.clone();
            column.write = column
                .write
                .as_ref()
                .map(|write| write.replace("value.", "candidate."));
            column
        })
        .collect();
    // A database-assigned key is not in the insert, the same rule the
    // repository adapter follows: the column is `generated always as
    // identity`, so naming it is the caller working around the policy.
    let generated = crate::sql::generated_key(&candidate_columns).map(|column| column.name.clone());
    let inserted: Vec<&crate::sql::Column> = candidate_columns
        .iter()
        .filter(|column| Some(&column.name) != generated.as_ref())
        .collect();
    let columns = inserted
        .iter()
        .map(|column| column.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = inserted
        .iter()
        .map(|column| format!(":{}", column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let bindings = inserted
        .iter()
        .map(|column| {
            format!(
                "                .param(\"{}\", {})",
                column.name,
                column.write.as_deref().expect("mapped insert column")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let select = candidate_columns
        .iter()
        .map(|column| column.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let map_args = candidate_columns
        .iter()
        .map(|column| {
            format!(
                "                {}",
                column.read.as_deref().expect("mapped target column")
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let mut imports = crate::sql::imports(&candidate_columns)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .chain(
            defaults
                .imports
                .iter()
                .map(|import| format!("import {import};\n")),
        )
        .collect::<Vec<_>>();
    imports.sort();
    imports.dedup();
    crate::template::render(
        crate::template_here!("spring/ensuring_usecase_java.java"),
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
            ("preamble", defaults.preamble),
            (
                "args",
                &defaults
                    .expressions
                    .iter()
                    .map(|expression| format!("                {expression}"))
                    .collect::<Vec<_>>()
                    .join(",\n"),
            ),
            ("table", &crate::sql::table_name(target)),
            ("columns", &columns),
            ("placeholders", &placeholders),
            ("bindings", &bindings),
            ("select", &select),
            ("map_args", &map_args),
            ("conflict_column", &conflict.column),
            ("conflict_target", &conflict.target),
            ("conflict_predicate", &conflict.predicate),
            ("conflict_component", &conflict.component),
            ("conflict_write", &conflict.write),
        ],
    )
}

/// The proof: two calls with the same key are one row and the same row.
///
/// A real database, because that is the only place `on conflict` means
/// anything -- an in-memory fake of this port would be a second implementation
/// of the very decision under test.
pub(super) fn ensuring_usecase_it_java(
    slice: &Slice,
    name: &str,
    resolved: &Target,
    fields: &[crate::generate::Field],
    conflict: &Conflict,
) -> String {
    let project = slice.project();
    let pkg: &str = &slice.owned(Layer::Adapters);
    let service: &str = &slice.placed(Layer::Service);
    let domain: &str = &slice.owned(Layer::Domain);
    let base: String = slice.root_package();
    let target: &str = &resolved.name;
    let samples = fields
        .iter()
        .map(|field| crate::generate::sample_value(field, project, domain))
        .collect::<Option<Vec<_>>>();
    let disabled = samples.is_none();
    let args = samples.unwrap_or_default().join(",\n                ");
    let imports = java_literal_imports(fields, domain)
        .into_iter()
        .map(|import| format!("import {import};\n"))
        .collect::<String>();
    crate::template::render(
        crate::template_here!("spring/ensuring_usecase_it_java.java"),
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
            (
                "container_import",
                &crate::generate::import_of(pkg, &base, "TestcontainersConfig"),
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
                    "@Disabled(\"todo: supply command samples jails cannot fabricate\")\n"
                } else {
                    ""
                },
            ),
            ("name", name),
            ("target", target),
            ("args", &args),
            ("conflict_component", &conflict.component),
        ],
    )
}
