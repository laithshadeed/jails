//! What a declaration's stable id is when the source leaves `@id` unsaid.
//!
//! **One owner, because two of them is a model that means something different
//! read back than it did written.** The parser needs this to resolve a
//! declaration with no `@id`; the CLI needs the same answer to decide whether
//! writing one would say anything. Written twice they drifted immediately --
//! the CLI spelled an enum `ent_colour` while the parser derived
//! `enum_colour`, so a hand-written enum and a generated one had different
//! artifact ids for the same declaration.
//!
//! **`@id` is materialised only where it differs from the derivation**, which
//! is the same rule [`crate::DerivedValue::named`] uses for `pinned`: a pin
//! that agrees with the convention has displaced nothing, so writing it says
//! nothing and costs the reader a hash on every line. [`id_attribute`] is that
//! decision, and every writer goes through it.
//!
//! The folds are here too. A caller passes the name as the reader typed it and
//! this module applies the model's own label fold, because a writer
//! folding with its own copy is how the two halves disagree about `orderId`.

use crate::naming::stable_fragment;

/// The `@id(...)` a writer should emit, including its leading space.
///
/// Empty when the id is the one the parser derives, which is the common case
/// and the reason a generated model reads like the specification's.
pub fn id_attribute(id: &str, derived: &str) -> String {
    match id == derived {
        true => String::new(),
        false => format!(" @id({id})"),
    }
}

/// `app Notes` — the application's own identity.
pub fn app_id(name: &str) -> String {
    format!("app_{}", stable_fragment(name))
}

/// `cap json`, keyed by the capability's label rather than its alias.
pub fn capability_id(label: &str) -> String {
    format!("cap_{}", stable_fragment(label))
}

/// `dep org.jspecify:jspecify`.
pub fn dependency_id(coordinate: &str) -> String {
    format!("dep_{}", stable_fragment(coordinate))
}

/// `prop server.port` — a setting is its own key, so a hash of the key adds
/// nothing a reader can use.
pub fn setting_id(key: &str) -> String {
    format!("prop_{}", stable_fragment(key))
}

/// `entity Note` and `enum Colour` share one namespace, so they share one
/// prefix: both are entities in the linked model, and an enum whose id said
/// `enum_` gave the same declaration a different artifact id depending on
/// whether a hand or the CLI wrote it.
pub fn entity_id(name: &str) -> String {
    format!("ent_{}", stable_fragment(name))
}

/// A field's identity hangs off its entity's, so pinning the entity's `@id`
/// through a rename keeps every field id with it and the rename materialises
/// exactly one attribute.
///
/// **The owner's kind prefix is not repeated.** `fld_ent_note_title` says
/// "field" and "entity" for one field of one entity, and a rename is where
/// that id stops agreeing with the name and gets written into the model for
/// a reader to see -- so it is `fld_note_title`, which is also the spelling
/// every project generated before this rule carries. An entity whose id is
/// pinned to something else keeps that text whole, because there is no
/// convention left to strip.
pub fn field_id(entity_id: &str, name: &str) -> String {
    format!(
        "fld_{}_{}",
        owner_fragment(entity_id),
        stable_fragment(name)
    )
}

/// An owner id with its own kind prefix removed, for the ids hung off it.
///
/// **Fields only, and the reason is that the key already shipped.**
/// `fld_note_title` is what every generated project on disk carries, so this
/// restores that spelling rather than inventing one. A relation, a constraint
/// and an index were never written with an `@id` by the CLI, so their derived
/// `rel_ent_item_owner` *is* the shipped key -- and a stable id is a merge
/// pairing, so an id that reads slightly better is not worth moving one.
fn owner_fragment(entity_id: &str) -> &str {
    entity_id.strip_prefix("ent_").unwrap_or(entity_id)
}

/// A `pk [...]` or `unique [...]` member, named by the columns it constrains.
pub fn constraint_id(prefix: &str, entity_id: &str, columns: &[String]) -> String {
    format!("{prefix}_{entity_id}_{}", column_suffix(columns))
}

/// An `index [...]` member, named by the columns it indexes.
pub fn index_id(entity_id: &str, columns: &[String]) -> String {
    format!("idx_{entity_id}_{}", column_suffix(columns))
}

/// A `relation` member.
pub fn relation_id(entity_id: &str, label: &str) -> String {
    format!("rel_{entity_id}_{}", stable_fragment(label))
}

/// A `command`, `query`, `transition` or `event`.
pub fn operation_id(name: &str) -> String {
    format!("op_{}", stable_fragment(name))
}

/// A `component <kind> <name>`; the kind is part of the identity because two
/// kinds may legally name one thing.
pub fn component_id(kind: &str, name: &str) -> String {
    format!("cmp_{}_{}", kind.replace('-', "_"), stable_fragment(name))
}

/// A `variant` inside a component, hung off the component's id for the same
/// reason a field hangs off its entity's.
pub fn variant_id(component_id: &str, name: &str) -> String {
    format!("var_{component_id}_{}", stable_fragment(name))
}

/// An `eject <boundary>` line.
pub fn ejection_id(target: &str) -> String {
    format!("eject_{}", stable_fragment(target))
}

/// The column list a constraint or index is named by.
///
/// The columns arrive as the reader wrote them -- `createdAt` -- and an
/// ordering (`created_at desc`) is stripped by the caller that parsed it.
fn column_suffix(columns: &[String]) -> String {
    columns
        .iter()
        .map(|column| stable_fragment(column))
        .collect::<Vec<_>>()
        .join("_")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of the module: a writer that agrees with the parser
    /// writes nothing.
    #[test]
    fn an_id_that_matches_the_convention_is_not_written() {
        assert_eq!(id_attribute("ent_note", &entity_id("Note")), "");
        assert_eq!(
            id_attribute("ent_task", &entity_id("Note")),
            " @id(ent_task)"
        );
    }

    /// `enum` and `entity` are one namespace in the linked model, so a
    /// generated `Colour` and a hand-written one have to be the same id.
    #[test]
    fn enums_and_entities_share_one_identity_prefix() {
        assert_eq!(entity_id("Colour"), "ent_colour");
    }

    /// The fold is this module's, so a caller passing the reader's spelling
    /// gets the parser's answer.
    #[test]
    fn names_are_folded_here_rather_than_by_the_caller() {
        assert_eq!(field_id("ent_note", "createdAt"), "fld_note_created_at");
        assert_eq!(field_id("x9", "createdAt"), "fld_x9_created_at");
        assert_eq!(
            index_id("ent_note", &["userId".to_string(), "createdAt".to_string()]),
            "idx_ent_note_user_id_created_at"
        );
        assert_eq!(
            component_id("state-machine", "Order"),
            "cmp_state_machine_order"
        );
    }
}
