//! A built-in template, and the name a reader overrides it by.
//!
//! **The name travels with the text**, because resolving an override needs
//! both and a bare `&'static str` carries only one. Every template is
//! declared through [`template!`], so a kind cannot acquire one the reader has
//! no way to replace -- the type is what makes that structural rather than a
//! rule somebody has to remember.

use crate::Diagnostic;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Template {
    /// Path under `templates/`, which is exactly how an override is named.
    pub name: &'static str,
    pub built_in: &'static str,
}

impl Template {
    /// The text to render with: the reader's override, or jails' own.
    pub fn resolve<'a>(
        self,
        overrides: &'a jails_contracts::TemplateOverrides,
    ) -> Result<&'a str, Diagnostic>
    where
        Self: 'a,
    {
        overrides
            .resolve(self.name, self.built_in)
            .map_err(|(message, fix)| {
                Diagnostic::new(
                    "compile-template-override-mismatch",
                    self.name,
                    message,
                    fix,
                )
            })
    }
}

/// Declare a built-in template by its path under `templates/`.
macro_rules! template {
    ($path:literal) => {
        $crate::Template {
            name: $path,
            built_in: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../templates/",
                $path
            )),
        }
    };
}
pub(crate) use template;
