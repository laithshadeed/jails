//! Reader-authored replacements for jails' own Java templates.
//!
//! **A captured external fact, not a filesystem read.** A reader who wants
//! the generated code to *look* different -- not a new generator, just this
//! class shaped differently -- puts a file at `.jails/templates/<name>`
//! (project) or `~/.config/jails/templates/<name>` (machine), and it replaces
//! the built-in of the same name in that order. The compiler may not observe
//! the filesystem, so capture reads them once and hands them over here.
//!
//! **An overridden template is not golden-tested.** That is the honest cost,
//! and it is why `doctor` reports every active override by name: jails names
//! what it did not write before the reader finds out from a failing build.
//!
//! Overrides are held to the built-ins' contract: the placeholder set must
//! match exactly. A template missing a key the generator supplies, or using
//! one it does not, is an error naming *their* file -- the built-in's
//! placeholders are the interface, and a mismatch is the reader's typo rather
//! than jails' bug.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Every override this workspace offers, keyed the way the built-ins are: by
/// path relative to `templates/`, with `/` separators on every platform.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemplateOverrides {
    pub files: BTreeMap<String, TemplateOverride>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemplateOverride {
    /// Where it came from, so a refusal can name the file the reader edited.
    pub origin: String,
    pub text: String,
}

impl TemplateOverrides {
    /// The template to render `name` with: the reader's, or jails' own.
    ///
    /// The built-in is the contract. An override that does not honour it is
    /// refused with both halves of the disagreement, because "these
    /// placeholders differ" without saying which sends the reader diffing two
    /// files by eye.
    pub fn resolve<'a>(&'a self, name: &str, built_in: &'a str) -> Result<&'a str, String> {
        let Some(candidate) = self.files.get(name) else {
            return Ok(built_in);
        };
        let expected = placeholders(built_in);
        let found = placeholders(&candidate.text);
        let missing = expected
            .iter()
            .filter(|key| !found.contains(key))
            .copied()
            .collect::<Vec<_>>();
        let unknown = found
            .iter()
            .filter(|key| !expected.contains(key))
            .copied()
            .collect::<Vec<_>>();
        if missing.is_empty() && unknown.is_empty() {
            return Ok(&candidate.text);
        }
        Err(format!(
            "template override {} does not match the built-in `{name}`\n       missing: [{}]; not supplied by jails: [{}]\n       fix: the placeholders are the contract -- copy jails' own templates/{name} and edit around them",
            candidate.origin,
            missing.join(", "),
            unknown.join(", ")
        ))
    }

    /// Every override in play, for `doctor` to name.
    pub fn origins(&self) -> impl Iterator<Item = (&str, &str)> {
        self.files
            .iter()
            .map(|(name, file)| (name.as_str(), file.origin.as_str()))
    }
}

/// The `{{key}}` set a template uses, in first-seen order.
fn placeholders(template: &str) -> Vec<&str> {
    let mut found: Vec<&str> = Vec::new();
    let mut rest = template;
    while let Some(at) = rest.find("{{") {
        let after = &rest[at + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let key = &after[..end];
        if !found.contains(&key) {
            found.push(key);
        }
        rest = &after[end + 2..];
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overrides(name: &str, text: &str) -> TemplateOverrides {
        TemplateOverrides {
            files: BTreeMap::from([(
                name.to_string(),
                TemplateOverride {
                    origin: format!(".jails/templates/{name}"),
                    text: text.to_string(),
                },
            )]),
        }
    }

    #[test]
    fn an_absent_override_resolves_to_the_built_in() {
        let overrides = TemplateOverrides::default();
        assert_eq!(
            overrides.resolve("a.java", "class {{name}} {}"),
            Ok("class {{name}} {}")
        );
    }

    #[test]
    fn the_same_placeholders_in_a_different_shape_are_accepted() {
        let overrides = overrides("a.java", "// mine\nclass {{name}} {}");
        assert_eq!(
            overrides.resolve("a.java", "class {{name}} {}"),
            Ok("// mine\nclass {{name}} {}")
        );
    }

    #[test]
    fn a_dropped_placeholder_names_the_file_and_the_key() {
        let overrides = overrides("a.java", "class Whatever {}");
        let refusal = overrides
            .resolve("a.java", "class {{name}} {}")
            .expect_err("a dropped placeholder is refused");
        assert!(refusal.contains(".jails/templates/a.java"), "{refusal}");
        assert!(refusal.contains("missing: [name]"), "{refusal}");
    }

    #[test]
    fn a_key_jails_does_not_supply_is_refused_too() {
        let overrides = overrides("a.java", "class {{name}} { {{invented}} }");
        let refusal = overrides
            .resolve("a.java", "class {{name}} {}")
            .expect_err("an invented placeholder is refused");
        assert!(
            refusal.contains("not supplied by jails: [invented]"),
            "{refusal}"
        );
    }
}
