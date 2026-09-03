//! Which component kinds have an emitter, and the boundary registry held
//! against what the compiler emits.

/// Whether this component kind has an emitter behind it.
///
/// A kind that links, plans, applies and reports success while producing no
/// file and no diagnostic is a silent no-op on a declaration the author
/// wrote -- worse than a refusal, because there is nothing to notice.
///
/// The match is exhaustive on purpose: JDL v1 §20.2 asks for a test that
/// fails "when a registered role has no emitter", and the strongest version of
/// that test is a compile error. Adding a kind stops the build here until
/// somebody decides which arm it belongs in.
pub(crate) const fn component_kind_is_emitted(kind: jails_model::ComponentKind) -> bool {
    use jails_model::ComponentKind as Kind;
    match kind {
        Kind::Class
        | Kind::Interface
        | Kind::Service
        | Kind::Controller
        | Kind::Sealed
        | Kind::Strategy
        | Kind::Test
        | Kind::IntegrationTest => true,
        // `cases` emits no Java, but it is not silent: its reader-owned
        // brief is captured as an exact plan input, so changing the file
        // after review refuses the apply. A backend need not write a file.
        Kind::Cases => true,
        Kind::Auth
        | Kind::Cli
        | Kind::Client
        | Kind::Command
        | Kind::Handler
        | Kind::Fetcher
        | Kind::Idempotency
        | Kind::Job
        | Kind::Presence
        | Kind::Socket
        | Kind::HttpSink
        | Kind::HttpWorkflow
        | Kind::DurableJob
        | Kind::Webhook => true,
    }
}

/// The boundary registry against what the compiler emits: JDL v1 §20.2's
/// exhaustiveness tests. A registered role with no emitter, or an emitter
/// naming a role no row carries, fails here before any model does.
#[cfg(test)]
mod boundary_tests {
    use jails_contracts::{BuildSystem, WorkspaceSnapshot};
    use jails_model::boundary::{self, Owner, Scope};
    use jails_model::{ComponentKind, Facet, StableId};
    use std::collections::{BTreeMap, BTreeSet};

    /// One entity with every facet an emitter answers to, one enum with wire
    /// values, on the storage the JDBC adapter and its proof need.
    const EVERY_FACET: &str = "jdl 1\n\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  java 26\n  platform spring\n  build maven\n  storage postgres\n}\n\ncap fake\ncap json\n\nenum Priority @id(ent_priority) {\n  LOW = \"low\"\n  HIGH = \"high\"\n}\n\nentity Task @id(ent_task) {\n  use scaffold, factory, seed, dto\n  use search(fields: [title])\n  id: uuid @id(fld_task_id) @pk\n  title: string @id(fld_task_title) @notBlank\n}\n";

    /// Every artifact the maximal model emits, by id, with its semantic ids.
    fn emitted() -> BTreeMap<String, BTreeSet<String>> {
        let mut model = jails_model::parse_jdl(EVERY_FACET).unwrap();
        // The events port has an emitter and no projection that selects it
        // (JDL v1 §11.1 lists none), so the facet is put on the entity here:
        // the question is whether the row and the emitter agree, not whether
        // the language can reach them.
        model
            .entities
            .values_mut()
            .find(|entity| entity.id.as_str() == "ent_task")
            .unwrap()
            .facets
            .insert(Facet::Events);
        let mut snapshot = WorkspaceSnapshot::detached(model);
        snapshot.project.spring_boot = Some("4.1.0".to_string());
        snapshot.project.build_system = BuildSystem::Maven;
        crate::Compiler::compile(
            &snapshot,
            &snapshot.model.model,
            &jails_model::Evolution::none(),
        )
        .unwrap()
        .generated
        .files
        .values()
        .map(|file| {
            (
                file.provenance.artifact_id.clone(),
                file.provenance.semantic_ids.clone(),
            )
        })
        .collect()
    }

    const OWNERS: [&str; 2] = ["ent_task", "ent_priority"];

    /// A registered entity boundary has an emitter: the id its row spells is
    /// in the tree of the model that declares every facet.
    #[test]
    fn every_registered_entity_boundary_is_emitted() {
        let emitted = emitted();
        let unemitted: Vec<_> = boundary::rows_for(Owner::Entity)
            .filter(|row| {
                !OWNERS.iter().any(|owner| {
                    row.artifact_id(owner, Some("cap_db"))
                        .is_some_and(|id| emitted.contains_key(&id))
                })
            })
            .map(|row| format!("  {} -> {}", row.path, row.role))
            .collect();
        assert!(
            unemitted.is_empty(),
            "registered boundaries no emitter produces:\n{}",
            unemitted.join("\n")
        );
    }

    /// Every artifact emitted for an entity is a registered boundary, so
    /// `eject` can name it by path and no emitter concatenates a role of its
    /// own.
    #[test]
    fn every_emitted_entity_artifact_is_a_registered_boundary() {
        let registered: BTreeSet<String> = boundary::rows_for(Owner::Entity)
            .flat_map(|row| {
                OWNERS
                    .iter()
                    .filter_map(move |owner| row.artifact_id(owner, Some("cap_db")))
            })
            .collect();
        let unregistered: Vec<_> = emitted()
            .into_iter()
            .filter(|(_, semantic)| OWNERS.iter().any(|owner| semantic.contains(*owner)))
            .filter(|(id, _)| !registered.contains(id))
            .map(|(id, _)| format!("  {id}"))
            .collect();
        assert!(
            unregistered.is_empty(),
            "entity artifacts no boundary row names:\n{}",
            unregistered.join("\n")
        );
    }

    /// A component kind's registered roles are exactly its recipe's rows, and
    /// `implementation` names one of them that is ejectable main source.
    #[test]
    fn every_component_recipe_role_is_registered_and_every_registered_role_is_a_row() {
        for kind in ComponentKind::ALL {
            let registered: BTreeMap<&str, &str> = boundary::rows_for(Owner::Component(kind))
                .map(|row| (row.path, row.role))
                .collect();
            let Some(recipe) = crate::emit_component::recipe_for(kind) else {
                assert!(
                    registered.is_empty(),
                    "{} has boundary rows and no recipe",
                    kind.label()
                );
                continue;
            };
            let rows: BTreeSet<&str> = recipe.files.iter().map(|file| file.role).collect();
            let paths: BTreeSet<&str> = registered
                .iter()
                .filter(|(path, _)| **path != "implementation")
                .map(|(_, role)| *role)
                .collect();
            assert_eq!(
                paths,
                rows,
                "{}: registered roles and recipe rows differ",
                kind.label()
            );
            let implementation = registered
                .get("implementation")
                .unwrap_or_else(|| panic!("{} registers no `implementation`", kind.label()));
            let row = recipe
                .files
                .iter()
                .find(|file| file.role == *implementation)
                .unwrap_or_else(|| panic!("{}: `implementation` names no row", kind.label()));
            assert!(
                row.ejectable && row.source_set == crate::recipe::SourceSet::Main,
                "{}: `implementation` must name ejectable main source",
                kind.label()
            );
        }
    }

    /// A storage-scoped row is keyed on the storage capability, and the
    /// owner-scoped rows are not.
    #[test]
    fn storage_scoped_rows_are_the_jdbc_adapter_and_its_proofs() {
        let stored: Vec<_> = boundary::rows_for(Owner::Entity)
            .filter(|row| row.scope == Scope::Storage)
            .map(|row| row.path)
            .collect();
        assert_eq!(
            stored,
            ["repo.postgres", "repo.postgres.it", "search.postgres"]
        );
    }
}
