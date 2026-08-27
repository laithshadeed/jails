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
    /// The verb this endpoint answers, where the recipe has a choice.
    ///
    /// `transition` is the one that does: its update is idempotent, so PUT and
    /// PATCH are both correct spellings of "set these fields on this row", and
    /// a frontend calling one will not accept the other. Every other recipe
    /// here either derives its verb from the request or has exactly one.
    pub method: jails_spec::spec::kind::HttpMethod,
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
            method: jails_spec::spec::kind::HttpMethod::Put,
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

    /// How a generated `MockMvcTester` test *sends* this request.
    ///
    /// It has to be how the controller reads it, and it was not: every
    /// `--consumes form` endpoint jails wrote shipped a proof that posted a
    /// JSON body at an `@ModelAttribute` parameter. The data binder reads
    /// request *parameters*, so every component arrived null and the request
    /// was answered 400 -- and on a transition the second generated test
    /// asserted 400, so it passed for exactly the wrong reason.
    ///
    /// One owner for the same reason `Endpoint` has one: the binding and the
    /// thing that has to match it are the same fact, and three renderers were
    /// deriving it separately. `bugs.md` B48 is that shape.
    ///
    /// `values` is `(component, JSON sample)` -- the sample as it would be
    /// written in a JSON body, quotes and all, because that is what the
    /// callers already have. A form parameter is text, so the quotes come off.
    pub fn request(
        &self,
        project: &crate::model::Project,
        values: &[(String, String)],
        indent: &str,
    ) -> String {
        match self.consumes {
            jails_spec::spec::kind::WireFormat::Json => {
                let body = values
                    .iter()
                    .map(|(name, sample)| format!("  \"{name}\": {sample}"))
                    .collect::<Vec<_>>()
                    .join(",\n");
                format!(
                    "{indent}.contentType(MediaType.APPLICATION_JSON)\n{indent}.content(\"\"\"\n{{\n{body}\n}}\n\"\"\")"
                )
            }
            // The *bound* name, not the component name: a snake-cased project
            // gives each component `@BindParam("user_id")`, and a test posting
            // `userId` would then bind nothing. The two come from one place.
            jails_spec::spec::kind::WireFormat::Form => values
                .iter()
                .map(|(name, sample)| {
                    let bound = match project.wire_naming() {
                        jails_project::model::WireNaming::AsWritten => name.clone(),
                        jails_project::model::WireNaming::SnakeCase => crate::sql::snake_case(name),
                    };
                    format!(
                        "{indent}.param(\"{bound}\", \"{}\")",
                        sample.trim_matches('"')
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    /// The `MediaType` import [`Endpoint::request`] costs, which is none for a
    /// form post.
    pub fn media_type_import(&self) -> &'static str {
        match self.consumes {
            jails_spec::spec::kind::WireFormat::Json => {
                "import org.springframework.http.MediaType;\n"
            }
            jails_spec::spec::kind::WireFormat::Form => "",
        }
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
