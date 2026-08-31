//! How a Java value reaches a JDBC parameter, and which columns may not be
//! written at all.
//!
//! Split from [`super`] by secret: that module decides the schema -- the DDL, a
//! migration, a column's type -- and this one decides what happens at the bind
//! site. They move for different reasons, and the two questions met only
//! because both mention a column.
//!
//! Both answers here are ones no generated code could state for itself, and
//! both were wrong until a generated integration test was finally run against
//! a real PostgreSQL.

use super::{SqlDefault, declares_enum, sql_default};
use crate::CompileError;
use jails_model::{AppModel, Field, TypeRef};
use std::collections::BTreeSet;

/// Whether the database assigns this column, so an insert must not name it.
///
/// `generated always as identity` is not merely defaulted -- PostgreSQL
/// refuses an insert that supplies a value at all, so this is the difference
/// between a repository that can store a row and one that cannot.
pub(crate) fn database_assigned(field: &Field) -> Result<bool, CompileError> {
    Ok(matches!(sql_default(field)?, Some(SqlDefault::Identity)))
}

/// How a Java value of this field's type is bound as a JDBC parameter.
///
/// **The receiver is baked in rather than prefixed by the caller.**
/// `Timestamp.from(x.at())` puts it in the middle, so gluing a wrapper on the
/// front yields `x.Timestamp.from(at())` -- which reads fine and does not
/// compile. Same rule the legacy `sql.rs` records, and the reason this is one
/// function rather than a wrapper each call site remembers.
///
/// It exists because the PostgreSQL driver refuses to infer a type for
/// `java.time.Instant` -- *"Can't infer the SQL type to use for an instance of
/// java.time.Instant"* -- so every generated repository over an entity with a
/// timestamp could not insert a row. `timestamps = true` gives one to every
/// scaffold, so that was very nearly every canonical project, and nothing
/// caught it because no generated integration test had ever run.
pub(crate) fn bound_value(
    model: &AppModel,
    field: &Field,
    accessor: &str,
    imports: &mut BTreeSet<String>,
) -> String {
    match &field.ty {
        TypeRef::Builtin(builtin) => match builtin.semantics().token {
            "instant" => {
                imports.insert("java.sql.Timestamp".to_string());
                format!("Timestamp.from({accessor})")
            }
            // Stored as `text`, and the driver has no mapping from the Java
            // type to it.
            "duration" | "uri" | "path" | "zone-id" | "currency" => {
                format!("{accessor}.toString()")
            }
            _ => accessor.to_string(),
        },
        // A declared enum is a `text` column. `name()` rather than
        // `toString()`, which a reader may override.
        TypeRef::External(name) if declares_enum(model, name) => format!("{accessor}.name()"),
        TypeRef::External(_) => accessor.to_string(),
    }
}
