//! What a reviewed plan says is left to do, and what it noticed on the way.
//!
//! **Split out by the secret both halves share: neither describes a file.**
//! Everything else the compiler produces is a desired artifact -- bytes at a
//! path, an edit to a reader document. These two are the plan's remarks about
//! itself: the services it declared and did not start, the formatter it wants
//! run over what it wrote, the shape it emitted that is probably not what the
//! reader meant.
//!
//! They ride on the *plan* rather than on a frontend so `--pretend` shows
//! them, the exported bundle carries them, and apply cannot start something
//! the reviewed plan did not name.

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
    diagnostics
}
