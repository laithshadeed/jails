//! What holds the two repository adapters to one behaviour: the contract, and
//! the two tests that call it.
//!
//! **The adapters themselves are rows** ([`super::storage`]); these three are
//! functions because each reaches across nodes for a sample -- the record
//! arguments a proof constructs, and the ancestor rows a foreign key demands
//! before the child can be stored -- which is the same reason `emit_http`
//! stays one.
//!
//! **Two adapters, and exactly one of them is a bean.** The in-memory one
//! exists so a generated project starts before anybody has run `add db`; the
//! JDBC one takes over when the starter is there. Annotating both would make
//! two beans qualify for one injection point, which is the ambiguity
//! `jails beans` reports and a scaffold that compiles and cannot run.

use super::*;
use jails_model::{Package, boundary};

const CONTRACT: crate::Template = crate::template!("spring/repository_contract_java.java");
const FAKE_TEST: crate::Template = crate::template!("spring/fake_repository_test_java.java");

/// The integration test for the JDBC adapter.
///
/// **The only tier that answers the question this adapter exists for.** A
/// `JdbcClient` statement is a string until something runs it: a column list
/// that drifted, a type PostgreSQL will not accept, a `returning` clause that
/// names a column the insert does not write -- every one of them compiles, and
/// every one of them fails on the first real call. The unit tiers cannot see
/// any of it.
///
/// `None` when the entity has a component jails cannot sample: a guessed value
/// would not compile, and a test that constructs nothing proves nothing. That
/// is the same rule the record's own companion follows.
pub(super) fn lower_db_repository_it(
    model: &AppModel,
    capability_id: &str,
    entity: &Entity,
) -> Result<Option<Unit>, Diagnostic> {
    let package = crate::emit_java::entity_package(model, entity, Package::AdaptersJdbc);
    let type_name = format!("Jdbc{}RepositoryIT", entity.names.java_type);
    let record = &entity.names.java_type;
    let mut imports = BTreeSet::from([
        domain_import(model, entity),
        format!(
            "{}.{record}Repository",
            crate::emit_java::entity_package(model, entity, Package::Repository)
        ),
        "org.junit.jupiter.api.Test".to_string(),
        "org.springframework.beans.factory.annotation.Autowired".to_string(),
        "org.springframework.boot.test.context.SpringBootTest".to_string(),
        "org.springframework.transaction.annotation.Transactional".to_string(),
    ]);
    let repository_package = crate::emit_java::entity_package(model, entity, Package::Repository);
    if repository_package != package {
        imports.insert(format!("{repository_package}.{record}RepositoryContract"));
    }
    // **Its ancestors first.** A `Member` stored without its `Workspace` fails
    // on the foreign key before anything about the adapter is proved, so the
    // same fixture builder the operation proofs use writes the rows this one
    // references -- deepest first, shared, and bound to the same key the child
    // carries.
    let Some(fixtures) =
        crate::emit_operation::proof::ancestor_fixtures(model, entity, &[], record, &mut imports)?
    else {
        return Ok(None);
    };
    let Some(arguments) = crate::emit_operation::proof::record_arguments(
        model,
        entity,
        &fixtures.substitutions,
        &mut imports,
    ) else {
        return Ok(None);
    };
    let (setup, autowired) = (fixtures.setup, fixtures.autowired);
    // The assertions are the contract's, not this test's: two copies of them
    // is how the fake and the adapter drift.
    let body = format!(
        "@SpringBootTest\n@Transactional\nclass {type_name} {{\n\n    @Autowired\n    private {record}Repository repository;\n\n{autowired}    @Test\n    void satisfiesThe{record}RepositoryContractAgainstTheRealDatabase() {{\n{setup}        {record}RepositoryContract.savesReadsAndDeletes(\n                repository, new {record}({arguments}));\n    }}\n\n    // Reader-owned cases belong below this stable boundary.\n}}"
    );
    let artifact_id = boundary::REPOSITORY_POSTGRES_IT.stored_by(capability_id, entity.id.as_str());
    let mut unit = JavaUnit::new(&package, &imports, &body);
    crate::emit_capability::imported_test_container(model, &mut unit);
    let rendered = unit.render(&artifact_id);
    let path = crate::refuse::project_path(format!(
        "{}/{}/{type_name}.java",
        crate::emit_companion_test::JAVA_TEST_ROOT,
        package.replace('.', "/")
    ))?;
    Ok(Some(Unit {
        path,
        file: RenderedFile {
            kind: FileKind::JavaTest,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: Some(capability_id.to_string()),
                ejectable: true,
                semantic_ids: BTreeSet::from([
                    capability_id.to_string(),
                    entity.id.as_str().to_string(),
                ]),
                compiler_pass: "java-facets".to_string(),
            },
        },
    }))
}

/// One set of assertions both repository adapters are held to.
///
/// Without it two implementations of one port can disagree indefinitely
/// about what `save` returns, whether `deleteById` on a missing row is
/// `false` or an error, and whether `findAll` contains what was just stored.
///
/// Static methods rather than an abstract base class, because the two callers
/// are not the same shape: the proof is a Spring test whose ancestor rows have
/// to be written through their own repositories first, and the fake's test is
/// an ordinary object with a constructor. Inheritance would drag the fixture
/// machinery into a test that needs none of it, so what is shared is the
/// assertions -- the part that must not differ.
///
/// `None` on an entity jails cannot sample, which is what both callers do:
/// a contract class no test calls is dead code a reader has to decode.
pub(super) fn lower_repository_contract(
    model: &AppModel,
    entity: &Entity,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Option<Unit>, Diagnostic> {
    if crate::emit_operation::proof::record_arguments(
        model,
        entity,
        &std::collections::BTreeMap::new(),
        &mut BTreeSet::new(),
    )
    .is_none()
    {
        return Ok(None);
    }
    let primary_key = primary_key(entity)?;
    let package = crate::emit_java::entity_package(model, entity, Package::Repository);
    let record = &entity.names.java_type;
    let type_name = format!("{record}RepositoryContract");
    let imports = BTreeSet::from([
        domain_import(model, entity),
        "static org.assertj.core.api.Assertions.assertThat".to_string(),
    ]);
    let mut unit = JavaUnit::from_source(
        &CONTRACT
            .resolve(templates)?
            .replace("{{pkg}}", &package)
            .replace("{{record}}", record)
            .replace("{{key}}", &primary_key.names.java_member),
    );
    for name in &imports {
        unit.import(name);
    }
    let artifact_id = boundary::REPOSITORY_CONTRACT.owned_by(entity.id.as_str());
    Ok(Some(test_unit(
        &package,
        &type_name,
        artifact_id.clone(),
        unit.render(&artifact_id),
        Provenance {
            artifact_id,
            ejection_id: None,
            // Both generated tests call it, so ownership cannot move without
            // leaving the compiler emitting calls into a type it no longer
            // controls. Same rule as a port.
            ejectable: false,
            semantic_ids: BTreeSet::from([entity.id.as_str().to_string()]),
            compiler_pass: "java-facets".to_string(),
        },
    )?))
}

/// The in-memory adapter, held to the contract above.
pub(super) fn lower_fake_repository_test(
    model: &AppModel,
    capability_id: &str,
    entity: &Entity,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Option<Unit>, Diagnostic> {
    let package = crate::emit_java::entity_package(model, entity, Package::AdaptersMemory);
    let record = &entity.names.java_type;
    let type_name = format!("InMemory{record}RepositoryTest");
    let repository_package = crate::emit_java::entity_package(model, entity, Package::Repository);
    let mut imports = BTreeSet::from([
        domain_import(model, entity),
        "org.junit.jupiter.api.Test".to_string(),
    ]);
    if repository_package != package {
        imports.insert(format!("{repository_package}.{record}RepositoryContract"));
    }
    // No ancestor fixtures: a `LinkedHashMap` enforces no foreign key, so the
    // rows a database would demand first are exactly the ones this adapter
    // does not need.
    let Some(arguments) = crate::emit_operation::proof::record_arguments(
        model,
        entity,
        &std::collections::BTreeMap::new(),
        &mut imports,
    ) else {
        return Ok(None);
    };
    let mut unit = JavaUnit::from_source(
        &FAKE_TEST
            .resolve(templates)?
            .replace("{{pkg}}", &package)
            .replace("{{record}}", record)
            .replace("{{arguments}}", &arguments),
    );
    for name in &imports {
        unit.import(name);
    }
    // **Entity-scoped, like the adapter it tests, and for the same reason.**
    // The in-memory adapter's owner switches from `cap_scaffold_default` to
    // `cap_fake` the moment `add fake` is run, at an unchanged path -- so a
    // capability-scoped id makes the same file a *new* artifact with no merge
    // base, and reconciliation refuses it as reader-owned. The `db` proof
    // beside this one is capability-scoped because `db` never hands over.
    let artifact_id = boundary::REPOSITORY_FAKE_TEST.owned_by(entity.id.as_str());
    Ok(Some(test_unit(
        &package,
        &type_name,
        artifact_id.clone(),
        unit.render(&artifact_id),
        Provenance {
            artifact_id: artifact_id.clone(),
            ejection_id: Some(capability_id.to_string()),
            ejectable: true,
            semantic_ids: BTreeSet::from([
                capability_id.to_string(),
                entity.id.as_str().to_string(),
            ]),
            compiler_pass: "capability-fake".to_string(),
        },
    )?))
}

fn test_unit(
    package: &str,
    type_name: &str,
    _artifact_id: String,
    rendered: String,
    provenance: Provenance,
) -> Result<Unit, Diagnostic> {
    let path = crate::refuse::project_path(format!(
        "{}/{}/{type_name}.java",
        crate::emit_companion_test::JAVA_TEST_ROOT,
        package.replace('.', "/")
    ))?;
    Ok(Unit {
        path,
        file: RenderedFile {
            kind: FileKind::JavaTest,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance,
        },
    })
}

#[cfg(test)]
mod tests {
    use jails_contracts::{BuildSystem, WorkspaceSnapshot};
    use std::collections::BTreeMap;

    fn slice(source: &str) -> BTreeMap<String, (bool, String)> {
        let model = jails_model::parse_jdl(source).unwrap();
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
                (
                    file.provenance.ejectable,
                    String::from_utf8(file.bytes.clone()).unwrap(),
                ),
            )
        })
        .collect()
    }

    const BOTH: &str = "jdl 1\n\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  java 26\n  platform spring\n  build maven\n  storage postgres\n}\n\ncap fake\n\nentity Task @id(ent_task) {\n  use repo\n  id: uuid @id(fld_task_id) @pk\n  title: string @id(fld_task_title)\n}\n";

    /// One contract, both adapters.
    ///
    /// Assertions living only in the JDBC proof let the fake drift from what
    /// it stands in for while every test using it stays green.
    #[test]
    fn both_repository_adapters_are_held_to_one_contract() {
        let slice = slice(BOTH);
        let (_, contract) = &slice["art_ent_task_repository_contract"];
        assert!(
            contract.contains("public static void savesReadsAndDeletes("),
            "{contract}"
        );
        for artifact in [
            "art_ent_task_repository_memory_test",
            "art_cap_db_ent_task_repository_it",
        ] {
            let (_, source) = &slice[artifact];
            assert!(
                source.contains("TaskRepositoryContract.savesReadsAndDeletes("),
                "`{artifact}` does not call the contract:\n{source}"
            );
            // The assertions must not also be inline, or the two can drift
            // again with both tests still passing.
            assert!(
                !source.contains("assertThat(repository.findAll())"),
                "`{artifact}` keeps its own copy of the assertions:\n{source}"
            );
        }
    }

    /// The contract is public, because its callers are in other packages.
    ///
    /// A package-private one compiles in isolation and fails at the first test
    /// that uses it -- which the goldens cannot see, since they compare bytes
    /// and never run `javac`. This is that failure, pinned.
    #[test]
    fn the_contract_is_reachable_from_the_packages_that_call_it() {
        let slice = slice(BOTH);
        let (ejectable, contract) = &slice["art_ent_task_repository_contract"];
        assert!(contract.contains("public final class TaskRepositoryContract"));
        // Both callers are elsewhere, so both must import it.
        for artifact in [
            "art_ent_task_repository_memory_test",
            "art_cap_db_ent_task_repository_it",
        ] {
            let (_, source) = &slice[artifact];
            assert!(
                source.contains("import com.example.notes.repository.TaskRepositoryContract;"),
                "`{artifact}` calls the contract without importing it:\n{source}"
            );
        }
        // Managed ABI: both generated tests call it.
        assert!(!ejectable);
    }

    /// **Declaring `fake` must not re-identify a file it does not move.**
    ///
    /// The in-memory adapter is emitted before any capability asks for it --
    /// something has to satisfy the port -- owned by `cap_scaffold_default`,
    /// and `add fake` hands it to `cap_fake` at an unchanged path. A
    /// capability-scoped artifact id therefore makes the same bytes a *new*
    /// artifact with no merge base, and reconciliation refuses it as
    /// reader-owned: `add fake` fails on a project that was fine a moment
    /// earlier. The adapter and its test are entity-scoped for exactly this
    /// reason; the `db` proof is capability-scoped, which is safe only
    /// because `db` never hands over.
    #[test]
    fn declaring_fake_does_not_re_identify_the_in_memory_adapter_or_its_test() {
        let without = slice(
            "jdl 1\n\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  java 26\n  platform spring\n  build maven\n  storage none\n}\n\nentity Task @id(ent_task) {\n  use repo\n  id: uuid @id(fld_task_id) @pk\n  title: string @id(fld_task_title)\n}\n",
        );
        let with = slice(
            "jdl 1\n\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  java 26\n  platform spring\n  build maven\n  storage none\n}\n\ncap fake\n\nentity Task @id(ent_task) {\n  use repo\n  id: uuid @id(fld_task_id) @pk\n  title: string @id(fld_task_title)\n}\n",
        );
        for artifact in [
            "art_ent_task_repository_memory",
            "art_ent_task_repository_memory_test",
            "art_ent_task_repository_contract",
        ] {
            assert!(
                without.contains_key(artifact) && with.contains_key(artifact),
                "`{artifact}` is not emitted under both owners:\nwithout: {:?}\nwith: {:?}",
                without.keys().collect::<Vec<_>>(),
                with.keys().collect::<Vec<_>>()
            );
        }
    }

    /// An entity jails cannot sample gets no contract, because it gets no
    /// caller -- the same refusal both tests already make.
    #[test]
    fn an_unsampleable_entity_gets_no_contract_class() {
        // `storage none`, because a project type in a *stored* entity refuses
        // one layer earlier -- at the SQL projection, for want of a codec --
        // and never reaches the question this test asks.
        let slice = slice(
            "jdl 1\n\napp Notes @id(project_notes) {\n  pkg com.example.notes\n  java 26\n  platform spring\n  build maven\n  storage none\n}\n\ncap fake\n\nentity Task @id(ent_task) {\n  use repo\n  id: uuid @id(fld_task_id) @pk\n  owner: com.example.other.Owner @id(fld_task_owner)\n}\n",
        );
        assert!(!slice.contains_key("art_ent_task_repository_contract"));
        assert!(!slice.contains_key("art_ent_task_repository_memory_test"));
        assert!(!slice.contains_key("art_cap_db_ent_task_repository_it"));
    }

    /// **`Map<long, Task>` is not a type.** An integral key is the one case
    /// where the Java spelling of a component differs by *position*: `long` as
    /// a parameter, `Long` as a type argument. Every scenario the goldens
    /// carried keyed on `uuid` or `string`, which box to themselves, so the
    /// in-memory adapter shipped a file that does not compile for the whole
    /// time `id:long@pk` has been the default shape for an assigned key.
    #[test]
    fn an_integral_key_boxes_where_it_is_a_type_argument() {
        for (token, primitive, boxed) in [("long", "long", "Long"), ("int", "int", "Integer")] {
            let slice = slice(&format!(
                "jdl 1\n\napp Notes @id(project_notes) {{\n  pkg com.example.notes\n  java 26\n  platform spring\n  build maven\n  storage postgres\n}}\n\ncap fake\n\nentity Task @id(ent_task2) {{\n  use repo\n  id: {token} @id(fld_task2_id) @pk\n  note: string @id(fld_task2_note)\n}}\n"
            ));
            let (_, memory) = slice
                .get("art_ent_task2_repository_memory")
                .expect("a repo facet emits the in-memory adapter");
            assert!(
                memory.contains(&format!("Map<{boxed}, Task> rows")),
                "the map's key argument must be boxed:\n{memory}"
            );
            // And the method the port declares stays primitive, or the
            // override does not match the interface it claims to implement.
            assert!(
                memory.contains(&format!("findById({primitive} id)")),
                "the parameter must stay primitive:\n{memory}"
            );
        }
    }
}
