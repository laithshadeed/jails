//! The one MockMvc dialect: which entry point a project's Spring Boot version
//! has, and how a request driven at a route and the status it answers with are
//! spelled in it.
//!
//! **Three emitters used to answer this separately.** The operation
//! controller's proof, a `g controller` unit's companion test and a scaffold's
//! HTTP facet test each decided the Boot threshold, chose an import set, and
//! wrote the request twice -- once fluent, once classic. Written three times
//! they drifted: the fluent spelling of "no `If-Match` applies
//! unconditionally" was emitted into classic tests as well, where
//! `MockMvcTester` is not imported and the file does not compile. One owner is
//! what makes that impossible rather than unlikely.

use std::collections::BTreeSet;

/// The Spring Boot major at which `MockMvcTester` can be relied on.
///
/// It arrived in Spring Framework 6.2, which is Boot 3.4 -- but the captured
/// version is read as a major and nothing finer, so the threshold is drawn at
/// 4 rather than guessed at 3-point-something. A Boot 3.4+ project therefore
/// gets the classic entry point it does not strictly need, which costs a
/// fluent chain; a Boot 2 project given the fluent one would not compile, and
/// the error would name a package rather than a version.
const MOCKMVC_TESTER_BOOT_MAJOR: u32 = 4;

/// Which MockMvc front end the generated test drives.
///
/// `MockMvcTester` is Spring's AssertJ front end and needs no
/// `throws Exception`; `perform(...)` has existed since Spring 3 and still
/// does in 7, so it is the fallback rather than the other way round -- the
/// shape that compiles everywhere is the one to reach for when the version
/// cannot be established.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Dialect {
    /// `MockMvcTester`, asserted through AssertJ.
    Fluent,
    /// `MockMvc.perform(...)`, asserted through `MockMvcResultMatchers`.
    Classic,
}

/// The status a driven request has to answer with.
///
/// A closed set rather than a code, because the two dialects spell a status
/// differently and only these four are ever asserted: an emitter that wants a
/// fifth adds it here, where both spellings are written side by side.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Status {
    Ok,
    /// Any 2xx -- what a route whose handler the reader still has to write is
    /// held to, because the body is theirs and the code may not be 200.
    Successful,
    Created,
    /// What Spring answers for a path that is mapped and a method that is not.
    MethodNotAllowed,
}

impl Status {
    fn fluent(self) -> &'static str {
        match self {
            Self::Ok => "hasStatusOk()",
            Self::Successful => "hasStatus2xxSuccessful()",
            Self::Created => "hasStatus(201)",
            Self::MethodNotAllowed => "hasStatus(405)",
        }
    }

    fn classic(self) -> &'static str {
        match self {
            Self::Ok => "status().isOk()",
            Self::Successful => "status().is2xxSuccessful()",
            Self::Created => "status().isCreated()",
            Self::MethodNotAllowed => "status().isMethodNotAllowed()",
        }
    }
}

/// One request driven at a route, and what its answer must be.
pub(crate) struct Drive<'a> {
    /// The HTTP verb, lowercase: the method name in both dialects.
    pub(crate) verb: &'a str,
    pub(crate) uri: &'a str,
    /// URI template arguments, already spelled with their leading comma.
    pub(crate) uri_arguments: &'a str,
    /// The builder chain between the URI and the assertion -- `.param(...)`,
    /// `.contentType(...)`, `.content(...)`, `.header(...)`. Identical in both
    /// dialects, because `MockMvcTester`'s builder and
    /// `MockHttpServletRequestBuilder` declare the same methods; each line
    /// carries its own leading newline and indentation.
    pub(crate) extras: &'a str,
    pub(crate) status: Status,
    /// The exact response body, where the test knows it. A route whose handler
    /// the reader still has to write does not, and asserting a shape jails
    /// invented would test jails' guess.
    pub(crate) body_text: Option<&'a str>,
    /// What the statement is indented by.
    pub(crate) indent: &'a str,
}

impl Dialect {
    /// Which front end the captured Boot version has.
    pub(crate) fn of(spring_boot: Option<&str>) -> Self {
        match crate::emit_capability::boot_major(spring_boot)
            .is_some_and(|major| major >= MOCKMVC_TESTER_BOOT_MAJOR)
        {
            true => Self::Fluent,
            false => Self::Classic,
        }
    }

    /// The type a field holding the entry point declares, imported.
    pub(crate) fn tester(self, imports: &mut BTreeSet<String>) -> &'static str {
        match self {
            Self::Fluent => {
                imports.insert(
                    "org.springframework.test.web.servlet.assertj.MockMvcTester".to_string(),
                );
                "MockMvcTester"
            }
            Self::Classic => {
                imports.insert("org.springframework.test.web.servlet.MockMvc".to_string());
                "MockMvc"
            }
        }
    }

    /// What a test method driving this dialect declares.
    ///
    /// `perform` declares a checked exception, and that is the honest cost of
    /// the shape that compiles everywhere.
    pub(crate) fn throws(self) -> &'static str {
        match self {
            Self::Fluent => "",
            Self::Classic => " throws Exception",
        }
    }

    /// A standalone entry point over one already-constructed controller.
    ///
    /// Standalone rather than `@SpringBootTest` is a requirement rather than a
    /// preference: a context per operation controller in a project that also
    /// declared `db` would drag a container into Surefire for adapters that
    /// need no database.
    pub(crate) fn standalone(self, controller: &str, imports: &mut BTreeSet<String>) -> String {
        match self {
            Self::Fluent => {
                self.tester(imports);
                format!("MockMvcTester.of({controller})")
            }
            Self::Classic => {
                self.tester(imports);
                imports.insert(
                    "org.springframework.test.web.servlet.setup.MockMvcBuilders".to_string(),
                );
                format!("MockMvcBuilders.standaloneSetup({controller}).build()")
            }
        }
    }

    /// One request-and-status statement, terminated.
    pub(crate) fn drive(self, drive: &Drive<'_>, imports: &mut BTreeSet<String>) -> String {
        let Drive {
            verb,
            uri,
            uri_arguments,
            extras,
            status,
            body_text,
            indent,
        } = drive;
        match self {
            Self::Fluent => {
                imports.insert("static org.assertj.core.api.Assertions.assertThat".to_string());
                let body = body_text.map_or_else(String::new, |text| {
                    format!("\n{indent}        .bodyText()\n{indent}        .isEqualTo(\"{text}\")")
                });
                format!(
                    "{indent}assertThat(mvc.{verb}().uri(\"{uri}\"{uri_arguments}){extras})\n{indent}        .{}{body};",
                    status.fluent()
                )
            }
            Self::Classic => {
                imports.insert(format!(
                    "static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.{verb}"
                ));
                imports.insert(
                    "static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status"
                        .to_string(),
                );
                let body = body_text.map_or_else(String::new, |text| {
                    imports.insert(
                        "static org.springframework.test.web.servlet.result.MockMvcResultMatchers.content"
                            .to_string(),
                    );
                    format!("\n{indent}        .andExpect(content().string(\"{text}\"))")
                });
                format!(
                    "{indent}mvc.perform({verb}(\"{uri}\"{uri_arguments}){extras})\n{indent}        .andExpect({}){body};",
                    status.classic()
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug one owner exists to make impossible: before this module the
    /// "applies unconditionally" case was written in the fluent spelling and
    /// emitted into classic tests as well, where `MockMvcTester` is not
    /// imported and the file does not compile.
    #[test]
    fn every_statement_a_classic_test_emits_is_classic() {
        let mut imports = BTreeSet::new();
        let rendered = Dialect::Classic.drive(
            &Drive {
                verb: "put",
                uri: "/messages/{id}",
                uri_arguments: ", \"1\"",
                extras: "",
                status: Status::Ok,
                body_text: None,
                indent: "        ",
            },
            &mut imports,
        );
        assert!(rendered.contains("mvc.perform(put("), "{rendered}");
        assert!(!rendered.contains("assertThat("), "{rendered}");
        assert!(!imports.iter().any(|name| name.contains("MockMvcTester")));
    }

    #[test]
    fn the_boot_major_picks_the_entry_point() {
        assert_eq!(Dialect::of(Some("4.0.0")), Dialect::Fluent);
        assert_eq!(Dialect::of(Some("3.5.0")), Dialect::Classic);
        assert_eq!(Dialect::of(None), Dialect::Classic);
    }
}
