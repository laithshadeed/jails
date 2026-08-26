use super::{Name, SqlName};
use crate::Result;
use jails_support::codec::{Codec, Decoder, Encoder};

/// A declared field's name, holding **both** renderings a field has.
///
/// plan.md P3.1. One concept had three renderings and none of them was
/// recorded: the spec string went into the Java record verbatim, `sql.rs`
/// snake-cased it for the column, and a reader typing the other spelling of
/// the same field got a second field. `user_id:uuid` produced a record
/// component called `user_id` -- which is not a Java name -- while
/// `userId:uuid` produced a column called `user_id` all the same, so the two
/// declarations disagreed about the Java half and agreed about the SQL half.
///
/// So the type owns the derivation rather than each caller owning a `format!`:
/// **the column is the normal form, and the Java name is derived from it.**
/// That is what makes `user_id`, `userId` and `user_ID` one field rather than
/// three -- snake-casing is the step that erases the difference, and camelising
/// the result puts every one of them back as `userId`.
///
/// The one error case is a name that cannot produce a Java identifier by
/// convention: `_id`, `id_` and `a__b` all snake-case to something with an
/// empty segment, and a segment-joining rule has no answer for one. Refusing
/// is the whole point -- silently dropping the underscore would rename the
/// reader's field, and keeping it would emit a column jails cannot round-trip.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct FieldName {
    java: Name,
    column: SqlName,
}

impl FieldName {
    pub fn parse(text: &str) -> Result<Self> {
        // The Java identifier rules and the reserved-word list first, so
        // `field name `int`` is refused as what was typed rather than as a
        // derivation of it.
        let declared = Name::parse(text)?;
        let column = SqlName::conventional_column(&declared);
        let java = camel_case(column.as_str()).ok_or_else(|| {
            jails_support::Failure::Told(format!(
                "field name `{text}` has a word in it that does not start with a letter, so it \
                 has no conventional Java spelling.\n       fix: drop the leading, trailing or \
                 doubled `_`, and start each word with a letter -- `userId` and `user_id` are \
                 the same field."
            ))
        })?;
        // Checked against the *derived* name, not the declared one: `Class`
        // and `get_class` are refused for the same reasons `class` and
        // `getClass` are, and only the derivation can see it.
        let java = Name::parse(&java)?;
        reject_record_component_name(java.as_str())?;
        reject_sql_keyword(column.as_str())?;
        Ok(Self { java, column })
    }

    /// The record component, the DTO field and the accessor: lowerCamelCase.
    pub fn java(&self) -> &str {
        self.java.as_str()
    }

    /// This field as the plain [`Name`] the rest of the protocol carries --
    /// always the Java rendering, since that is the canonical spelling.
    pub fn as_name(&self) -> &Name {
        &self.java
    }

    /// The unquoted SQL column: snake_case.
    pub fn column(&self) -> &SqlName {
        &self.column
    }

    /// The one canonical spelling of this name. Deliberately the Java one:
    /// the field spec is a Java-facing vocabulary, and `column()` is reachable
    /// from it by a total function.
    pub fn as_str(&self) -> &str {
        self.java.as_str()
    }
}

impl Codec for FieldName {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(self.java.as_str())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.string()?)
    }
}

impl std::fmt::Display for FieldName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.java.as_str())
    }
}

impl PartialEq<Name> for FieldName {
    /// A field is looked up by whichever spelling the reader typed, so
    /// `jails resource field drop user_id` and `... userId` find the same one.
    fn eq(&self, other: &Name) -> bool {
        FieldName::parse(other.as_str()).is_ok_and(|parsed| parsed.java == self.java)
    }
}

/// `user_id` -> `userId`. `None` for any input with a word that does not
/// start with a letter, which is the one thing a segment-joining rule cannot
/// answer.
///
/// **The refusal is what makes the mapping reversible**, and reversible is
/// what lets one check stand in for two: with every word starting with a
/// lowercase letter, the capitals in the camel form are exactly the word
/// boundaries, so two distinct columns can never camelise to one Java name.
/// `a_1b` and `a1b` both become `a1b` and would otherwise reach a record as
/// two components of the same name.
fn camel_case(column: &str) -> Option<String> {
    let mut words = column.split('_');
    let first = words.next().filter(|word| starts_with_letter(word))?;
    let mut out = String::with_capacity(column.len());
    out.push_str(first);
    for word in words {
        let mut characters = word.chars();
        let initial = characters.next().filter(char::is_ascii_lowercase)?;
        out.extend(initial.to_uppercase());
        out.push_str(characters.as_str());
    }
    Some(out)
}

fn starts_with_letter(word: &str) -> bool {
    word.chars()
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
}

fn reject_record_component_name(name: &str) -> Result<()> {
    if matches!(
        name,
        "clone"
            | "equals"
            | "finalize"
            | "getClass"
            | "hashCode"
            | "notify"
            | "notifyAll"
            | "toString"
            | "wait"
    ) {
        return Err(format!(
            "field name `{name}` conflicts with java.lang.Object record behavior.\n       \
             fix: choose a domain-specific component name such as `contentHash` or `description`."
        )
        .into());
    }
    Ok(())
}

fn reject_sql_keyword(column: &str) -> Result<()> {
    if SqlName::is_postgres_reserved(column) {
        return Err(format!(
            "field name `{column}` is reserved by PostgreSQL and would make generated SQL \
             invalid.\n       fix: choose a domain-specific name such as `source`, `target`, or \
             `sortOrder`."
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_spellings_of_one_field_converge() {
        for spelling in ["user_id", "userId", "userID"] {
            let name = FieldName::parse(spelling).expect(spelling);
            assert_eq!(name.java(), "userId", "java() of `{spelling}`");
            assert_eq!(
                name.column().as_str(),
                "user_id",
                "column() of `{spelling}`"
            );
        }
    }

    #[test]
    fn a_single_word_is_unchanged_in_both_renderings() {
        let name = FieldName::parse("email").expect("email");
        assert_eq!(name.java(), "email");
        assert_eq!(name.column().as_str(), "email");
    }

    #[test]
    fn a_word_that_does_not_start_with_a_letter_has_no_java_spelling() {
        // The last two are why the rule is about letters and not only about
        // emptiness: `a_1b` and `a1b` would otherwise both camelise to `a1b`,
        // and the mapping this type is built on would stop being reversible.
        for spelling in ["_id", "id_", "a__b", "a_1b"] {
            let failure = FieldName::parse(spelling).expect_err(spelling);
            assert!(
                failure.to_string().contains("does not start with a letter"),
                "`{spelling}`: {failure}"
            );
        }
    }

    #[test]
    fn two_distinct_columns_never_camelise_to_one_java_name() {
        // Exhaustive over a small alphabet: the property one duplicate check
        // now stands on, rather than a pair of examples.
        let mut seen = std::collections::HashMap::new();
        for column in ["a", "ab", "a_b", "ab_c", "a_bc", "a1", "a1_b", "a_b1"] {
            let name = FieldName::parse(column).expect(column);
            let previous = seen.insert(name.java().to_string(), column);
            assert_eq!(previous, None, "`{column}` collides with {previous:?}");
        }
    }

    #[test]
    fn a_reserved_word_is_refused_through_its_derived_spelling() {
        // `Class` is a legal Java identifier and snake-cases to `class`,
        // which is not.
        assert!(FieldName::parse("Class").is_err());
        // `get_class` derives `getClass`, which a record cannot declare.
        assert!(FieldName::parse("get_class").is_err());
        // `desc` is reserved by PostgreSQL, and so is the column `user_id`
        // is not -- the check reads the derived column, not the spec string.
        assert!(FieldName::parse("desc").is_err());
    }

    #[test]
    fn a_field_is_found_by_either_spelling() {
        let name = FieldName::parse("createdAt").expect("createdAt");
        assert!(name == Name::parse("created_at").expect("created_at"));
        assert!(name == Name::parse("createdAt").expect("createdAt"));
        assert!(!(name == Name::parse("updatedAt").expect("updatedAt")));
    }
}
