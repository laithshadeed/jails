use super::Name;
use crate::Result;
use jails_support::codec::{Codec, Decoder, Encoder};

/// A validated unquoted SQL identifier used at destructive lifecycle
/// boundaries. Generated table names are lowercase snake case, so accepting a
/// broader SQL expression here would make exact confirmation meaningless.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct SqlName(String);

impl SqlName {
    pub fn parse(value: &str) -> Result<Self> {
        let mut chars = value.chars();
        let Some(first) = chars.next() else {
            return Err(
                "SQL name is empty.\n       fix: pass the exact generated table name.".into(),
            );
        };
        if !(first.is_ascii_lowercase() || first == '_')
            || !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        {
            return Err(format!(
                "`{value}` is not a lowercase unquoted SQL name.\n       \
                 fix: pass the exact generated table name, for example `tasks`."
            )
            .into());
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether PostgreSQL requires this otherwise-valid identifier to be
    /// quoted because it is part of the SQL grammar. Generated SQL uses
    /// unquoted names deliberately, so callers must refuse these at the
    /// declaration boundary.
    pub fn is_postgres_reserved(value: &str) -> bool {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "all"
                | "any"
                | "array"
                | "asc"
                | "as"
                | "cast"
                | "check"
                | "collate"
                | "column"
                | "constraint"
                | "cross"
                | "current_date"
                | "current_time"
                | "current_user"
                | "desc"
                | "distinct"
                | "end"
                | "except"
                | "foreign"
                | "from"
                | "full"
                | "grant"
                | "group"
                | "having"
                | "ilike"
                | "in"
                | "inner"
                | "into"
                | "is"
                | "join"
                | "leading"
                | "left"
                | "like"
                | "limit"
                | "natural"
                | "offset"
                | "on"
                | "only"
                | "or"
                | "order"
                | "outer"
                | "primary"
                | "references"
                | "right"
                | "select"
                | "similar"
                | "some"
                | "table"
                | "then"
                | "to"
                | "union"
                | "unique"
                | "user"
                | "using"
                | "when"
                | "where"
                | "window"
                | "with"
        )
    }

    /// The one conventional physical table spelling for a logical entity.
    ///
    /// Storage adoption and generators both call this function so a rename
    /// cannot preserve one inferred table while generated SQL uses another.
    pub fn conventional_table(entity: &Name) -> Self {
        Self(plural_snake_case(entity.as_str()))
    }

    /// The one conventional physical column spelling for a Java component.
    ///
    /// Declaration validation and SQL generation both call this function so
    /// two distinct Java names cannot quietly become one unquoted column.
    pub fn conventional_column(component: &Name) -> Self {
        Self(snake_case(component.as_str()))
    }
}

impl Codec for SqlName {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(&self.0)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        Self::parse(&decoder.string()?)
    }
}

fn plural_snake_case(value: &str) -> String {
    let base = snake_case(value);
    let (prefix, last) = match base.rfind('_') {
        Some(at) => base.split_at(at + 1),
        None => ("", base.as_str()),
    };
    if let Some(plural) = irregular_plural(last) {
        return format!("{prefix}{plural}");
    }
    if base.ends_with("fe") {
        return format!("{}ves", &base[..base.len() - 2]);
    }
    if base.ends_with('f') && !base.ends_with("ff") {
        return format!("{}ves", &base[..base.len() - 1]);
    }
    if base.ends_with("ss")
        || base.ends_with('x')
        || base.ends_with('z')
        || base.ends_with("ch")
        || base.ends_with("sh")
    {
        format!("{base}es")
    } else if base.ends_with('s') {
        base
    } else if base.ends_with('y')
        && base
            .chars()
            .rev()
            .nth(1)
            .is_some_and(|before| !matches!(before, 'a' | 'e' | 'i' | 'o' | 'u'))
    {
        format!("{}ies", &base[..base.len() - 1])
    } else {
        format!("{base}s")
    }
}

fn snake_case(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::with_capacity(value.len() + 4);
    for (index, &character) in chars.iter().enumerate() {
        if character.is_uppercase() {
            let starts_run = index > 0 && !chars[index - 1].is_uppercase();
            let ends_run = index > 0
                && chars[index - 1].is_uppercase()
                && chars.get(index + 1).is_some_and(|next| next.is_lowercase());
            if starts_run || ends_run {
                out.push('_');
            }
            out.extend(character.to_lowercase());
        } else {
            out.push(character);
        }
    }
    out
}

fn irregular_plural(word: &str) -> Option<&'static str> {
    Some(match word {
        "person" => "people",
        "child" => "children",
        "man" => "men",
        "woman" => "women",
        "foot" => "feet",
        "tooth" => "teeth",
        "goose" => "geese",
        "mouse" => "mice",
        "equipment" => "equipment",
        "information" => "information",
        "money" => "money",
        "news" => "news",
        "series" => "series",
        "species" => "species",
        "staff" => "staff",
        "audio" => "audio",
        "metadata" => "metadata",
        "data" => "data",
        _ => return None,
    })
}
