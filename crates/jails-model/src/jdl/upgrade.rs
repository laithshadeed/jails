//! `jails model upgrade --to 1`: the pre-v1 draft, translated into v1 source.
//!
//! `jdl-sol.md` §22 mandates this bridge and constrains it in one sentence that
//! shapes the whole module: the upgrader "preserves comments, source order,
//! explicit IDs, logical names, physical names, and operation routes", and
//! where a legacy construct "has no typed v1 equivalent, upgrade aborts with
//! all candidate spans; it never chooses one."
//!
//! **It is a line rewriter, not a re-render.** The obvious implementation --
//! parse to [`crate::AppModel`], print v1 -- cannot preserve a comment or a
//! blank line, because neither reaches the model. The pre-v1 grammar is itself
//! line-oriented, so rewriting line by line preserves all three for free and a
//! line this module does not recognize is a refusal rather than a silent drop.
//!
//! **Every refusal below is a construct v1 deliberately dropped**, not a gap
//! here. `@as` is the pre-v1 way to give a declaration a stable label that
//! differs from its name; v1 derives the label from the name and pins identity
//! with `@id`, so carrying `@as` over would silently rekey every cross
//! reference to that declaration. `@package` is §22's own abort row: the fix is
//! a canonical layer move or an ejection, and neither is a syntax change.
//!
//! **The two new axes come from the workspace, not from the source.** §22:
//! "The legacy file does not contain the new `platform` and `build` axes.
//! During upgrade, the importer MUST inspect the selected module once and
//! materialize `platform spring|plain` and `build maven|gradle`." An
//! unrecognized build system aborts -- that is the "unsupported build language"
//! row, and guessing `maven` would produce a model that compiles and writes
//! into the wrong build file.

mod identity;
mod lookup;

use identity::{notes, preserves_identity};

use crate::{AppModel, Diagnostics, StableId};

/// The two axes §22 requires the caller to observe rather than read.
///
/// They are values rather than an `Option` pair because "no evidence" is an
/// abort at the capture boundary, not a default this module may invent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Axes {
    pub platform: Platform,
    pub build: Build,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Platform {
    Spring,
    Plain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Build {
    Maven,
    Gradle,
}

impl Platform {
    const fn word(self) -> &'static str {
        match self {
            Self::Spring => "spring",
            Self::Plain => "plain",
        }
    }
}

impl Build {
    const fn word(self) -> &'static str {
        match self {
            Self::Maven => "maven",
            Self::Gradle => "gradle",
        }
    }
}

/// Translate a pre-v1 JDL draft into JDL v1 source.
///
/// **The legacy source is compiled first, and the result is compiled after.**
/// §22 requires the upgrade to preserve explicit IDs and physical names, and
/// the two dialects derive both differently: pre-v1 keys a field's ID off its
/// entity's *label* where v1 keys it off the entity's *ID*, so
/// `fld_task_title` and `fld_ent_task_title` name the same column in two
/// files that look equally correct. Translating the syntax alone therefore
/// re-identifies most of the model in silence, which is the one outcome a
/// bridge must not have: the next `sync` sees every field as new.
///
/// So the legacy model supplies the identity, this writes it out explicitly,
/// and [`preserves_identity`] proves it landed. The result is not formatted:
/// `format_jdl_v1` owns layout, and running it here would make one command
/// answer two questions.
pub fn upgrade(source: &str, axes: Axes) -> Result<Upgraded, Diagnostics> {
    if super::v1::is_v1(source) {
        return Err(refuse(
            0,
            "this source is already JDL v1",
            "there is nothing to upgrade; `jails model fmt` formats it",
        ));
    }
    let legacy = super::parse(source)?;
    let upgraded = rewrite(source, axes, &legacy)?;
    let linked = super::v1::parse(&upgraded)?;
    preserves_identity(&legacy, &linked)?;
    Ok(Upgraded {
        notes: notes(&legacy, &linked),
        source: upgraded,
    })
}

/// The upgraded source, and what it changes about the model beyond spelling.
///
/// **The notes are not decoration.** §22 has the upgrade "produce a diff and
/// require normal review" precisely because two of the translations mean
/// something: `dialect postgresql` becomes `storage postgres`, and v1 reads a
/// SQL storage axis as a `db` capability -- so a draft that declared a dialect
/// without one gains a JDBC adapter. And v1 keeps a record's fields in
/// declaration order where the pre-v1 draft sorted them by label, so the
/// positional constructor moves. A reviewer reading a hundred-line diff should
/// not have to notice either for themselves.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Upgraded {
    pub source: String,
    pub notes: Vec<String>,
}

fn rewrite(source: &str, axes: Axes, legacy: &AppModel) -> Result<String, Diagnostics> {
    let header = Header::read(source, axes)?;
    let mut output = String::from("jdl 1\n");
    output.push_str(&header.app_block());

    let mut cursor = Cursor {
        depth: Depth::Root,
        entity: String::new(),
        legacy,
    };
    for (offset, raw) in source.lines().enumerate() {
        let number = offset + 1;
        if header.consumed.contains(&number) {
            continue;
        }
        let (body, comment) = split_comment(raw);
        let line = body.trim();
        if line.is_empty() {
            // A comment-only line, or a blank one. Both are the reader's and
            // survive verbatim -- this is the half a re-render cannot do.
            output.push_str(raw.trim_end());
            output.push('\n');
            continue;
        }
        let indent = &raw[..raw.len() - raw.trim_start().len()];
        let Some(translated) = cursor.line(number, line)? else {
            continue;
        };
        output.push_str(indent);
        output.push_str(&translated);
        if !comment.is_empty() {
            output.push(' ');
            output.push_str(comment.trim_end());
        }
        output.push('\n');
    }
    if !matches!(cursor.depth, Depth::Root) {
        return Err(refuse(
            source.lines().count().max(1),
            "a JDL declaration is missing its closing `}`",
            "close the entity, enum, or operation block",
        ));
    }
    Ok(output)
}

/// Which block the rewriter is inside, and the legacy model it reads identity
/// from.
///
/// Pre-v1 and v1 nest identically -- entity, then operation -- so the depth is
/// the whole parser state the translation needs. `entity` is the enclosing
/// entity's label, which is the key every nested lookup needs.
struct Cursor<'a> {
    depth: Depth,
    entity: String,
    legacy: &'a AppModel,
}

enum Depth {
    Root,
    Entity,
    Enum,
    Operation,
}

impl Cursor<'_> {
    /// The v1 spelling of one legacy line, or `None` where v1 states the same
    /// thing somewhere else and the line is dropped.
    fn line(&mut self, number: usize, line: &str) -> Result<Option<String>, Diagnostics> {
        match self.depth {
            Depth::Root => self.root(number, line),
            Depth::Entity => self.entity_member(number, line).map(Some),
            Depth::Enum => {
                if line == "}" {
                    self.depth = Depth::Root;
                }
                Ok(Some(line.to_string()))
            }
            Depth::Operation => {
                if line == "}" {
                    self.depth = Depth::Entity;
                    return Ok(Some("}".to_string()));
                }
                operation_property(number, line).map(Some)
            }
        }
    }

    fn root(&mut self, number: usize, line: &str) -> Result<Option<String>, Diagnostics> {
        reject_unsupported(number, line)?;
        if let Some(rest) = line.strip_prefix("entity ") {
            self.depth = Depth::Entity;
            self.entity = label(first_word(rest.trim_end_matches('{').trim()));
            return self.entity_header(number, rest).map(Some);
        }
        if let Some(rest) = line.strip_prefix("enum ") {
            self.depth = Depth::Enum;
            let name = first_word(rest.trim_end_matches('{').trim());
            let id = self.entity_id(number, &label(name))?;
            return Ok(Some(format!("enum {name} @id({id}) {{")));
        }
        if let Some(rest) = line.strip_prefix("capability ") {
            return self.capability(number, rest.trim());
        }
        if let Some(rest) = line.strip_prefix("dependency ") {
            return dependency(rest.trim()).map(Some);
        }
        if let Some(rest) = line.strip_prefix("setting ") {
            return self.setting(number, rest.trim()).map(Some);
        }
        if let Some(rest) = line.strip_prefix("eject ") {
            return Ok(Some(format!("eject {}", rest.trim())));
        }
        for kind in UNIT_KINDS {
            if let Some(rest) = line.strip_prefix(&format!("{kind} ")) {
                return Ok(Some(format!("component {kind} {}", rest.trim())));
            }
        }
        Err(refuse(
            number,
            format!("`{line}` is not a pre-v1 top-level declaration"),
            "remove it, or upgrade a source the legacy parser accepts",
        ))
    }

    fn entity_header(&self, number: usize, rest: &str) -> Result<String, Diagnostics> {
        let rest = rest.trim_end_matches('{').trim();
        let name = first_word(rest);
        let entity = self.entity(number, &label(name))?;
        let mut header = format!("entity {name} @id({}) {{", entity.id.as_str());
        if !entity.active {
            header = format!("entity {name} @id({}) @retired {{", entity.id.as_str());
        }
        // `@scaffold`, `@factory`, `@dto` and `@repository` are pre-v1 header
        // markers; v1 states them as `use` members, so they leave the header
        // and become the first lines of the block.
        for projection in projections_of(rest) {
            header.push_str(&format!("\n  use {projection}"));
        }
        // A `table` the reader pinned is a physical name §22 must preserve,
        // and pre-v1 spells it nowhere the rewriter can see -- it is derived.
        // Emitting it unconditionally is what keeps a pluralizer or naming
        // change on either side from silently renaming the table.
        header.push_str(&format!("\n  table \"{}\"", entity.names.sql_table));
        Ok(header)
    }

    fn entity_member(&mut self, number: usize, line: &str) -> Result<String, Diagnostics> {
        if line == "}" {
            self.depth = Depth::Root;
            return Ok("}".to_string());
        }
        reject_unsupported(number, line)?;
        if let Some(rest) = line.strip_prefix("index ") {
            return self.index(number, rest.trim());
        }
        if OPERATION_KINDS
            .iter()
            .any(|kind| line.starts_with(&format!("{kind} ")))
        {
            self.depth = Depth::Operation;
            return self.operation_header(number, line);
        }
        self.field(number, line)
    }

    fn field(&self, number: usize, line: &str) -> Result<String, Diagnostics> {
        let line = line.trim_end_matches([',', ';']).trim();
        let (name, rest) = line.split_once(':').ok_or_else(|| {
            refuse(
                number,
                format!("`{line}` is not a pre-v1 field"),
                "write `name: type`, optionally followed by `!`, `?`, or an attribute",
            )
        })?;
        let name = name.trim();
        let type_token = first_word(rest);
        // The length range rides on the type token pre-v1 (`string!(1..200)`)
        // and is an attribute in v1 (`@length(1..200)`), so it comes off first
        // -- otherwise the `!`/`?` suffix is not where the suffix test looks.
        let (shape, length) = match type_token.find('(') {
            Some(open) if type_token.ends_with(')') => (
                &type_token[..open],
                Some(&type_token[open + 1..type_token.len() - 1]),
            ),
            _ => (type_token, None),
        };
        let (type_name, optional, non_blank) = match shape.strip_suffix('!') {
            Some(value) => (value, false, true),
            None => match shape.strip_suffix('?') {
                Some(value) => (value, true, false),
                None => (shape, false, false),
            },
        };
        let field = self.field_node(number, name)?;
        let mut out = format!("{name}: {type_name}");
        if optional {
            out.push('?');
        }
        out.push_str(&format!(" @id({})", field.id.as_str()));
        if non_blank {
            out.push_str(" @notBlank");
        }
        for marker_name in ["pk", "unique", "index"] {
            if marker(rest, marker_name) {
                out.push_str(&format!(" @{marker_name}"));
            }
        }
        // `@column` is the pre-v1 spelling of the physical name and v1 calls
        // it `@map`. It is written whether or not the reader pinned one, for
        // the reason the table is: a derived column name is still a physical
        // name §22 must carry across, and the two dialects derive it apart.
        out.push_str(&format!(" @map({})", field.names.sql_column));
        if let Some(length) = length {
            out.push_str(&format!(" @length({length})"));
        }
        reject_unsupported(number, rest)?;
        Ok(out)
    }

    fn index(&self, number: usize, rest: &str) -> Result<String, Diagnostics> {
        let open = rest.find('(').ok_or_else(|| {
            refuse(
                number,
                "the index has no column list",
                "write `index (title, createdAt desc)`",
            )
        })?;
        let close = rest[open + 1..]
            .find(')')
            .map(|at| open + 1 + at)
            .ok_or_else(|| {
                refuse(
                    number,
                    "the index column list is not closed",
                    "close the index columns with `)`",
                )
            })?;
        let columns = bare_list(&rest[open + 1..close]);
        let index = self.index_node(number, &columns)?;
        Ok(format!(
            "index [{columns}] @id({}) @map({})",
            index.id.as_str(),
            index.sql_name
        ))
    }

    fn operation_header(&self, number: usize, line: &str) -> Result<String, Diagnostics> {
        let declaration = line.trim_end_matches('{').trim();
        reject_unsupported(number, declaration)?;
        let mut head = declaration;
        if let Some(at) = head.find('@') {
            head = head[..at].trim_end();
        }
        let name = head
            .split_once(char::is_whitespace)
            .map(|(_, rest)| rest.trim())
            .unwrap_or_default();
        let name = name.split('(').next().unwrap_or_default().trim();
        let id = self.operation_id(number, &label(name))?;
        Ok(format!("{head} @id({id}) {{"))
    }

    fn capability(&self, number: usize, rest: &str) -> Result<Option<String>, Diagnostics> {
        reject_unsupported(number, rest)?;
        let kind = first_word(rest);
        // §22: `capability db`, its `postgres` alias and `capability h2` are
        // "removed after materializing `storage postgres`/`storage h2`". The
        // `app` block already carries the axis, and v1 materializes the same
        // capability from it -- so keeping the line would declare it twice.
        if matches!(kind, "db" | "postgres" | "h2") {
            return Ok(None);
        }
        let mut line = format!("cap {kind}");
        if let Some(name) = annotation(rest, "name") {
            line.push(' ');
            line.push_str(name);
        }
        let label = label(annotation(rest, "as").unwrap_or(kind));
        let id = self.capability_id(number, &label)?;
        line.push_str(&format!(" @id({id})"));
        Ok(Some(line))
    }

    fn setting(&self, number: usize, rest: &str) -> Result<String, Diagnostics> {
        let (head, value) = rest.split_once('=').ok_or_else(|| {
            refuse(
                number,
                "a pre-v1 setting has no value",
                "write `setting server.port = \"8080\"`",
            )
        })?;
        let head = head.trim();
        let key = first_word(head);
        let target = annotation(head, "target").unwrap_or("main");
        let id = self.setting_id(number, key, target)?;
        let mut line = format!("prop {key} = {} @id({id})", value.trim());
        if target != "main" {
            line.push_str(&format!(" @target({target})"));
        }
        Ok(line)
    }
}

/// The pre-v1 top-level unit kinds, which v1 spells `component <kind> <Name>`.
const UNIT_KINDS: [&str; 8] = [
    "class",
    "interface",
    "service",
    "test",
    "integration-test",
    "sealed",
    "strategy",
    "controller",
];

const OPERATION_KINDS: [&str; 4] = ["command", "query", "transition", "event"];

/// The v1 `use` members a pre-v1 entity header implies.
fn projections_of(header: &str) -> Vec<&'static str> {
    let mut projections = Vec::new();
    if marker(header, "scaffold") {
        projections.push("scaffold");
    }
    if let Some(values) = annotation(header, "facets") {
        for facet in values.split(',').map(str::trim) {
            // `record` is the deliberate omission: v1 gives every entity a
            // record, so `use record` does not exist and would not parse.
            if let Some(projection) = match facet {
                "repository" => Some("repo"),
                "service" => Some("service"),
                "http" => Some("http"),
                "dto" => Some("dto"),
                "factory" => Some("factory"),
                "value" => Some("value"),
                "search" => Some("search"),
                "seed" => Some("seed"),
                _ => None,
            } {
                projections.push(projection);
            }
        }
    }
    for (marker_name, projection) in [
        ("factory", "factory"),
        ("dto", "dto"),
        ("repository", "repo"),
    ] {
        if marker(header, marker_name) {
            projections.push(projection);
        }
    }
    projections
}

fn operation_property(number: usize, line: &str) -> Result<String, Diagnostics> {
    let Some((key, value)) = line.trim_end_matches([',', ';']).split_once(':') else {
        return Err(refuse(
            number,
            format!("`{line}` is not a pre-v1 operation property"),
            "write `key: value` inside the operation",
        ));
    };
    let value = value.trim();
    match key.trim() {
        "sets" => Ok(format!("update [{}]", bare_list(value))),
        "orderBy" | "order_by" => Ok(format!("order by [{}]", bare_list(value))),
        "limit" => Ok(format!("limit {value}")),
        "yields" => Ok(format!("emit {value}")),
        "route" => {
            let (method, path) = value.split_once(char::is_whitespace).ok_or_else(|| {
                refuse(
                    number,
                    format!("`{value}` is not a route"),
                    "write `route: POST /tasks`",
                )
            })?;
            Ok(format!("route {} \"{}\"", method.trim(), path.trim()))
        }
        other => Err(refuse(
            number,
            format!("`{other}` is not a pre-v1 operation property"),
            "use sets, orderBy, limit, yields, or route",
        )),
    }
}

fn dependency(rest: &str) -> Result<String, Diagnostics> {
    let (head, version) = match rest.split_once('=') {
        Some((head, encoded)) => (head.trim(), Some(encoded.trim().to_string())),
        None => (rest, None),
    };
    let mut line = format!("dep {}", first_word(head));
    if let Some(version) = version {
        line.push_str(&format!(" @version({version})"));
    }
    // A dependency's ID is derived from its coordinate in both dialects, so
    // it is the one declaration that needs no pinning -- and pinning it would
    // still not hold its *label*, which v1 derives from the coordinate rather
    // than from the ID. Nothing references a dependency by label.
    if let Some(id) = annotation(head, "id") {
        line.push_str(&format!(" @id({id})"));
    }
    if let Some(scope) = annotation(head, "scope") {
        line.push_str(&format!(" @scope({scope})"));
    }
    Ok(line)
}

/// The inside of a pre-v1 `[a, b desc]` or `a, b desc`, without its brackets.
fn bare_list(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The root properties that become the `app { }` block.
struct Header {
    name: String,
    id: Option<String>,
    package: String,
    java: String,
    storage: &'static str,
    axes: Axes,
    /// Source lines already spoken for, so the body pass skips them.
    consumed: Vec<usize>,
}

impl Header {
    fn read(source: &str, axes: Axes) -> Result<Self, Diagnostics> {
        let mut name = None;
        let mut id = None;
        let mut package = None;
        let mut java = None;
        let mut storage = None;
        let mut consumed = Vec::new();
        for (offset, raw) in source.lines().enumerate() {
            let number = offset + 1;
            let line = split_comment(raw).0.trim();
            if let Some(rest) = line.strip_prefix("application ") {
                reject_unsupported(number, rest)?;
                let rest = rest.trim();
                name = Some(first_word(rest).to_string());
                id = annotation(rest, "id").map(str::to_string);
            } else if let Some(rest) = line.strip_prefix("package ") {
                package = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("java ") {
                java = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("dialect ") {
                storage = Some(match rest.trim() {
                    "postgresql" | "postgres" => "postgres",
                    "h2" => "h2",
                    "sqlite" => "sqlite",
                    other => {
                        return Err(refuse(
                            number,
                            format!("`{other}` is not a storage JDL v1 can express"),
                            "use `dialect postgresql`, `dialect h2`, or `dialect sqlite`",
                        ));
                    }
                });
            } else {
                continue;
            }
            consumed.push(number);
        }
        let Some(name) = name else {
            return Err(refuse(
                1,
                "this source has no `application` header",
                "add `application MyApp` before upgrading",
            ));
        };
        Ok(Self {
            name,
            id,
            package: package.unwrap_or_default(),
            java: java.unwrap_or_default(),
            // `storage none` is the honest answer for a draft that declared no
            // dialect. Inventing `postgres` here would put a schema in a
            // project that has none.
            storage: storage.unwrap_or("none"),
            axes,
            consumed,
        })
    }

    fn app_block(&self) -> String {
        let mut block = format!("app {}", self.name);
        if let Some(id) = &self.id {
            block.push_str(&format!(" @id({id})"));
        }
        block.push_str(" {\n");
        if !self.package.is_empty() {
            block.push_str(&format!("  pkg {}\n", self.package));
        }
        if !self.java.is_empty() {
            block.push_str(&format!("  java {}\n", self.java));
        }
        block.push_str(&format!("  platform {}\n", self.axes.platform.word()));
        block.push_str(&format!("  build {}\n", self.axes.build.word()));
        block.push_str(&format!("  storage {}\n", self.storage));
        block.push_str("}\n");
        block
    }
}

/// The two annotations §22 refuses rather than translates.
///
/// Both would compile if dropped, which is exactly why they are checked: `@as`
/// rekeys every reference to the declaration it names, and `@package` moves
/// generated code out from under the canonical layout. §22 gives `@package` a
/// fix that is not a syntax change -- "plan a canonical layer move or eject the
/// implementation boundary" -- so this says that instead of choosing one.
fn reject_unsupported(number: usize, text: &str) -> Result<(), Diagnostics> {
    if annotation(text, "as").is_some() {
        return Err(refuse(
            number,
            "`@as` has no JDL v1 equivalent",
            "JDL v1 derives the stable label from the declared name and pins identity with \
             `@id`. Rename the declaration to match its label, or keep this source on the \
             legacy parser.",
        ));
    }
    if annotation(text, "package").is_some() {
        return Err(refuse(
            number,
            "`@package` has no JDL v1 equivalent",
            "JDL v1 places generated code by the canonical layout. Plan a canonical layer move \
             for unchanged managed code, or eject the implementation boundary first.",
        ));
    }
    Ok(())
}

fn split_comment(line: &str) -> (&str, &str) {
    let body = super::strip_comment(line);
    (body, &line[body.len()..])
}

fn annotation<'a>(input: &'a str, name: &str) -> Option<&'a str> {
    super::annotation(input, name)
}

fn marker(input: &str, name: &str) -> bool {
    super::marker(input, name)
}

fn first_word(input: &str) -> &str {
    super::first_word(input)
}

fn label(value: &str) -> String {
    super::label(value)
}

fn refuse(line: usize, message: impl Into<String>, fix: impl Into<String>) -> Diagnostics {
    super::problem(line, message, fix)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AXES: Axes = Axes {
        platform: Platform::Spring,
        build: Build::Maven,
    };

    /// One draft using every pre-v1 construct the rewriter recognizes.
    const DRAFT: &str = r#"// the notes application
application Notes @id(project_notes)
package com.example.notes
java 26
dialect postgresql

capability api @id(cap_api)
capability json @name(Dataset)
capability db
dependency org.apache.commons:commons-csv @scope(test) = "1.13.0"
setting server.port @target(main) = "8080"

entity Task @id(ent_task) @scaffold {
  // what it is called
  id: uuid @pk
  title: string!(1..200) @id(fld_task_title) @unique @column(task_title)
  done: boolean?
  rank: int @index

  index (title, done desc) @id(idx_task_recent)

  event TaskChanged(title) {
  }

  query Open(title) {
    orderBy: title, rank
    limit: 20
    route: GET /tasks
  }

  transition Rename(title) {
    sets: title
    yields: task_changed
  }
}

enum Status {
  OPEN
  CLOSED
}

class Clock @id(unit_class_clock)
"#;

    /// The property the whole module exists for.
    ///
    /// `upgrade` runs [`preserves_identity`] itself, so this asserting `Ok` is
    /// the check -- but it is spelled out again here because the failure it
    /// guards against is silent: an upgrade that re-keys a field produces a v1
    /// file that compiles, and the damage shows up as a migration on the next
    /// `sync`.
    #[test]
    fn upgrading_keeps_every_identity_and_physical_name() {
        let before = crate::parse_jdl(DRAFT).unwrap();
        let upgraded = upgrade(DRAFT, AXES).unwrap().source;
        let after = crate::parse_jdl(&upgraded).unwrap();
        preserves_identity(&before, &after).unwrap();

        let task = before
            .entities
            .values()
            .find(|entity| entity.label == "task")
            .unwrap();
        for field in &task.fields {
            assert!(
                upgraded.contains(&format!("@id({})", field.id.as_str())),
                "field `{}` is not pinned in\n{upgraded}",
                field.label
            );
        }
    }

    /// The formatter is the second reader of everything this writes, and a
    /// file it rejects is one `jails model fmt` cannot touch afterwards.
    #[test]
    fn the_upgraded_source_is_formattable_and_stays_v1() {
        let upgraded = upgrade(DRAFT, AXES).unwrap().source;
        assert!(super::super::v1::is_v1(&upgraded));
        let formatted = crate::format_jdl_v1(&upgraded).unwrap();
        assert_eq!(crate::format_jdl_v1(&formatted).unwrap(), formatted);
        crate::parse_jdl(&formatted).unwrap();
    }

    /// §22: the two new axes come from the module, and the source cannot
    /// state them.
    #[test]
    fn the_two_new_axes_come_from_the_caller() {
        let draft = "application Demo @id(project_demo)\npackage com.example.demo\njava 26\n\
                     dialect postgresql\nentity Task @id(ent_task) {\n  id: uuid @pk\n}\n";
        let plain = upgrade(
            draft,
            Axes {
                platform: Platform::Plain,
                build: Build::Gradle,
            },
        )
        .unwrap()
        .source;
        assert!(plain.contains("platform plain"), "{plain}");
        assert!(plain.contains("build gradle"), "{plain}");
    }

    /// **A pre-v1 draft can say something v1 refuses, and the upgrade must
    /// pass that refusal through rather than soften it.**
    ///
    /// `@scaffold` writes an HTTP projection, and v1 requires `platform
    /// spring` for one. Pre-v1 has no platform axis at all, so the same source
    /// is legal there and the conflict only becomes visible once the axis is
    /// materialized -- which is the upgrade doing its job. The temptation is to
    /// relax the prerequisite; the diagnostic is the correct outcome, and it
    /// names both the projection and the fix.
    #[test]
    fn a_scaffold_on_a_plain_platform_refuses_with_the_prerequisite() {
        let error = upgrade(
            DRAFT,
            Axes {
                platform: Platform::Plain,
                build: Build::Maven,
            },
        )
        .unwrap_err();
        let rendered = format!("{error:?}");
        assert!(
            rendered.contains("requires `repo`, `service`, and platform spring"),
            "{rendered}"
        );
    }

    /// §22 removes `capability db` after materializing `storage postgres`;
    /// keeping the line would declare the same capability twice.
    #[test]
    fn the_storage_capability_moves_into_the_app_block() {
        let upgraded = upgrade(DRAFT, AXES).unwrap().source;
        assert!(upgraded.contains("storage postgres"), "{upgraded}");
        assert!(!upgraded.contains("cap db"), "{upgraded}");
        let after = crate::parse_jdl(&upgraded).unwrap();
        assert_eq!(
            after
                .capabilities
                .values()
                .filter(|capability| capability.kind == "db")
                .count(),
            1
        );
    }

    /// The two changes an upgrade makes that are not spelling, said out loud.
    ///
    /// Both are visible in the diff and neither is obvious in one: a reviewer
    /// scanning a hundred lines should not have to notice for themselves that
    /// a JDBC adapter appeared or that a record's constructor moved.
    #[test]
    fn the_upgrade_reports_what_it_changes_beyond_spelling() {
        let draft = "application Demo @id(project_demo)\npackage com.example.demo\njava 26\n\
                     dialect postgresql\nentity Task @id(ent_task) {\n  title: string!\n  \
                     id: uuid @pk\n}\n";
        let notes = upgrade(draft, AXES).unwrap().notes;
        assert!(
            notes.iter().any(|note| note.contains("`db` capability")),
            "{notes:?}"
        );
        assert!(
            notes
                .iter()
                .any(|note| note.contains("declaration order (title, id)")),
            "{notes:?}"
        );
    }

    /// A comment is the reader's and a re-render would lose it, which is why
    /// this is a line rewriter.
    #[test]
    fn comments_and_blank_lines_survive() {
        let upgraded = upgrade(DRAFT, AXES).unwrap().source;
        assert!(upgraded.contains("// the notes application"), "{upgraded}");
        assert!(upgraded.contains("// what it is called"), "{upgraded}");
    }

    #[test]
    fn the_two_untranslatable_annotations_refuse_by_name() {
        let base = "application Demo @id(project_demo)\npackage com.example.demo\njava 26\n\
                    dialect postgresql\n";
        let renamed =
            format!("{base}entity WorkItem @id(ent_task) @as(task) {{\n  id: uuid @pk\n}}\n");
        let error = upgrade(&renamed, AXES).unwrap_err();
        assert!(format!("{error:?}").contains("`@as` has no JDL v1 equivalent"));

        let placed = format!("{base}class Clock @id(unit_class_clock) @package(core)\n");
        let error = upgrade(&placed, AXES).unwrap_err();
        assert!(format!("{error:?}").contains("`@package` has no JDL v1 equivalent"));
    }

    #[test]
    fn a_source_that_is_already_v1_refuses() {
        let error = upgrade("jdl 1\napp Demo {\n  pkg com.example\n}\n", AXES).unwrap_err();
        assert!(format!("{error:?}").contains("already JDL v1"));
    }

    /// A dotted group or key is what every real dependency and almost every
    /// real property has, and pre-v1 refused both until `label` became
    /// `naming::stable_fragment`. Kept as a regression test on the legacy
    /// parser, not on the upgrade: the upgrade cannot run on a source that
    /// does not link.
    #[test]
    fn a_dotted_dependency_group_and_setting_key_link() {
        let draft = "application Demo @id(project_demo)\npackage com.example.demo\njava 26\n\
                     dialect postgresql\ndependency org.apache.commons:commons-csv = \"1.13.0\"\n\
                     setting server.port = \"8080\"\n";
        let model = crate::parse_jdl(draft).unwrap();
        assert_eq!(model.dependencies.len(), 1);
        assert_eq!(model.settings.len(), 1);
        upgrade(draft, AXES).unwrap();
    }
}
