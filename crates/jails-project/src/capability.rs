//! A capability row of `jails.toml`: the kind, and the name and package it
//! was given, if any.

use jails_model::CapabilityKind;
use jails_support::Result;
use jails_support::identity::{Name, Package};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Declaration {
    pub kind: CapabilityKind,
    pub name: Option<String>,
    pub package: Option<String>,
}

impl Declaration {
    pub fn plain(kind: CapabilityKind) -> Self {
        Self {
            kind,
            name: None,
            package: None,
        }
    }

    pub fn asked(kind: CapabilityKind, name: Option<&str>, package: Option<&str>) -> Self {
        Self {
            kind,
            name: name.map(str::to_string),
            package: package.map(str::to_string),
        }
    }

    /// The name is a Java identifier and the package a Java package, and each
    /// is accepted only by a capability it means something to: `csv`, `sqlite`,
    /// `json` and `http` come in named instances; the Spring packs are one per
    /// project but placed; the rest write conventional or project-global
    /// output and take neither.
    pub fn validate(&self) -> Result<()> {
        use CapabilityKind::*;
        self.name.as_deref().map(Name::parse).transpose()?;
        self.package.as_deref().map(Package::parse).transpose()?;
        match self.kind {
            Csv | Sqlite | Json | Http => Ok(()),
            Api | Actuator | Cache | Security | Cors | Sse | Mail | Redis | Observability => {
                if self.name.is_some() {
                    return Err(format!(
                        "`{}` is one per project, so `--name` has no meaning for it.\n       \
                         fix: drop `--name`; `--package` does move where it is placed.",
                        self.kind.label()
                    )
                    .into());
                }
                Ok(())
            }
            Db | Kafka | Testkit | Fake | Format | Coverage | Loadtest | Ci | Docker | K8s
            | Toxiproxy | H2 | FastTest => {
                if let Some(rejected) = self
                    .name
                    .as_ref()
                    .map(|_| "--name")
                    .or(self.package.as_ref().map(|_| "--package"))
                {
                    return Err(format!(
                        "`{}` writes project-global or conventional output, so `{rejected}` has \
                         no meaning for it.\n       fix: drop `{rejected}`.",
                        self.kind.label()
                    )
                    .into());
                }
                Ok(())
            }
        }
    }

    pub fn display(&self) -> String {
        let mut out = self.kind.label().to_string();
        if let Some(name) = &self.name {
            out.push_str(&format!(" --name {name}"));
        }
        if let Some(package) = &self.package {
            out.push_str(&format!(" --package '{package}'"));
        }
        out
    }
}
