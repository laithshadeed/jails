//! The boundary registry: every implementation boundary the compiler emits
//! for an entity or a component, named once.
//!
//! JDL v1 §16.4 says an ejection names its boundary by a readable path --
//! `Task.repo.fake`, `Audit.implementation` -- "defined by a boundary
//! registry rather than string concatenation in the parser", and §20.2 says
//! emitters do not concatenate an artifact's name themselves. This is that
//! registry, and both halves read it: the linker resolves a path to the
//! stable artifact id an ejection stores, and the emitters name their outputs
//! from the same row, so the id an ejection resolves to is the id the
//! compiler emits. An exhaustiveness test in `jails-compiler` holds the two
//! together -- a row with no emitter, or an emitter naming a role no row
//! carries, fails the build.
//!
//! **A role is the suffix of an artifact id.** `art_<owner>_<role>` is what
//! the merge is keyed on, so a row here is a fact about every project jails
//! has generated: renaming a role re-identifies a file at an unchanged path,
//! which the executor correctly refuses as reader-owned. Add rows; do not
//! move them.

use crate::component::ComponentKind;

/// Whose boundaries a row describes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Owner {
    Entity,
    Component(ComponentKind),
}

/// Whose stable id an artifact is keyed on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scope {
    /// `art_<owner id>_<role>`: the owner's own.
    Owner,
    /// `art_<capability id>_<owner id>_<role>`: the primary storage's, because
    /// the JDBC adapter and its proof come and go with `storage postgres`.
    Storage,
}

/// One implementation boundary a readable path names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Boundary {
    pub owner: Owner,
    /// The path after the owner's name: `repo.fake`.
    pub path: &'static str,
    /// The artifact id's suffix, and a recipe row's `role`.
    pub role: &'static str,
    pub scope: Scope,
}

impl Boundary {
    /// The artifact id this boundary emits as, for one owner; `storage` is
    /// the primary storage capability's id, which a [`Scope::Storage`] row
    /// needs and the others ignore.
    pub fn artifact_id(&self, owner_id: &str, storage: Option<&str>) -> Option<String> {
        match self.scope {
            Scope::Owner => Some(format!("art_{owner_id}_{}", self.role)),
            Scope::Storage => {
                storage.map(|storage| format!("art_{storage}_{owner_id}_{}", self.role))
            }
        }
    }

    /// The artifact id of an owner-scoped boundary.
    ///
    /// The one spelling of `art_<owner>_<role>` outside the recipe loop, for
    /// the emitters that are still functions.
    pub fn owned_by(&self, owner_id: &str) -> String {
        debug_assert!(
            self.scope == Scope::Owner,
            "`{}` is storage-scoped",
            self.path
        );
        format!("art_{owner_id}_{}", self.role)
    }

    /// The artifact id of a storage-scoped boundary.
    pub fn stored_by(&self, storage_id: &str, owner_id: &str) -> String {
        debug_assert!(
            self.scope == Scope::Storage,
            "`{}` is owner-scoped",
            self.path
        );
        format!("art_{storage_id}_{owner_id}_{}", self.role)
    }
}

const fn entity(path: &'static str, role: &'static str) -> Boundary {
    Boundary {
        owner: Owner::Entity,
        path,
        role,
        scope: Scope::Owner,
    }
}

const fn stored(path: &'static str, role: &'static str) -> Boundary {
    Boundary {
        owner: Owner::Entity,
        path,
        role,
        scope: Scope::Storage,
    }
}

const fn component(kind: ComponentKind, path: &'static str, role: &'static str) -> Boundary {
    Boundary {
        owner: Owner::Component(kind),
        path,
        role,
        scope: Scope::Owner,
    }
}

// The entity's own facets, one file each.
pub const RECORD: Boundary = entity("record", "record");
pub const ENUM: Boundary = entity("enum", "enum");
pub const ENUM_CONVERTER: Boundary = entity("enum.converter", "enum-converter");
pub const REPOSITORY: Boundary = entity("repo", "repository");
pub const SERVICE: Boundary = entity("service", "service");
pub const EVENTS: Boundary = entity("events", "events");
pub const SEARCH: Boundary = entity("search", "search");
pub const FACTORY: Boundary = entity("factory", "factory");
pub const TEST: Boundary = entity("test", "test");
// The repository port's implementations and what proves them.
pub const REPOSITORY_FAKE: Boundary = entity("repo.fake", "repository_memory");
pub const REPOSITORY_FAKE_TEST: Boundary = entity("repo.fake.test", "repository_memory_test");
pub const REPOSITORY_CONTRACT: Boundary = entity("repo.contract", "repository_contract");
pub const REPOSITORY_POSTGRES: Boundary = stored("repo.postgres", "repository");
pub const REPOSITORY_POSTGRES_IT: Boundary = stored("repo.postgres.it", "repository_it");
pub const SEARCH_POSTGRES: Boundary = stored("search.postgres", "search");
// The HTTP surface.
pub const HTTP: Boundary = entity("http", "http");
pub const HTTP_API: Boundary = entity("http.api", "http_controller");
pub const HTTP_API_TEST: Boundary = entity("http.api.test", "http_controller_test");
pub const HTTP_REQUESTS: Boundary = entity("http.requests", "http_requests");
// The DTOs and the seed.
pub const DTO_REQUEST: Boundary = entity("dto.request", "dto_request");
pub const DTO_RESPONSE: Boundary = entity("dto.response", "dto_response");
pub const DTO_TEST: Boundary = entity("dto.test", "dto_test");
pub const SEED_DATA: Boundary = entity("seed.data", "seed_data");
pub const SEEDER: Boundary = entity("seed", "seeder");
pub const SEEDER_TEST: Boundary = entity("seed.test", "seeder_test");

/// Every boundary, entity rows first and then each component kind's.
///
/// A component kind's rows are the roles of its recipe, and
/// `implementation` names the one main-source file `eject` most often
/// wants -- the adapter behind a port, the bean behind an interface.
pub const BOUNDARIES: &[Boundary] = &[
    RECORD,
    ENUM,
    ENUM_CONVERTER,
    REPOSITORY,
    SERVICE,
    EVENTS,
    SEARCH,
    FACTORY,
    TEST,
    REPOSITORY_FAKE,
    REPOSITORY_FAKE_TEST,
    REPOSITORY_CONTRACT,
    REPOSITORY_POSTGRES,
    REPOSITORY_POSTGRES_IT,
    SEARCH_POSTGRES,
    HTTP,
    HTTP_API,
    HTTP_API_TEST,
    HTTP_REQUESTS,
    DTO_REQUEST,
    DTO_RESPONSE,
    DTO_TEST,
    SEED_DATA,
    SEEDER,
    SEEDER_TEST,
    component(ComponentKind::Fetcher, "port", "port"),
    component(ComponentKind::Fetcher, "adapter", "adapter"),
    component(ComponentKind::Fetcher, "implementation", "adapter"),
    component(ComponentKind::Fetcher, "test", "test"),
    component(ComponentKind::Auth, "config", "config"),
    component(ComponentKind::Auth, "tokens", "tokens"),
    component(ComponentKind::Auth, "implementation", "tokens"),
    component(ComponentKind::Auth, "test", "test"),
    component(ComponentKind::Client, "interface", "interface"),
    component(ComponentKind::Client, "config", "config"),
    component(ComponentKind::Client, "implementation", "config"),
    component(ComponentKind::Client, "test", "test"),
    component(ComponentKind::Job, "job", "job"),
    component(ComponentKind::Job, "implementation", "job"),
    component(ComponentKind::Job, "test", "test"),
    component(ComponentKind::Handler, "handler", "handler"),
    component(ComponentKind::Handler, "implementation", "handler"),
    component(ComponentKind::Handler, "test", "test"),
    component(ComponentKind::Socket, "handler", "handler"),
    component(ComponentKind::Socket, "config", "config"),
    component(ComponentKind::Socket, "implementation", "handler"),
    component(ComponentKind::Socket, "test", "test"),
    component(ComponentKind::Webhook, "verifier", "verifier"),
    component(ComponentKind::Webhook, "controller", "controller"),
    component(ComponentKind::Webhook, "implementation", "verifier"),
    component(ComponentKind::Webhook, "test", "test"),
    component(ComponentKind::Command, "command", "command"),
    component(ComponentKind::Command, "implementation", "command"),
    component(ComponentKind::Command, "test", "test"),
    component(ComponentKind::Cli, "cli", "cli"),
    component(ComponentKind::Cli, "implementation", "cli"),
    component(ComponentKind::Cli, "test", "test"),
    component(ComponentKind::Presence, "port", "port"),
    component(ComponentKind::Presence, "store", "store"),
    component(ComponentKind::Presence, "implementation", "store"),
    component(ComponentKind::Presence, "it", "it"),
    component(ComponentKind::Idempotency, "record", "record"),
    component(ComponentKind::Idempotency, "port", "port"),
    component(ComponentKind::Idempotency, "store", "store"),
    component(ComponentKind::Idempotency, "guard", "guard"),
    component(ComponentKind::Idempotency, "implementation", "guard"),
    component(ComponentKind::Idempotency, "test", "test"),
    component(ComponentKind::HttpWorkflow, "workflow", "workflow"),
    component(ComponentKind::HttpWorkflow, "controller", "controller"),
    component(ComponentKind::HttpWorkflow, "implementation", "workflow"),
    component(ComponentKind::HttpWorkflow, "test", "test"),
];

/// The rows one owner kind carries.
pub fn rows_for(owner: Owner) -> impl Iterator<Item = &'static Boundary> {
    BOUNDARIES.iter().filter(move |row| row.owner == owner)
}

/// What a readable path's first segment names.
pub struct Resolved {
    pub owner: Owner,
    pub id: String,
}

/// Why a path did not resolve, in the words of the diagnostic.
#[derive(Debug)]
pub struct Unresolved {
    pub message: String,
    pub fix: String,
}

/// Resolve a readable boundary path to the artifact id it names.
///
/// `owner_of` answers what the first segment names -- an entity by its Java
/// type, a component by its name -- and `storage` is the primary storage
/// capability's id when the model declares one. Everything else is decided
/// here, so the linker and `jails model eject` cannot resolve a path two
/// ways.
pub fn resolve(
    path: &str,
    owner_of: impl Fn(&str) -> Result<Option<Resolved>, String>,
    storage: Option<&str>,
) -> Result<String, Unresolved> {
    let Some((name, rest)) = path.split_once('.') else {
        return Err(Unresolved {
            message: format!("`{path}` is not a boundary path"),
            fix: "name a boundary as `<Entity>.<path>` or `<Component>.<path>`, such as \
                  `Task.repo.fake`, or an artifact id from generated provenance"
                .to_string(),
        });
    };
    let Some(owner) = owner_of(name).map_err(|message| Unresolved {
        message,
        fix: "rename one of them, or eject by artifact id".to_string(),
    })?
    else {
        return Err(Unresolved {
            message: format!("`{name}` is not an entity or component of this model"),
            fix: "name a declared entity by its Java type or a component by its name".to_string(),
        });
    };
    let mut paths = rows_for(owner.owner).map(|row| row.path).peekable();
    if paths.peek().is_none() {
        return Err(Unresolved {
            message: format!("`{name}` has no registered implementation boundary"),
            fix: "eject by the artifact id generated provenance reports".to_string(),
        });
    }
    let Some(row) = rows_for(owner.owner).find(|row| row.path == rest) else {
        return Err(Unresolved {
            message: format!("`{name}` has no boundary `{rest}`"),
            fix: format!(
                "use one of: {}",
                paths
                    .map(|path| format!("`{name}.{path}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    };
    row.artifact_id(&owner.id, storage)
        .ok_or_else(|| Unresolved {
            message: format!("`{path}` needs a primary storage, and the model declares none"),
            fix: "declare `storage postgres` in the app block".to_string(),
        })
}

/// Resolve a readable boundary path against a linked model.
///
/// What `jails model eject <path>` calls, through the same resolver the
/// linker uses on an `eject` line, so the command and the source agree on
/// which artifact a path names.
pub fn resolve_in(model: &crate::AppModel, path: &str) -> Result<String, Unresolved> {
    crate::ejection::resolver(&model.entities, &model.components, &model.capabilities)(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owners(name: &str) -> Result<Option<Resolved>, String> {
        Ok(match name {
            "Task" => Some(Resolved {
                owner: Owner::Entity,
                id: "ent_task".to_string(),
            }),
            "Audit" => Some(Resolved {
                owner: Owner::Component(ComponentKind::Client),
                id: "cmp_client_audit".to_string(),
            }),
            "Plain" => Some(Resolved {
                owner: Owner::Component(ComponentKind::Class),
                id: "cmp_class_plain".to_string(),
            }),
            _ => None,
        })
    }

    #[test]
    fn the_specification_paths_resolve_to_the_ids_the_compiler_emits() {
        assert_eq!(
            resolve("Task.repo.fake", owners, None).unwrap(),
            "art_ent_task_repository_memory"
        );
        assert_eq!(
            resolve("Task.record", owners, None).unwrap(),
            "art_ent_task_record"
        );
        assert_eq!(
            resolve("Task.http.api", owners, None).unwrap(),
            "art_ent_task_http_controller"
        );
        assert_eq!(
            resolve("Task.repo.postgres", owners, Some("cap_db")).unwrap(),
            "art_cap_db_ent_task_repository"
        );
        assert_eq!(
            resolve("Audit.implementation", owners, None).unwrap(),
            "art_cmp_client_audit_config"
        );
    }

    #[test]
    fn a_storage_scoped_boundary_needs_a_storage() {
        let refused = resolve("Task.repo.postgres", owners, None).unwrap_err();
        assert!(
            refused.message.contains("primary storage"),
            "{}",
            refused.message
        );
    }

    #[test]
    fn an_unknown_path_lists_the_owner_s_boundaries() {
        let refused = resolve("Task.repo.mysql", owners, None).unwrap_err();
        assert!(refused.fix.contains("`Task.repo.fake`"), "{}", refused.fix);
        let refused = resolve("Nobody.record", owners, None).unwrap_err();
        assert!(
            refused.message.contains("not an entity"),
            "{}",
            refused.message
        );
        let refused = resolve("Plain.implementation", owners, None).unwrap_err();
        assert!(
            refused.message.contains("no registered"),
            "{}",
            refused.message
        );
    }

    /// Two rows with one `(owner, path)` would make `resolve` pick whichever
    /// came first.
    #[test]
    fn every_path_is_registered_once_per_owner() {
        let mut seen = std::collections::BTreeSet::new();
        for row in BOUNDARIES {
            assert!(
                seen.insert((format!("{:?}", row.owner), row.path)),
                "`{}` is registered twice for {:?}",
                row.path,
                row.owner
            );
        }
    }
}
