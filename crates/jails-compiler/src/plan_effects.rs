//! What a reviewed plan says is left to do, and what it noticed on the way.
//!
//! **Neither half describes a file.** Everything else the compiler produces
//! is a desired artifact -- bytes at a path, an edit to a reader document.
//! These two are the plan's remarks about itself: the services it declared
//! and did not start, the formatter it wants run over what it wrote, the
//! shape it emitted that is probably not what the reader meant.

use jails_contracts::RenderedTree;
use jails_model::AppModel;
use std::collections::BTreeMap;

/// Every effect this transition leaves for the caller to perform.
pub(crate) fn follow_up(
    next_model: &AppModel,
    generated: &RenderedTree,
    baseline: &RenderedTree,
) -> Vec<jails_contracts::EffectIntent> {
    // **What is left to do once the files are written.** A compose
    // service jails declares is not running because it was declared, and
    // the command that declared it is the one place a reader is looking.
    // It rides on the plan rather than on the frontend so `--pretend`
    // shows it, the exported bundle carries it, and apply cannot start
    // something the reviewed plan did not name.
    let compose_services = |tree: &RenderedTree| {
        tree.reader_facets
            .iter()
            .filter_map(|(id, facet)| match &facet.kind {
                jails_contracts::ReaderFacetKind::ComposeService { service, .. } => {
                    Some((id.clone(), (service.clone(), facet.path.clone())))
                }
                _ => None,
            })
            .collect::<BTreeMap<_, _>>()
    };
    let accepted = compose_services(baseline);
    // **Only what this transition introduces.** Every service the model
    // declares is in `generated` on every compile, so starting all of
    // them would make an unrelated `jails add csv` try to bring up a
    // database -- and fail on a machine with no engine, over a capability
    // that has nothing to do with one. The plan's effects are the plan's:
    // what changed, not what exists.
    let mut follow_up_effects: Vec<jails_contracts::EffectIntent> = compose_services(generated)
        .into_iter()
        .filter(|(id, service)| accepted.get(id) != Some(service))
        .map(|(_, service)| service)
        // **The document travels with the service.** The effect runs
        // after the commit publishes, so the live `compose.yaml` is no
        // longer proof of what this transition described -- naming the
        // file here is what lets apply run against the exact bytes it
        // wrote instead of whatever is on disk by then.
        .map(|(service, document)| jails_contracts::EffectIntent {
            id: format!("effect_compose_up_{service}"),
            kind: "compose-up".to_string(),
            arguments: BTreeMap::from([
                ("service".to_string(), service),
                ("document".to_string(), document.as_str().to_string()),
            ]),
        })
        .collect();
    // **Formatting is an effect, not a rendering.** The wrapping a
    // formatter chooses cannot be predicted from a template -- that is
    // what a formatter is for -- so a project that declares `format` has
    // to have one run over what was just written, or `jails check` fails
    // on jails' own output. It rides on the plan for the same reason
    // compose does: the reviewed transition says what is left to do.
    if next_model
        .capabilities
        .values()
        .any(|capability| capability.kind == "format")
        && generated
            .files
            .keys()
            .any(|path| path.as_str().ends_with(".java"))
    {
        follow_up_effects.push(jails_contracts::EffectIntent {
            id: "effect_format".to_string(),
            kind: "format".to_string(),
            arguments: BTreeMap::new(),
        });
    }
    follow_up_effects.sort_by(|left, right| left.id.cmp(&right.id));
    follow_up_effects.dedup_by(|left, right| left.id == right.id);
    follow_up_effects
}

/// What the compiler noticed and would not refuse over.
pub(crate) fn diagnostics(next_model: &AppModel) -> Vec<jails_contracts::CompilerDiagnostic> {
    // **A resource with nowhere to keep its rows is worth saying out
    // loud.** Without a declared storage the scaffold still emits its
    // record, its port and an in-memory adapter -- a resource that runs
    // and forgets everything on restart. That is a legitimate shape to
    // want, and it is also what a reader who simply has not run `jails
    // add db` yet gets, with nothing to tell the two apart.
    let mut diagnostics = Vec::new();
    if next_model.project.dialect != "postgresql" {
        for entity in next_model.entities.values() {
            if entity.active && entity.facets.contains(&jails_model::Facet::Repository) {
                diagnostics.push(jails_contracts::CompilerDiagnostic {
                    severity: jails_contracts::DiagnosticSeverity::Warning,
                    code: "storage-absent".to_string(),
                    semantic_id: Some(jails_model::StableId::as_str(&entity.id).to_string()),
                    message: format!(
                        "`{}` is stored in memory only: this model declares no SQL storage, so no `create table {}` was written",
                        entity.names.java_type, entity.names.sql_table
                    ),
                    fix: "run `jails add db` for PostgreSQL and Flyway migrations, or keep the in-memory adapter".to_string(),
                });
            }
        }
    }
    // **A query no index can serve is a sequential scan nobody asked for**,
    // and the compiler can see it without running anything: the filtered
    // columns are model facts and so is every index it emits. Reported per
    // query rather than per column, because one usable access path is enough
    // -- a filter set with a single indexed leading column among five is
    // served.
    //
    // **A warning and never a refusal**, for `free-text-closed-set`'s reason:
    // whether a table will grow enough to care is the reader's knowledge and
    // not jails'. Naming the shape and the command is the difference between
    // a tool with an opinion and a tool that guesses.
    if crate::emit_sql::has_database(next_model) {
        for query in crate::emit_operation::filtered_queries(next_model) {
            if query.columns.is_empty() {
                continue;
            }
            if query.columns.iter().any(|(entity, field)| {
                crate::emit_sql::leading_index_fields(entity).contains(&field.id)
            }) {
                continue;
            }
            let (target, _) = query.columns[0];
            let columns = query
                .columns
                .iter()
                .map(|(entity, field)| {
                    format!("{}.{}", entity.names.sql_table, field.names.sql_column)
                })
                .collect::<Vec<_>>()
                .join(", ");
            diagnostics.push(jails_contracts::CompilerDiagnostic {
                severity: jails_contracts::DiagnosticSeverity::Warning,
                code: "query-unindexed".to_string(),
                semantic_id: Some(
                    jails_model::StableId::as_str(&query.operation.id).to_string(),
                ),
                message: format!(
                    "`{}` filters on {columns}, and no index leads with a column it filters on: every call reads the whole table",
                    query.operation.names.java_type
                ),
                // The command first, and the attribute second: a table
                // already accepted refuses a bare `@index` as an index
                // change with no evolution policy, so the order the two are
                // offered in is the order they work in.
                fix: format!(
                    "run `jails resource index add {} '<columns>'` naming a filtered column first, or add `@index` to the field before the table is accepted",
                    target.names.java_type
                ),
            });
        }
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One stored entity, one query, and a filter column nothing indexes.
    const MODEL: &str = "jdl 1\n\
app Demo {\n pkg com.example.demo\n java 26\n platform plain\n build maven\n storage postgres\n}\n\
entity Task {\n \
 id: uuid @pk\n \
 owner: uuid\n \
 title: string\n \
 query ByOwner(owner) {\n }\n\
}\n\
use repo for Task\n";

    fn codes(source: &str) -> Vec<String> {
        let model = jails_model::parse_jdl(source).expect("the fixture parses");
        diagnostics(&model)
            .into_iter()
            .map(|diagnostic| diagnostic.code)
            .collect()
    }

    /// A query reads a table whose only index is its primary key, and
    /// nothing says so.
    #[test]
    fn a_query_no_index_can_serve_is_reported_with_the_command_that_fixes_it() {
        let model = jails_model::parse_jdl(MODEL).expect("the fixture parses");
        let found = diagnostics(&model);
        let reported = found
            .iter()
            .find(|diagnostic| diagnostic.code == "query-unindexed")
            .expect("an unindexed filter is reported");
        assert_eq!(
            reported.severity,
            jails_contracts::DiagnosticSeverity::Warning,
            "whether a table grows is the reader's knowledge, not jails'"
        );
        assert!(reported.message.contains("tasks.owner"), "{reported:?}");
        assert!(
            reported.fix.contains("jails resource index add Task"),
            "{reported:?}"
        );
    }

    /// Indexing the column closes it, which is the whole point of saying so.
    #[test]
    fn indexing_the_filtered_column_answers_the_diagnostic() {
        assert!(
            !codes(&MODEL.replace("owner: uuid\n", "owner: uuid @index\n"))
                .contains(&"query-unindexed".to_string())
        );
    }

    /// A composite index the query's column leads is enough; a composite it
    /// only appears inside is not, because PostgreSQL cannot use one for a
    /// predicate that names no leading column.
    #[test]
    fn only_a_leading_column_counts_as_served() {
        let leads = MODEL.replace(
            " title: string\n",
            " title: string\n index [owner, title]\n",
        );
        assert!(!codes(&leads).contains(&"query-unindexed".to_string()));
        let trails = MODEL.replace(
            " title: string\n",
            " title: string\n index [title, owner]\n",
        );
        assert!(codes(&trails).contains(&"query-unindexed".to_string()));
    }

    /// A query on the primary key is served by the key's own index, and a
    /// query with no predicate at all has nothing to serve.
    #[test]
    fn a_key_lookup_and_a_full_listing_are_both_quiet() {
        assert!(
            !codes(&MODEL.replace("ByOwner(owner)", "ById(id)"))
                .contains(&"query-unindexed".to_string())
        );
        assert!(
            !codes(&MODEL.replace("ByOwner(owner)", "All()"))
                .contains(&"query-unindexed".to_string())
        );
    }

    /// Without SQL storage there are no indexes to be missing, and
    /// `storage-absent` is already the honest thing to say about the shape.
    #[test]
    fn an_in_memory_resource_is_not_told_to_add_an_index() {
        let found = codes(&MODEL.replace("storage postgres", "storage none"));
        assert!(!found.contains(&"query-unindexed".to_string()));
        assert!(found.contains(&"storage-absent".to_string()));
    }
}
