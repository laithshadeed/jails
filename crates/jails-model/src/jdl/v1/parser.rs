//! JDL v1 to `source::Document`, plus the CST the same pass builds.
//!
//! **Two outputs from one walk, on purpose.** The `source::Document` is what
//! the linker turns into an `AppModel`; the `DocumentCst` is what the edit
//! functions splice into. Parsing twice would let the two disagree about where
//! a declaration begins, and an edit that splices at the wrong boundary
//! corrupts a file the reader owns.
//!
//! No resolution happens here — names stay strings and every reference is
//! checked by the linker — so that a document with several bad references
//! reports all of them at once instead of stopping at the first.
//!
//! Split by declaration shape: `declaration.rs` for the app block and the
//! top-level statements, `component.rs`, `operation.rs`, `projection.rs` and
//! `relation.rs` for the rest.

use super::cst::{DeclarationCst, DocumentCst, MemberCst};
use super::token::{Span, Token, TokenKind, problem};
use crate::source;
use crate::{DependencyScope, Diagnostics, Facet, SettingTarget};
use std::collections::{BTreeMap, BTreeSet};
mod attribute;
mod component;
mod declaration;
mod operation;
mod projection;
mod relation;

use attribute::{
    decode_argument, field_scope, flag_attribute, has_attribute, length, one_arg, one_raw_arg,
    reject_unknown_attributes, set_once, stable_fragment,
};

pub(super) struct ParsedDocument {
    pub cst: DocumentCst,
    pub source: source::Document,
}

#[derive(Default)]
struct AppDraft {
    name: String,
    id: Option<String>,
    package: Option<String>,
    java: Option<u16>,
    platform: Option<String>,
    build: Option<String>,
    storage: Option<String>,
}

struct EntityDraft {
    name: String,
    id: String,
    active: bool,
    /// The `@package(...)` override, relative to the base package.
    package: Option<String>,
    table: Option<String>,
    facets: BTreeSet<Facet>,
    fields: BTreeMap<String, source::Field>,
    /// Field labels in the order the block declared them.
    field_order: Vec<String>,
    indexes: BTreeMap<String, source::Index>,
    constraints: Vec<source::EntityConstraint>,
    relations: BTreeMap<String, source::Relation>,
    projections: Vec<source::Projection>,
}

#[derive(Clone, Debug)]
struct Attribute {
    name: String,
    args: Vec<String>,
    raw_args: Vec<String>,
    parenthesized: bool,
}

pub(super) fn parse(input: &str, tokens: Vec<Token>) -> Result<ParsedDocument, Diagnostics> {
    Parser::new(input, tokens).document()
}

struct Parser<'a> {
    input: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
    declarations: Vec<DeclarationCst>,
    members: Vec<MemberCst>,
    app: Option<AppDraft>,
    capabilities: BTreeMap<String, source::Capability>,
    dependencies: BTreeMap<String, source::Dependency>,
    settings: BTreeMap<String, source::Setting>,
    ejections: BTreeMap<String, source::Ejection>,
    entities: BTreeMap<String, source::Entity>,
    operations: BTreeMap<String, source::Operation>,
    components: BTreeMap<String, source::Component>,
    projection_rules: Vec<source::ProjectionRule>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str, tokens: Vec<Token>) -> Self {
        Self {
            input,
            tokens,
            cursor: 0,
            declarations: Vec::new(),
            members: Vec::new(),
            app: None,
            capabilities: BTreeMap::new(),
            dependencies: BTreeMap::new(),
            settings: BTreeMap::new(),
            ejections: BTreeMap::new(),
            entities: BTreeMap::new(),
            operations: BTreeMap::new(),
            components: BTreeMap::new(),
            projection_rules: Vec::new(),
        }
    }

    fn document(mut self) -> Result<ParsedDocument, Diagnostics> {
        self.skip_trivia();
        self.expect("jdl", "JDL0001", "the first declaration must be `jdl 1`")?;
        let version = self.take_integer()?;
        if version != "1" {
            return Err(self.here(
                "JDL0001",
                format!("unsupported JDL language version `{version}`"),
                "use `jdl 1` or run an explicit source migration",
            ));
        }
        self.end_line()?;
        self.skip_layout();
        if !self.at("app") {
            return Err(self.here(
                "JDL0100",
                "JDL v1 requires one app block after the version declaration",
                "add `app Name { ... }` with pkg, java, platform, build, and storage",
            ));
        }
        self.parse_app()?;

        loop {
            self.skip_layout();
            if self.kind() == TokenKind::Eof {
                break;
            }
            match self.text() {
                "cap" => self.parse_cap()?,
                "dep" => self.parse_dep()?,
                "prop" => self.parse_prop()?,
                "enum" => self.parse_enum()?,
                "entity" => self.parse_entity()?,
                "eject" => self.parse_eject()?,
                "use" => self.parse_top_level_use()?,
                "command" | "query" | "transition" | "event" => self.parse_operation(None)?,
                "component" => self.parse_component()?,
                unknown => {
                    return Err(self.here(
                        "JDL0101",
                        format!("unknown top-level declaration `{unknown}`"),
                        "use cap, dep, prop, enum, entity, event, component, use, or eject",
                    ));
                }
            }
        }

        let app = self.app.take().expect("app was parsed");
        let package = app.package.ok_or_else(|| {
            self.here(
                "JDL0201",
                "the app has no pkg property",
                "add `pkg com.example.app`",
            )
        })?;
        let java = app.java.ok_or_else(|| {
            self.here(
                "JDL0202",
                "the app has no java property",
                "add `java 21` or newer",
            )
        })?;
        let platform = app.platform.ok_or_else(|| {
            self.here(
                "JDL0203",
                "the app has no platform property",
                "add `platform spring` or `platform plain`",
            )
        })?;
        let build = app.build.ok_or_else(|| {
            self.here(
                "JDL0204",
                "the app has no build property",
                "add `build maven` or `build gradle`",
            )
        })?;
        let storage = app.storage.ok_or_else(|| {
            self.here(
                "JDL0205",
                "the app has no storage property",
                "add `storage postgres`, `h2`, `sqlite`, or `none`",
            )
        })?;
        debug_assert!(matches!(platform.as_str(), "spring" | "plain"));
        debug_assert!(matches!(build.as_str(), "maven" | "gradle"));
        let dialect = match storage.as_str() {
            "postgres" => "postgresql",
            "h2" => "h2",
            "sqlite" => "sqlite",
            "none" => "none",
            _ => unreachable!("storage was checked while parsing"),
        };
        let mut capabilities = self.capabilities;
        if let Some(kind) = match storage.as_str() {
            "postgres" => Some("db"),
            "h2" => Some("h2"),
            "sqlite" => Some("sqlite"),
            "none" => None,
            _ => unreachable!("storage was checked while parsing"),
        } && !capabilities
            .values()
            .any(|capability| capability.kind == kind)
        {
            capabilities.insert(
                kind.to_string(),
                source::Capability {
                    id: format!("cap_{kind}"),
                    kind: kind.to_string(),
                    name: None,
                    package: None,
                },
            );
        }
        let project_label = stable_fragment(&app.name);
        let source = source::Document {
            schema: "jails.model.v1".to_string(),
            project: source::Project {
                id: app.id.unwrap_or_else(|| format!("app_{project_label}")),
                name: app.name,
                base_package: package,
                java_release: java,
                dialect: dialect.to_string(),
                platform,
                build,
            },
            capabilities,
            dependencies: self.dependencies,
            settings: self.settings,
            ejections: self.ejections,
            units: BTreeMap::new(),
            components: self.components,
            entities: self.entities,
            operations: self.operations,
            projection_rules: self.projection_rules,
        };
        let cst = DocumentCst::new(
            self.input.to_string(),
            self.tokens,
            self.declarations,
            self.members,
        );
        Ok(ParsedDocument { cst, source })
    }

    fn parse_type_ref(&mut self) -> Result<String, Diagnostics> {
        let atom = self.take_word("field type")?;
        if matches!(atom.as_str(), "list" | "map") && self.consume("<") {
            let mut result = format!("{atom}<");
            let mut depth = 1_u32;
            while depth > 0 {
                if self.kind() == TokenKind::Eof || self.kind() == TokenKind::Newline {
                    return Err(self.here(
                        "JDL0511",
                        "a collection type is not closed",
                        "close the type with `>`",
                    ));
                }
                let text = self.text().to_string();
                self.bump_raw();
                if text == "<" {
                    depth += 1;
                } else if text == ">" {
                    depth -= 1;
                }
                result.push_str(&text);
            }
            Ok(result)
        } else {
            Ok(atom)
        }
    }

    fn field_list(&mut self) -> Result<Vec<String>, Diagnostics> {
        self.expect("[", "JDL0520", "a constraint needs a bracketed field list")?;
        let mut columns = Vec::new();
        loop {
            let field = self.take_word("constraint field")?;
            let direction = if self.at("asc") || self.at("desc") {
                let direction = self.text().to_string();
                self.bump();
                format!(" {direction}")
            } else {
                String::new()
            };
            columns.push(format!("{field}{direction}"));
            if self.consume("]") {
                break;
            }
            self.expect(",", "JDL0520", "separate constraint fields with `,`")?;
        }
        Ok(columns)
    }

    /// A declaration's attribute list, checked against the closed set it
    /// accepts, with its `@id` resolved.
    ///
    /// Every declaration head does these three things and they are decided
    /// together: what it may carry, and what its identity is when the source
    /// leaves `@id` unsaid. The default is a closure so a label computed
    /// from the head is not built for a declaration that pinned its id.
    fn declared(
        &mut self,
        allowed: &[&str],
        default_id: impl FnOnce() -> String,
    ) -> Result<(Vec<Attribute>, String), Diagnostics> {
        let attributes = self.attributes()?;
        reject_unknown_attributes(&attributes, allowed, self)?;
        let id = one_arg(&attributes, "id")?.unwrap_or_else(default_id);
        Ok((attributes, id))
    }

    fn attributes(&mut self) -> Result<Vec<Attribute>, Diagnostics> {
        let mut attributes = Vec::new();
        while self.consume("@") {
            let name = self.take_word("attribute name")?;
            let mut args = Vec::new();
            let mut raw_args = Vec::new();
            let parenthesized = self.consume("(");
            if parenthesized {
                let mut current = String::new();
                let mut depth = 0_u32;
                loop {
                    if self.kind() == TokenKind::Eof || self.kind() == TokenKind::Newline {
                        return Err(self.here(
                            "JDL0110",
                            format!("attribute `@{name}` is not closed"),
                            "close the attribute with `)`",
                        ));
                    }
                    if self.at(")") && depth == 0 {
                        self.bump();
                        if !current.trim().is_empty() {
                            let raw = current.trim().to_string();
                            args.push(decode_argument(&raw)?);
                            raw_args.push(raw);
                        }
                        break;
                    }
                    if self.at(",") && depth == 0 {
                        self.bump();
                        let raw = current.trim().to_string();
                        args.push(decode_argument(&raw)?);
                        raw_args.push(raw);
                        current.clear();
                        continue;
                    }
                    let text = self.text().to_string();
                    self.bump_raw();
                    if matches!(text.as_str(), "(" | "[") {
                        depth += 1;
                    } else if matches!(text.as_str(), ")" | "]") {
                        depth = depth.saturating_sub(1);
                    }
                    current.push_str(&text);
                }
            }
            attributes.push(Attribute {
                name,
                args,
                raw_args,
                parenthesized,
            });
        }
        Ok(attributes)
    }

    fn take_value(&mut self, description: &str) -> Result<String, Diagnostics> {
        match self.kind() {
            TokenKind::String => self.take_string(description),
            TokenKind::Word | TokenKind::Integer => {
                let value = self.text().to_string();
                self.bump();
                Ok(value)
            }
            _ => Err(self.here(
                "JDL0112",
                format!("expected {description}"),
                "provide a string, integer, decimal, boolean, or enum constant",
            )),
        }
    }

    fn take_string(&mut self, description: &str) -> Result<String, Diagnostics> {
        if self.kind() != TokenKind::String {
            return Err(self.here(
                "JDL0113",
                format!("expected quoted {description}"),
                "use a JSON-style double-quoted string",
            ));
        }
        let encoded = self.text().to_string();
        self.bump();
        serde_json::from_str(&encoded).map_err(|error| {
            self.here(
                "JDL0004",
                format!("invalid string: {error}"),
                "use JSON string escapes",
            )
        })
    }

    fn take_word(&mut self, description: &str) -> Result<String, Diagnostics> {
        if self.kind() != TokenKind::Word {
            return Err(self.here(
                "JDL0102",
                format!("expected {description}"),
                format!("provide a valid {description}"),
            ));
        }
        let value = self.text().to_string();
        self.bump();
        Ok(value)
    }

    fn take_integer(&mut self) -> Result<String, Diagnostics> {
        if self.kind() != TokenKind::Integer {
            return Err(self.here(
                "JDL0103",
                "expected an integer",
                "provide a base-ten integer without a leading plus sign",
            ));
        }
        let value = self.text().to_string();
        self.bump();
        Ok(value)
    }

    fn end_line(&mut self) -> Result<(), Diagnostics> {
        self.skip_trivia();
        if self.kind() != TokenKind::Newline {
            return Err(self.here(
                "JDL0104",
                "expected the end of a JDL declaration",
                "remove extra tokens; semicolons are not legal in JDL v1",
            ));
        }
        self.cursor += 1;
        Ok(())
    }

    fn expect(
        &mut self,
        expected: &str,
        code: &'static str,
        message: impl Into<String>,
    ) -> Result<(), Diagnostics> {
        if !self.consume(expected) {
            return Err(self.here(code, message, format!("add `{expected}` here")));
        }
        Ok(())
    }

    fn consume(&mut self, expected: &str) -> bool {
        self.skip_trivia();
        if self.text() == expected {
            self.cursor += 1;
            self.skip_trivia();
            true
        } else {
            false
        }
    }

    fn at(&mut self, expected: &str) -> bool {
        self.skip_trivia();
        self.text() == expected
    }

    fn bump(&mut self) {
        self.cursor += 1;
        self.skip_trivia();
    }

    fn bump_raw(&mut self) {
        self.cursor += 1;
    }

    fn skip_trivia(&mut self) {
        while self.tokens.get(self.cursor).is_some_and(Token::is_trivia) {
            self.cursor += 1;
        }
    }

    fn skip_layout(&mut self) {
        loop {
            self.skip_trivia();
            if self.kind() == TokenKind::Newline {
                self.cursor += 1;
            } else {
                break;
            }
        }
        self.skip_trivia();
    }

    fn text(&self) -> &str {
        self.tokens
            .get(self.cursor)
            .map_or("", |token| token.text(self.input))
    }

    fn kind(&self) -> TokenKind {
        self.tokens
            .get(self.cursor)
            .map_or(TokenKind::Eof, |token| token.kind)
    }

    fn span(&self) -> Span {
        self.tokens
            .get(self.cursor)
            .map_or(Span::new(self.input.len(), self.input.len()), |token| {
                token.span
            })
    }

    fn previous_end(&self) -> usize {
        self.tokens
            .get(self.cursor.saturating_sub(1))
            .map_or(0, |token| token.span.end)
    }

    fn declaration(&mut self, kind: &str, name: Option<String>, start: usize, end: usize) {
        self.declarations.push(DeclarationCst {
            kind: kind.to_string(),
            name,
            span: Span::new(start, end),
        });
    }

    fn member(&mut self, owner: &str, kind: &str, name: Option<String>, start: usize, end: usize) {
        let start = self.input[..start]
            .rfind('\n')
            .map_or(0, |newline| newline + 1);
        self.members.push(MemberCst {
            owner: owner.to_string(),
            kind: kind.to_string(),
            name,
            span: Span::new(start, end),
        });
    }

    fn here(
        &self,
        code: &'static str,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Diagnostics {
        problem(self.input, self.span().start, code, message, fix)
    }
}
