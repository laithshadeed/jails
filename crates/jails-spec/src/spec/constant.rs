//! An enum constant as the CLI spells it: `NAME` or `NAME=wire`.

use jails_support::Result;
use jails_support::identity::Name;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstantSpec {
    pub name: Name,
    pub wire: Option<String>,
}

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

    pub fn canonical(&self) -> String {
        match &self.wire {
            Some(wire) => format!("{}={wire}", self.name),
            None => self.name.to_string(),
        }
    }
}
