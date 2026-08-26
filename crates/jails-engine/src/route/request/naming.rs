//! Whether a generated name is one the emitted Java can actually carry.
//!
//! A different question from the rest of [`super`], which assembles a request
//! out of values already known to be well formed. This one is asked *of the
//! rendered plan*: the hazard is not the name, it is the declaration the name
//! produces, and only the bytes know whether there is one.

use super::*;

/// The lower-camel variable name a generator derives from a type name.
///
/// One spelling of the rule `jails_generate::generate::lower_first` renders
/// into templates, so the check below asks the same question the emitted Java
/// answers.
fn derived_variable(name: &str) -> String {
    let mut characters = name.chars();
    characters
        .next()
        .map(|first| first.to_lowercase().collect::<String>() + characters.as_str())
        .unwrap_or_default()
}

/// Refuse a name whose generated Java would declare a variable that is a Java
/// keyword.
///
/// `g scaffold class` was accepted and produced `ClassService.java` holding
/// `Class class`, which `javac` rejects with `<identifier> expected` in every
/// file that constructs the subject -- and by then the create-table migration
/// is sealed, so the way out is the one-way door.
///
/// The check is on the **rendered plan**, not on the kind. A name is only a
/// hazard where a template actually declares `<Type> <instance>`, and which
/// kinds do that is a fact about the generators rather than a list worth
/// keeping beside them: `g command Import` names no `Import import`, and
/// refusing it would cost a legitimate domain word for a collision that does
/// not exist. Asking the bytes cannot drift from the bytes.
pub(crate) fn refuse_reserved_variable(
    id: &IntentId,
    typed: &str,
    change: &jails_project::model::Change,
) -> Result<()> {
    let instance = derived_variable(id.name.as_str());
    if Name::parse(&instance).is_ok() {
        return Ok(());
    }
    let declaration = format!("{} {instance}", id.name);
    let hit = change.files.iter().find(|artifact| {
        artifact
            .path
            .extension()
            .is_some_and(|extension| extension == "java")
            && jails_java::java::blanked(&artifact.contents).contains(&declaration)
    });
    let Some(hit) = hit else {
        return Ok(());
    };
    Err(format!(
        "entity name `{typed}` derives Java variable `{instance}`, which is a Java keyword, and \
         `{}` declares it.\n       fix: choose a domain-specific entity name whose lower-camel \
         spelling is not a Java keyword.",
        hit.path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default()
    )
    .into())
}
