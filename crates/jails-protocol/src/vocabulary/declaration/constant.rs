//! One enum constant, and what it is called outside the application.
//!
//! `missing.md` M14. `g enum` uppercased whatever it was given and that was
//! the whole vocabulary, so three of the four closed sets in one real project
//! could not be expressed at all: `open`/`in_progress` (lowercase),
//! `Account`/`Billing` (TitleCase), and `-`/`!`/`!!`, which are not Java
//! identifiers in any casing.
//!
//! **The Java name and the wire value are two different things**, and the
//! failure of treating them as one is quiet: an enum whose constants are
//! `OPEN` and `IN_PROGRESS` serialises as `"OPEN"`, the page reads `"open"`,
//! and the badge is simply blank. So a constant is a `Name` and, optionally, a
//! string -- and the string is deliberately *not* a `Name`, because the whole
//! point is the values a `Name` cannot hold.
//!
//! The database is not the wire. A stored enum is stored by its Java name and
//! the `check` constraint lists those, because a column is an internal
//! contract with one reader; the wire value is the external one.

use super::*;

/// A constant declared as `NAME` or `NAME=wire`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstantSpec {
    pub name: Name,
    /// What this constant is called outside the application, when that is not
    /// its own name.
    pub wire: Option<String>,
}

/// The Java spelling of a constant, from whatever was typed.
///
/// `gbp` and `GBP` are the same constant, and `in_progress` and
/// `IN_PROGRESS` are too. It normalises **here**, in the one parser, so
/// the name a ledger records and the name a generator writes cannot
/// differ -- which is the whole reason `FieldSpec::parse` is where it is.
fn constant_name(text: &str) -> Result<Name> {
    let normalised: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    Name::parse(&normalised).map_err(|_| {
        jails_support::Failure::Told(format!(
            "`{text}` is not a usable enum constant.\n       fix: a constant is a Java \
             identifier; write `NAME=wire` when the value on the wire is not one."
        ))
    })
}

impl ConstantSpec {
    /// `OPEN` or `OPEN=open`.
    ///
    /// The wire half accepts anything printable and non-empty: it is a string
    /// somebody else's client already sends, and jails' job is to carry it
    /// exactly, not to have an opinion about it. What it may not contain is a
    /// quote or a backslash, because it is rendered into a Java string literal
    /// and escaping it would be jails inventing an encoding.
    pub fn parse(token: &str) -> Result<Self> {
        let token = token.trim();
        let Some((name, wire)) = token.split_once('=') else {
            return Ok(Self {
                name: constant_name(token)?,
                wire: None,
            });
        };
        let name = constant_name(name.trim())?;
        let wire = wire.trim();
        if wire.is_empty() {
            return Err(format!(
                "`{name}=` declares an empty wire value.\n       fix: write `{name}` for a \
                 constant that is called its own name on the wire."
            )
            .into());
        }
        if wire.contains(['"', '\\']) || wire.chars().any(|c| c.is_control()) {
            return Err(format!(
                "the wire value for `{name}` contains a quote, a backslash or a control \
                 character.\n       fix: jails renders it into a Java string literal verbatim \
                 and will not invent an escaping for it."
            )
            .into());
        }
        Ok(Self {
            name,
            wire: Some(wire.to_string()),
        })
    }

    /// The spelling that reproduces this constant.
    pub fn canonical(&self) -> String {
        match &self.wire {
            Some(wire) => format!("{}={wire}", self.name),
            None => self.name.to_string(),
        }
    }

    /// What this constant is called on the wire, which is its own name when
    /// nothing else was said.
    pub fn wire_value(&self) -> &str {
        self.wire.as_deref().unwrap_or_else(|| self.name.as_str())
    }
}

impl Codec for ConstantSpec {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        self.name.encode(encoder)?;
        encoder.option(self.wire.as_ref(), |e, wire| e.string(wire))
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(Self {
            name: Name::decode(decoder)?,
            // Through `parse`, so a value a recovered journal carries is one
            // the CLI would have accepted -- `CLAUDE.md`'s rule that every
            // wire decoder calls the same constructor.
            wire: decoder
                .option(|d| d.string())?
                .map(|wire| {
                    Self::parse(&format!("A={wire}")).map(|spec| spec.wire_value().to_string())
                })
                .transpose()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_constant_is_called_its_own_name() {
        let spec = ConstantSpec::parse("OPEN").unwrap();
        assert_eq!(spec.name.as_str(), "OPEN");
        assert_eq!(spec.wire, None);
        assert_eq!(spec.wire_value(), "OPEN");
        assert_eq!(spec.canonical(), "OPEN");
    }

    /// The three shapes one real project needed, none of which a `Name` holds.
    #[test]
    fn a_wire_value_may_be_anything_the_client_already_sends() {
        for (token, name, wire) in [
            ("IN_PROGRESS=in_progress", "IN_PROGRESS", "in_progress"),
            ("ACCOUNT=Account", "ACCOUNT", "Account"),
            ("URGENT=!!", "URGENT", "!!"),
            ("NONE=-", "NONE", "-"),
        ] {
            let spec = ConstantSpec::parse(token).unwrap();
            assert_eq!(spec.name.as_str(), name);
            assert_eq!(spec.wire_value(), wire);
            assert_eq!(spec.canonical(), token);
        }
    }

    /// The name half is normalised here and nowhere else, so the constant a
    /// recorded intent carries is the constant a generator writes.
    #[test]
    fn the_name_half_is_normalised_by_the_one_parser() {
        assert_eq!(ConstantSpec::parse("gbp").unwrap().name.as_str(), "GBP");
        assert_eq!(
            ConstantSpec::parse("in-progress=in_progress")
                .unwrap()
                .name
                .as_str(),
            "IN_PROGRESS"
        );
        // A bare token keeps the behaviour it always had: uppercased, and no
        // wire value at all.
        assert_eq!(ConstantSpec::parse("gbp").unwrap().wire, None);
        assert!(ConstantSpec::parse("1ST").is_err());
        assert!(ConstantSpec::parse("OPEN=").is_err());
        assert!(ConstantSpec::parse("OPEN=a\"b").is_err());
    }
}
