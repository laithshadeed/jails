use super::*;

/// The artifact this field is being added to, as the store records it.
///
/// Recorded rather than read off disk, because the spec is what the next
/// render is computed from and a record's Java cannot say what its components
/// were *declared* as: `@pk`, `@unique` and `@index` change the DDL and
/// nothing about the type. Reading them back would produce a table missing
/// the key somebody believed they had asked for.
pub(super) fn recorded_target(
    project: &Project,
    store: &ObservedStore,
    target: &str,
    package: Option<&str>,
) -> Result<(IntentId, IntentSpec)> {
    let name = Name::parse(&jails_spec::spec::field::capitalize(target))?;
    let package = package
        .map(|package| Package::parse(&project.package_named("", Some(package))))
        .transpose()?;
    let mut found = store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.applied.iter())
        .filter_map(|row| match (&row.id, &row.version.spec) {
            (EntityId::Intent(id), EntitySpec::Intent(spec))
                if id.name == name
                    && package
                        .as_ref()
                        .is_none_or(|package| id.package == *package)
                    && matches!(
                        spec.arguments,
                        jails_protocol::declaration::IntentArguments::Fields(_)
                    ) =>
            {
                Some((id.clone(), spec.clone()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let packages = found
        .iter()
        .map(|(id, _)| id.package.as_str())
        .collect::<BTreeSet<_>>();
    if packages.len() > 1 {
        return Err(format!(
            "`{name}` is recorded in more than one package: {}.\n       fix: pass the relative `--package` that selects one identity.",
            packages.into_iter().collect::<Vec<_>>().join(", ")
        )
        .into());
    }
    // A logical Java type can be owned by both a narrow record intent and a
    // scaffold. Evolve the widest projection as the primary so repositories,
    // DTOs and HTTP surfaces move with the record; companion intents are
    // updated in the same request below.
    found.sort_by_key(|(id, _)| {
        (
            id.recipe != jails_protocol::entity::Recipe::Scaffold,
            id.clone(),
        )
    });
    Ok(found.into_iter().next().ok_or_else(|| {
        format!(
            "no `{name}` is recorded in this project.\n       fix: `jails g scaffold {name} \
             ...` or `jails g record {name} ...` first. Adding a component to something the \
             store never recorded would mean guessing what its other components were declared \
             as, and a declaration is not readable from the Java it produced."
        )
    })?)
}
