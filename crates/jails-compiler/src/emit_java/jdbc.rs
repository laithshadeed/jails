//! How a value crosses the JDBC boundary.
//!
//! **One owner, because five renderers bind parameters.** The repository
//! adapter, a command's insert, a query's filters, a transition's assignments
//! and its guards all hand values to `JdbcClient`, and pgjdbc refuses a value
//! it has no type code for -- at runtime, with a message naming neither the
//! column nor the statement. Four of the five bound raw, so an entity carrying
//! an `instant` or an enum failed on contact with a database and compiled
//! perfectly until then.
//!
//! The conversion itself is a fact about the builtin and lives on its row in
//! `jails-model` beside `sql_postgres`. This applies it, and owns the naming
//! rule deciding which accessor it is applied to.

use super::*;

/// One field's value, converted for binding as a JDBC parameter.
///
/// **One owner, because every adapter binds and none of them converted.**
/// pgjdbc refuses a value it has no type code for, at runtime, with a message
/// naming neither the column nor the statement -- so `save` failed on any
/// entity carrying an `instant`, which is every entity with `timestamps`.
/// The conversion is a fact about the builtin and lives on its row beside
/// `sql_postgres`; this applies it.
///
/// An optional component is unwrapped first, so the conversion sees the value
/// rather than the `Optional` -- and a null one stays null, which is what the
/// column takes.
pub(crate) fn jdbc_param(field: &Field, accessor: &str) -> String {
    let template = match &field.ty {
        TypeRef::Builtin(builtin) => builtin.semantics().jdbc_write,
        // A model-declared type reaches the column as `text`, and the only
        // one `emit_sql` will render a column for is an enum -- anything else
        // is refused there with "no declared SQL representation", so nothing
        // else can reach a binding site. `name()` is the spelling the column
        // holds and the row mapper reads back.
        TypeRef::External(_) => Some("{}.name()"),
    };
    match template {
        None => accessor.to_string(),
        Some(template) => template.replace("{}", accessor),
    }
}

/// The same conversion for a component that may be absent.
///
/// Unwrapped before converting and null after: a conversion applied to the
/// `Optional` would not compile, and one applied after `orElse(null)` would
/// call a method on null.
pub(crate) fn optional_jdbc_param(field: &Field, accessor: &str) -> String {
    let converted = jdbc_param(field, "value");
    if converted == "value" {
        format!("{accessor}.orElse(null)")
    } else {
        format!("{accessor}.map(value -> {converted}).orElse(null)")
    }
}

/// The Java member an operation parameter becomes.
///
/// **One owner, because three renderers name the same accessor.** A parameter
/// is a reference to a field, spelled in the model's label alphabet, so
/// `user_id` must reach Java as `userId` -- and the record shape, the insert
/// adapter and the resolve lookup all have to agree or the generated class
/// calls an accessor its own record does not declare. They did not: the
/// transition and query adapters projected the field, the command adapter and
/// the record shape read the label back, and `record Input(long user_id)` met
/// a request body binding `userId`, which Jackson resolved to `null` and
/// mapped into a primitive.
pub(crate) fn parameter_member(model: &AppModel, parameter: &OperationParameter) -> String {
    match &parameter.source {
        ParameterSource::Field(visible) => model
            .entities
            .get(&visible.entity)
            .and_then(|owner| owner.field(&visible.field))
            .map(|field| field.names.java_member.clone())
            .unwrap_or_else(|| jails_model::lower_camel_case(&parameter.name)),
        ParameterSource::Typed(_) => jails_model::lower_camel_case(&parameter.name),
    }
}
