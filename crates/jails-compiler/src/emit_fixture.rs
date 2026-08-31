//! The rows a generated integration test has to insert before its own.
//!
//! **A sampled foreign key names a row that does not exist.** `Message.userId`
//! sampled as `1` is a `User` nobody inserted, and PostgreSQL rejects the whole
//! statement -- so a test that stores the child has to store the parent first
//! and carry the key the database assigned it.
//!
//! One owner, because every adapter integration test needs the same rows: the
//! repository's round trip, a command's insert, a query's filter, a
//! transition's update. Written per emitter they would disagree about which
//! parents exist, and the disagreement only shows up against a real database.

use crate::emit_java::domain_import;
use jails_model::{AppModel, Entity, FieldId, Package};
use std::collections::{BTreeMap, BTreeSet};

/// Fixtures for one entity's parents, and how to reach the keys they assigned.
pub(crate) struct Parents {
    /// `@Autowired` repository fields, one per parent.
    pub(crate) autowired: String,
    /// Statements that store each parent, in declaration order.
    pub(crate) fixtures: String,
    /// The child's foreign-key components, by the parent key they read.
    pub(crate) overrides: BTreeMap<FieldId, String>,
}

/// Store every parent this entity references, or `None` when one cannot be
/// built.
///
/// One level deep. A parent that is itself a child would need ordering and
/// cycle detection, and a test that cannot be built correctly must not be
/// guessed at -- so the caller emits nothing rather than something wrong.
pub(crate) fn parents(
    model: &AppModel,
    entity: &Entity,
    imports: &mut BTreeSet<String>,
) -> Option<Parents> {
    let mut out = Parents {
        autowired: String::new(),
        fixtures: String::new(),
        overrides: BTreeMap::new(),
    };
    for relation in model
        .relations
        .values()
        .filter(|relation| relation.child == entity.id)
    {
        let parent = model.entities.get(&relation.parent)?;
        if model
            .relations
            .values()
            .any(|other| other.child == parent.id)
        {
            return None;
        }
        let parent_row = crate::emit_companion_test::constructor_call(model, parent, imports)?;
        let parent_type = &parent.names.java_type;
        let variable = format!("saved{parent_type}");
        let field = lower_first(parent_type);
        imports.insert(format!(
            "{}.{parent_type}Repository",
            model.project.package_for(Package::Repository)
        ));
        imports.insert(domain_import(model, parent));
        out.autowired.push_str(&format!(
            "\n    @Autowired\n    private {parent_type}Repository {field}Repository;\n"
        ));
        out.fixtures.push_str(&format!(
            "        {parent_type} {variable} = {field}Repository.save({parent_row});\n"
        ));
        for mapping in &relation.mappings {
            let remote = parent.field(&mapping.remote)?;
            out.overrides.insert(
                mapping.local.clone(),
                format!("{variable}.{}()", remote.names.java_member),
            );
        }
    }
    Some(out)
}

/// The imports and annotations every adapter integration test carries.
pub(crate) fn integration_imports(model: &AppModel) -> BTreeSet<String> {
    BTreeSet::from([
        format!(
            "{}.TestcontainersConfig",
            model.project.package_for(Package::Base)
        ),
        "org.junit.jupiter.api.Test".to_string(),
        "org.springframework.beans.factory.annotation.Autowired".to_string(),
        "org.springframework.boot.test.context.SpringBootTest".to_string(),
        "org.springframework.context.annotation.Import".to_string(),
        "org.springframework.transaction.annotation.Transactional".to_string(),
        "static org.assertj.core.api.Assertions.assertThat".to_string(),
    ])
}

/// `@Transactional` so each test rolls its rows back and the order they run in
/// cannot matter.
pub(crate) const ANNOTATIONS: &str =
    "@Import(TestcontainersConfig.class)\n@SpringBootTest\n@Transactional\n";

fn lower_first(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_lowercase().to_string() + characters.as_str()
    })
}
