//! The HTTP surface a recipe was asked for.
//!
//! Its own module rather than three lines in `spring.rs`, on that file's own
//! rule: a value with a secret of its own goes beside the file, not in it.

/// The HTTP surface a recipe was asked for: where the route is, and how the
/// request arrives.
///
/// One value because the two are computed together and consumed together, and
/// because they are the two halves of the same question -- an endpoint whose
/// URL is a fixed external contract almost always has a fixed request format
/// too. `missing.md` M8 is the first half and M15 the second, and they were
/// found in the same afternoon on the same project.
///
/// It also keeps the parameter count down: `usecase_files` already took six,
/// and a seventh positional `Option`/enum pair beside `Option<&str>` is the
/// Data Clump `abstract.md` §4 names.
#[derive(Clone, Copy)]
pub(crate) struct Endpoint<'a> {
    /// The route the caller named, or `None` for the derived one.
    pub route: Option<&'a str>,
    /// How the request body is bound.
    pub consumes: jails_spec::spec::kind::WireFormat,
}

impl Endpoint<'_> {
    /// The default surface: a derived route, reading JSON.
    ///
    /// What every endpoint jails wrote before `--consumes` existed, and what
    /// a test that is about the SQL rather than the wire asks for -- which is
    /// every test in `query.rs` and `workflow.rs`, hence `cfg(test)`.
    #[cfg(test)]
    pub fn json() -> Self {
        Self {
            route: None,
            consumes: jails_spec::spec::kind::WireFormat::Json,
        }
    }

    /// Spring's annotation for binding this request into one parameter.
    pub fn binding(&self) -> &'static str {
        self.consumes.binding()
    }

    /// The import that annotation costs.
    pub fn binding_import(&self) -> &'static str {
        self.consumes.binding_import()
    }

    /// The wire naming a record bound by this endpoint has to answer to.
    ///
    /// `None` for JSON, and not because JSON has no naming -- it does, and
    /// Jackson applies the project's strategy to it without help. This is
    /// only for the *data binder*, which has none.
    pub fn binding_naming(
        &self,
        project: &crate::model::Project,
    ) -> Option<jails_project::model::WireNaming> {
        match self.consumes {
            jails_spec::spec::kind::WireFormat::Form => Some(project.wire_naming()),
            jails_spec::spec::kind::WireFormat::Json => None,
        }
    }
}
