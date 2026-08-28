use super::cst::{DeclarationCst, DocumentCst};
use super::token::{Span, Token, TokenKind, problem};
use crate::source;
use crate::{DependencyScope, Diagnostics, Facet, SettingTarget};
use std::collections::{BTreeMap, BTreeSet};
mod component;
mod declaration;
mod operation;

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
    table: Option<String>,
    facets: BTreeSet<Facet>,
    fields: BTreeMap<String, source::Field>,
    indexes: BTreeMap<String, source::Index>,
}

#[derive(Clone, Debug)]
struct Attribute {
    name: String,
    args: Vec<String>,
}

pub(super) fn parse(input: &str, tokens: Vec<Token>) -> Result<ParsedDocument, Diagnostics> {
    Parser::new(input, tokens).document()
}

struct Parser<'a> {
    input: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
    declarations: Vec<DeclarationCst>,
    app: Option<AppDraft>,
    capabilities: BTreeMap<String, source::Capability>,
    dependencies: BTreeMap<String, source::Dependency>,
    settings: BTreeMap<String, source::Setting>,
    ejections: BTreeMap<String, source::Ejection>,
    entities: BTreeMap<String, source::Entity>,
    operations: BTreeMap<String, source::Operation>,
    components: BTreeMap<String, source::Component>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str, tokens: Vec<Token>) -> Self {
        Self {
            input,
            tokens,
            cursor: 0,
            declarations: Vec::new(),
            app: None,
            capabilities: BTreeMap::new(),
            dependencies: BTreeMap::new(),
            settings: BTreeMap::new(),
            ejections: BTreeMap::new(),
            entities: BTreeMap::new(),
            operations: BTreeMap::new(),
            components: BTreeMap::new(),
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
                "use" => {
                    return Err(self.here(
                        "JDL0701",
                        "top-level projection selectors are not linked in this implementation slice",
                        "move the `use` declaration inside its entity for now",
                    ));
                }
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
        let project_label = stable_fragment(&app.name);
        let source = source::Document {
            schema: "jails.model.v1".to_string(),
            project: source::Project {
                id: app.id.unwrap_or_else(|| format!("app_{project_label}")),
                name: app.name,
                base_package: package,
                java_release: java,
                dialect: dialect.to_string(),
            },
            capabilities: self.capabilities,
            dependencies: self.dependencies,
            settings: self.settings,
            ejections: self.ejections,
            units: BTreeMap::new(),
            components: self.components,
            entities: self.entities,
            operations: self.operations,
        };
        let cst = DocumentCst::new(self.input.to_string(), self.tokens, self.declarations);
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

    fn attributes(&mut self) -> Result<Vec<Attribute>, Diagnostics> {
        let mut attributes = Vec::new();
        while self.consume("@") {
            let name = self.take_word("attribute name")?;
            let mut args = Vec::new();
            if self.consume("(") {
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
                            args.push(decode_argument(current.trim())?);
                        }
                        break;
                    }
                    if self.at(",") && depth == 0 {
                        self.bump();
                        args.push(decode_argument(current.trim())?);
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
            attributes.push(Attribute { name, args });
        }
        Ok(attributes)
    }

    fn skip_balanced(&mut self, closing: &str) -> Result<(), Diagnostics> {
        let mut depth = 0_u32;
        loop {
            if self.kind() == TokenKind::Eof {
                return Err(self.here(
                    "JDL0111",
                    format!("missing closing `{closing}`"),
                    format!("add `{closing}`"),
                ));
            }
            if self.at(closing) && depth == 0 {
                self.bump();
                return Ok(());
            }
            if self.at("(") {
                depth += 1;
            } else if self.at(")") {
                depth = depth.saturating_sub(1);
            }
            self.bump_raw();
        }
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

    fn here(
        &self,
        code: &'static str,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Diagnostics {
        problem(self.input, self.span().start, code, message, fix)
    }
}

fn set_once<T>(
    slot: &mut Option<T>,
    value: T,
    name: &str,
    parser: &Parser<'_>,
) -> Result<(), Diagnostics> {
    if slot.replace(value).is_some() {
        return Err(parser.here(
            "JDL0211",
            format!("app property `{name}` is declared more than once"),
            format!("keep one `{name}` property"),
        ));
    }
    Ok(())
}

fn reject_unknown_attributes(
    attributes: &[Attribute],
    allowed: &[&str],
    parser: &Parser<'_>,
) -> Result<(), Diagnostics> {
    if let Some(attribute) = attributes
        .iter()
        .find(|attribute| !allowed.contains(&attribute.name.as_str()))
    {
        return Err(parser.here(
            "JDL0114",
            format!("attribute `@{}` is not valid here", attribute.name),
            format!("use only {}", allowed.join(", ")),
        ));
    }
    Ok(())
}

fn one_arg(attributes: &[Attribute], name: &str) -> Result<Option<String>, Diagnostics> {
    let matches = attributes
        .iter()
        .filter(|attribute| attribute.name == name)
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(Diagnostics::jdl_syntax(
            1,
            format!("attribute `@{name}` is repeated"),
            "keep one attribute of each kind",
        ));
    }
    let Some(attribute) = matches.first() else {
        return Ok(None);
    };
    if attribute.args.len() != 1 {
        return Err(Diagnostics::jdl_syntax(
            1,
            format!("attribute `@{name}` needs exactly one argument"),
            format!("write `@{name}(value)`"),
        ));
    }
    Ok(Some(attribute.args[0].clone()))
}

fn has_attribute(attributes: &[Attribute], name: &str) -> bool {
    attributes.iter().any(|attribute| attribute.name == name)
}

fn length(
    attributes: &[Attribute],
    parser: &Parser<'_>,
) -> Result<(Option<u32>, Option<u32>), Diagnostics> {
    let Some(value) = one_arg(attributes, "length")? else {
        return Ok((None, None));
    };
    let Some((min, max)) = value.split_once("..") else {
        return Err(parser.here(
            "JDL0512",
            format!("`{value}` is not a length range"),
            "use `@length(1..200)`, `@length(..200)`, or `@length(1..)`",
        ));
    };
    let bound = |value: &str| {
        if value.is_empty() {
            Ok(None)
        } else {
            value.parse::<u32>().map(Some).map_err(|_| {
                parser.here(
                    "JDL0512",
                    format!("`{value}` is not a non-negative length bound"),
                    "use an unsigned integer bound",
                )
            })
        }
    };
    let result = (bound(min)?, bound(max)?);
    if result == (None, None) {
        return Err(parser.here(
            "JDL0512",
            "a length range needs at least one bound",
            "provide a minimum, a maximum, or both",
        ));
    }
    Ok(result)
}

fn decode_argument(value: &str) -> Result<String, Diagnostics> {
    if value.starts_with('"') {
        serde_json::from_str(value).map_err(|error| {
            Diagnostics::jdl_syntax(
                1,
                format!("invalid string argument: {error}"),
                "use a valid JSON-style string",
            )
        })
    } else {
        Ok(value.replace([' ', '\t', '\r', '\n'], ""))
    }
}

fn stable_fragment(value: &str) -> String {
    let mut output = String::new();
    let mut previous_was_separator = false;
    for (position, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if position > 0 && !previous_was_separator {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if !previous_was_separator && !output.is_empty() {
            output.push('_');
            previous_was_separator = true;
        }
    }
    output.trim_matches('_').to_string()
}
