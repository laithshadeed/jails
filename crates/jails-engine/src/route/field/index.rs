//! `resource index add`: an index on a table that already exists.
//!
//! Beside the field evolutions rather than among them, because it is not one.
//! What it shares with them is the shape -- a recorded declaration edited, one
//! forward migration, one entry in the operation list -- which is what
//! `evolve_existing` is, and `FieldEvolution` says so where it classifies this.

use super::*;

/// Add a composite or ordered index to a table that already exists.
///
/// `--index` and `@index` both exist at creation time and nothing existed
/// afterwards, which `missing.md` M9 measured against a real project whose
/// third migration is exactly this line. An index is the *easy* half of what
/// `resource field add` already does: `g field` has to argue about a data plan
/// for a populated table and an index has none to argue about.
///
/// The columns are validated against the table before anything is written --
/// a typo here fails at `flyway migrate` with "column does not exist", on
/// whichever machine runs it first -- and the index is recorded on the entity,
/// so a re-plan reproduces it rather than dropping it.
pub fn add_index(run: &Run, target: &str, columns: &str, package: Option<&str>) -> Result<Outcome> {
    let project = run.project();
    let store = observed(project)?;
    let (id, spec) = recorded_target(project, &store, target, package)?;
    let declared = jails_protocol::declaration::IndexSpec::parse(
        &request::as_field_names(columns, spec.fields()),
        spec.fields(),
    )?;
    if spec.indexes.contains(&declared) {
        return Err(format!(
            "`{}` already declares an index on `{}`.\n       fix: nothing to do -- \
             `jails resource status {target}` shows what it has.",
            id.name,
            declared.canonical()
        )
        .into());
    }
    let table = jails_generate::sql::table_name(id.name.as_str());
    let mut after = spec.clone();
    after.indexes.push(declared.clone());
    // The position in the entity's own list, so this index is named in the
    // same series a `create table` would have used. Two indexes on one table
    // cannot then be given one name by two commands.
    let position = after.indexes.len();
    let body = jails_generate::sql::declared_index(
        &table,
        position,
        &request::as_column_names(&declared, after.fields()),
    );
    let slug = format!("index_{table}_{position}");
    evolve_existing(
        run,
        &store,
        target,
        package,
        id,
        after,
        FieldEvolution::AddIndex(declared),
        DataEvolution::None,
        body,
        &slug,
        vec![target.to_string(), columns.to_string()],
        BTreeMap::new(),
    )
}
