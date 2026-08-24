//! What a recorded intent declared, read back out of the ledger.
//!
//! One function, and its whole reason for existing is that a Java type cannot
//! say what its column is. `@pk`, `@unique` and `@index` ride on the *spec* a
//! generate call recorded, not on the record it produced -- so a scaffold that
//! composes a previously generated record has to ask the store what that
//! record declared. Inferring a primary key from a component called `id` would
//! put one in a schema nobody asked for.
//!
//! This module used to *be* the storage, and then the vocabulary over a
//! schema-1 storage: `record`, `paths`, `forget`, `record_model` beside this
//! one, over `.jails/files`, `.jails/version`, `.jails/intents/<hash>.files`
//! and `.jails/models/<hash>.files`. All of it went with the direct write
//! path: the transaction store records what an entity owns as part of
//! committing it, so a second registry maintained beside it could only drift.

use jails_support::Result;
use std::path::Path;

/// The fields the recorded intent for `name` declared, if there is one.
///
/// Name always; package only when one was asked for. An entity id records the
/// *resolved* package (`com.example.demo`), while `--package` is the override
/// the caller typed (`billing`) and is absent far more often than not.
/// Comparing the two directly matched nothing, so a scaffold referencing a
/// generated record was told that record had no primary key -- which it
/// plainly did.
pub fn model_fields(root: &Path, name: &str, package: Option<&str>) -> Result<Option<Vec<String>>> {
    let Ok(source) = std::fs::read_to_string(root.join(".jails/ledger.toml")) else {
        return Ok(None);
    };
    let Ok(ledger) = jails_protocol::envelope::LedgerV2::parse_file(&source) else {
        return Ok(None);
    };
    Ok(ledger.applied.iter().find_map(|entity| {
        let jails_protocol::entity::EntityId::Intent(id) = &entity.id else {
            return None;
        };
        let jails_protocol::entity::EntitySpec::Intent(spec) = &entity.version.spec else {
            return None;
        };
        let matches_package = match package {
            None => true,
            Some(asked) => {
                id.package.as_str() == asked || id.package.as_str().ends_with(&format!(".{asked}"))
            }
        };
        (id.name.as_str() == name && matches_package).then(|| spec.arguments.canonical())
    }))
}
