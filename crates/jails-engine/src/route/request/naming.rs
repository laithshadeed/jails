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

/// Refuse a name that would shadow a `java.lang` type in its own package.
///
/// `bugs.md` B50: `g record String value:string` wrote
/// `public record String(String value)` -- a package member outranks the
/// implicit `java.lang` import, so the component is typed as the record rather
/// than as text. The caller asked for a string field and got a self-reference,
/// and **it compiles**, as does the generated test, so the real-toolchain tier
/// -- the only one that answers the question this tool exists for -- was green
/// over it.
///
/// Asked of the rendered plan for the same reason [`refuse_reserved_variable`]
/// is: only a *declaration* shadows. `Name` validates references too, so a
/// refusal there would have refused `value:String` along with it.
///
/// The list is [`jails_protocol::identity::JAVA_LANG`], read off `java.base`'s
/// own class list rather than recalled -- a hand-written subset is a check
/// that silently stops applying to whatever it omits.
pub(crate) fn refuse_java_lang_shadow(
    id: &IntentId,
    typed: &str,
    change: &jails_project::model::Change,
) -> Result<()> {
    let name = id.name.as_str();
    if !jails_protocol::identity::JAVA_LANG.contains(&name) {
        return Ok(());
    }
    // `record X(`, `class X `, `class X<`, `interface X ` -- the four
    // declaration shapes jails emits, read through `blanked()` so the word in
    // a Javadoc sentence is not one.
    let declarations = [
        format!("record {name}("),
        format!("class {name} "),
        format!("class {name}<"),
        format!("interface {name} "),
    ];
    let hit = change.files.iter().find(|artifact| {
        artifact
            .path
            .extension()
            .is_some_and(|extension| extension == "java")
            && {
                let text = jails_java::java::blanked(&artifact.contents);
                declarations.iter().any(|shape| text.contains(shape))
            }
    });
    let Some(hit) = hit else {
        return Ok(());
    };
    Err(format!(
        "entity name `{typed}` is a type in `java.lang`, which every Java file imports \
         implicitly, and `{}` declares it.\n       Inside that package `{name}` would stop \
         meaning `java.lang.{name}` -- so a `{name}` component would be typed as the thing being \
         declared.\n       It compiles, which is why nothing downstream would report it.\n       \
         fix: choose an entity name that is not one of Java's own.",
        hit.path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default()
    )
    .into())
}
