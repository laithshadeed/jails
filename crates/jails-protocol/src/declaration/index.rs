//! What an index is: composite, ordered, and checked against the fields.
//!
//! Separate from the field syntax because a per-column `@index` marker and a
//! declared `--index` are different statements. The marker is part of one
//! component and says nothing about order; this names several components in an
//! order that is semantic and is never sorted.

use super::FieldSpec;
use crate::Result;
use crate::identity::Name;
use jails_support::codec::{Codec, Decoder, Encoder};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum IndexDirection {
    Ascending,
    Descending,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct IndexColumn {
    pub field: Name,
    pub direction: IndexDirection,
}

/// A composite or ordered index, which a per-column `@index` marker cannot say.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct IndexSpec {
    /// Order is semantic and is never sorted: a composite index on `(a, b)` is
    /// not the index on `(b, a)`.
    pub columns: Vec<IndexColumn>,
}

impl IndexSpec {
    /// `created_at desc, title` against the fields actually declared.
    ///
    /// This replaces a pass-through tail that persisted whatever followed a
    /// column name as trusted generated SQL. Only `asc` and `desc` are
    /// accepted, and an unknown column refuses here rather than at
    /// `flyway migrate`.
    pub fn parse(token: &str, fields: &[FieldSpec]) -> Result<Self> {
        let mut columns = Vec::new();
        for part in token.split(',') {
            let part = part.trim();
            if part.is_empty() {
                return Err(format!("index `{token}` has an empty column"));
            }
            let mut words = part.split_whitespace();
            let field = words.next().expect("a non-empty part has a first word");
            let direction = match words.next() {
                None | Some("asc") => IndexDirection::Ascending,
                Some("desc") => IndexDirection::Descending,
                Some(other) => {
                    return Err(format!(
                        "`{other}` follows the index column `{field}`, and only `asc` or `desc` \
                         may.\n       fix: arbitrary SQL is refused here rather than recorded as \
                         trusted generated SQL."
                    ));
                }
            };
            if let Some(trailing) = words.next() {
                return Err(format!(
                    "`{trailing}` follows the index column `{field}` and its direction"
                ));
            }
            let field = Name::parse(field)?;
            if !fields.iter().any(|declared| declared.name == field) {
                return Err(format!(
                    "index column `{field}` is not a declared field.\n       fix: index one of: \
                     {}.",
                    fields
                        .iter()
                        .map(|f| f.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if columns
                .iter()
                .any(|existing: &IndexColumn| existing.field == field)
            {
                return Err(format!("index column `{field}` is repeated"));
            }
            columns.push(IndexColumn { field, direction });
        }
        if columns.is_empty() {
            return Err("an index needs at least one column".to_string());
        }
        Ok(Self { columns })
    }

    pub fn canonical(&self) -> String {
        self.columns
            .iter()
            .map(|column| match column.direction {
                IndexDirection::Ascending => column.field.to_string(),
                IndexDirection::Descending => format!("{} desc", column.field),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}
impl Codec for IndexSpec {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.count(self.columns.len())?;
        for column in &self.columns {
            column.field.encode(encoder)?;
            encoder.tag(match column.direction {
                IndexDirection::Ascending => 0,
                IndexDirection::Descending => 1,
            });
        }
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        let count = decoder.count()?;
        let mut columns = Vec::new();
        for _ in 0..count {
            let field = Name::decode(decoder)?;
            let direction = match decoder.tag()? {
                0 => IndexDirection::Ascending,
                1 => IndexDirection::Descending,
                other => return Err(format!("unknown index direction tag {other}")),
            };
            columns.push(IndexColumn { field, direction });
        }
        if columns.is_empty() {
            return Err("an index needs at least one column".to_string());
        }
        Ok(Self { columns })
    }
}
