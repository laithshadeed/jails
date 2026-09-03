//! Lower semantic facets into deterministic Java source units.
//!
//! The one-file facets and the operation ports are [`crate::recipe::Recipe`]
//! rows over named fragment renderers (`entity` and `operation` hold the
//! rows, [`fragment`] the entity's renderers); what is still a function here
//! is several files from one facet (`dto`, `http`, `seed`) and the repository
//! adapters, which choose their owner and their bean by what the captured
//! build has on its classpath.

mod entity;
mod execution_context;
mod fragment;
mod record_validation;
mod repository;
mod storage;
mod time_ordered_uuid;

use crate::Diagnostic;
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree};
use jails_model::{
    AppModel, BuiltinType, Entity, EntityId, EnumConstant, Facet, Field, FieldId, Operation,
    OperationKind, OperationParameter, Package, ParameterSource, StableId, TypeRef,
};
use std::collections::BTreeSet;

mod input;
mod operation;

pub(crate) use input::{
    Binder, event_component_names, import_declared_type, input_components, parameter_components,
    parameter_member, record_shape_bound, wire_name,
};

pub(crate) const JAVA_ROOT: &str = jails_contracts::SourceRoot::MainJava.path();

pub(crate) fn emit(
    model: &AppModel,
    output: &mut RenderedTree,
    snapshot: &jails_contracts::WorkspaceSnapshot,
) -> Result<(), Diagnostic> {
    let spring_boot = snapshot.project.spring_boot.as_deref();
    let templates = &snapshot.template_overrides;
    let jdbc = crate::emit::jdbc_on_classpath(&snapshot.project);
    crate::emit_unit::emit(model, output, spring_boot)?;
    if let Some(unit) = execution_context::lower(model)? {
        output
            .insert(unit.path, unit.file)
            .map_err(crate::refuse::duplicate_emission)?;
    }
    if let Some(unit) = time_ordered_uuid::lower(model)? {
        output
            .insert(unit.path, unit.file)
            .map_err(crate::refuse::duplicate_emission)?;
    }
    for entity in model.entities.values().filter(|entity| entity.active) {
        // The one-file facets, the test-data builder and the enum converter:
        // rows, each present when the entity declares its facet.
        for recipe in entity::RECIPES {
            crate::recipe::render(model, entity, recipe, snapshot, output)?;
        }
        for facet in &entity.facets {
            match facet {
                Facet::Seed => {
                    for (path, file) in crate::emit_seed::lower(model, entity, templates)? {
                        output
                            .insert(path, file)
                            .map_err(crate::refuse::duplicate_emission)?;
                    }
                }
                // **The scaffold's HTTP surface, which is three files**: the
                // port, the controller that serves the resource, and its
                // test. A port alone -- an interface with no implementation,
                // no route and no caller -- would serve nothing.
                Facet::Http => {
                    for unit in crate::emit_resource_http::lower(model, entity, spring_boot)? {
                        output
                            .insert(unit.path, unit.file)
                            .map_err(crate::refuse::duplicate_emission)?;
                    }
                }
                Facet::Dto => {
                    for unit in crate::emit_dto::lower(model, entity, spring_boot) {
                        let unit = unit?;
                        output
                            .insert(unit.path, unit.file)
                            .map_err(crate::refuse::duplicate_emission)?;
                    }
                }
                // The companion test ships with the type, not as an opt-in:
                // a generated class nobody asserts anything about leaves the
                // suite green over it. See `emit_companion_test`.
                Facet::Record | Facet::Enum => {
                    if let Some(unit) = crate::emit_companion_test::lower(model, entity, *facet)? {
                        output
                            .insert(unit.path, unit.file)
                            .map_err(crate::refuse::duplicate_emission)?;
                    }
                }
                Facet::Factory
                | Facet::Repository
                | Facet::Service
                | Facet::Events
                | Facet::Search => {}
            }
        }
    }
    for operation in model.operations.values() {
        crate::recipe::render(model, operation, &operation::PORTS, snapshot, output)?;
    }
    // **A repository port always has an implementation**, and that is the
    // scaffold's whole promise: it produces a resource that *runs*. Gating the
    // in-memory adapter on the `fake` capability alone leaves a scaffolded
    // project with a `@Component` service constructor-injecting a port no bean
    // satisfies, so the application refuses to start with "No qualifying bean
    // of type ...Repository" -- a project that compiles and cannot boot, which
    // is exactly the failure `jails beans` exists to report.
    //
    // The bean is still unique: `lower_db_repository` carries `@Repository`
    // when `db` is declared and this one drops the annotation, so the two
    // never both qualify for one injection point.
    let fake = model
        .capabilities
        .values()
        .find(|capability| capability.kind == "fake");
    // **Declared, or already on the classpath.** A project that states
    // `spring-boot-starter-data-jdbc` in its own build has JDBC whether or not
    // the model declares `db`, and the bean has to be the adapter that talks
    // to it -- see `emit::jdbc_on_classpath`. Emitting the in-memory one as the bean
    // there gives a project with a real database a `LinkedHashMap` beside a
    // query adapter reading from PostgreSQL: two answers to one question, and
    // the wrong one wired in.
    let declared = model
        .capabilities
        .values()
        .find(|capability| capability.kind == "db");
    let stored = jdbc || declared.is_some();
    // **One contract, emitted once, whichever adapters the project has.** A
    // repository entity always gets at least one -- the in-memory adapter
    // stands in until `db` is declared -- so the contract always has a caller,
    // and it refuses on the same unsampleable entity they do rather than
    // shipping a test class nothing calls.
    for entity in model
        .entities
        .values()
        .filter(|entity| entity.active && entity.facets.contains(&Facet::Repository))
    {
        if let Some(unit) = repository::lower_repository_contract(model, entity, templates)? {
            output
                .insert(unit.path, unit.file)
                .map_err(crate::refuse::duplicate_emission)?;
        }
    }
    if fake.is_some() || !stored {
        let owner = fake.map_or("cap_scaffold_default", |capability| capability.id.as_str());
        for entity in model
            .entities
            .values()
            .filter(|entity| entity.active && entity.facets.contains(&Facet::Repository))
        {
            let node = storage::Stored::fake(owner, entity, !stored && spring_boot.is_some());
            crate::recipe::render(model, &node, node.recipe(), snapshot, output)?;
            // A fake with no test of its own can drift from the adapter it
            // stands in for while every test using it stays green.
            if let Some(unit) =
                repository::lower_fake_repository_test(model, owner, entity, templates)?
            {
                output
                    .insert(unit.path, unit.file)
                    .map_err(crate::refuse::duplicate_emission)?;
            }
        }
    }
    // The owner is the `db` capability where there is one; where JDBC is only
    // an observation, the adapter belongs to the scaffold that asked for the
    // port, the same way the in-memory one does when nothing declares `fake`.
    if let Some(owner) = declared
        .map(|capability| capability.id.as_str())
        .or_else(|| jdbc.then_some("cap_scaffold_default"))
    {
        for entity in model
            .entities
            .values()
            .filter(|entity| entity.active && entity.facets.contains(&Facet::Repository))
        {
            let node = storage::Stored::jdbc(owner, entity);
            crate::recipe::render(model, &node, node.recipe(), snapshot, output)?;
            // The tier that answers the question the adapter exists for. See
            // `lower_db_repository_it`.
            if let Some(unit) = repository::lower_db_repository_it(model, owner, entity)? {
                output
                    .insert(unit.path, unit.file)
                    .map_err(crate::refuse::duplicate_emission)?;
            }
        }
        // The search port's only implementation, and it belongs here rather
        // than beside the port: searching is a JDBC concern and the port
        // exists so a project that later moves search elsewhere replaces this
        // and nothing else.
        for entity in model
            .entities
            .values()
            .filter(|entity| entity.active && entity.facets.contains(&Facet::Search))
        {
            let node = storage::Stored::search(owner, entity);
            crate::recipe::render(model, &node, node.recipe(), snapshot, output)?;
        }
    }
    Ok(())
}

pub(crate) struct Unit {
    pub(crate) path: ProjectPath,
    pub(crate) file: RenderedFile,
}

fn operation_context(model: &AppModel, entity: &Entity, imports: &mut BTreeSet<String>) -> String {
    if entity
        .fields
        .iter()
        .any(|field| field.semantics.scope.is_some())
    {
        imports.insert(format!(
            "{}.ExecutionContext",
            crate::emit_java::entity_package(model, entity, Package::Application)
        ));
        "ExecutionContext context, ".to_string()
    } else {
        String::new()
    }
}

pub(crate) fn entity<'a>(model: &'a AppModel, id: &EntityId) -> Result<&'a Entity, Diagnostic> {
    model.entities.get(id).ok_or_else(|| {
        crate::refuse::unlinked(
            "$.operations",
            format!("linked operation references missing `{id}`"),
        )
    })
}

fn fields<'a>(entity: &'a Entity, ids: &[FieldId]) -> Result<Vec<&'a Field>, Diagnostic> {
    ids.iter()
        .map(|id| {
            entity.field(id).ok_or_else(|| {
                crate::refuse::unlinked(
                    format!("$.entities.{}", entity.id),
                    format!(
                        "linked operation references missing field `{id}` on `{}`",
                        entity.id
                    ),
                )
            })
        })
        .collect()
}

/// Where this entity's classes go: its pinned package, or the layer's.
///
/// **One function, because a slice is only a slice if every part of it moves
/// together.** `--package com.example.demo.billing` collapses the record, the
/// repository, the service, the DTOs and the controller into one package; a
/// call site that reached for `project.package_for` directly would leave its
/// artifact behind in `domain` or `web`, and the import a shared package
/// makes implicit would then be missing rather than wrong -- a file that does not
/// compile, in a slice that looked like it worked.
pub(crate) fn entity_package(model: &AppModel, entity: &Entity, slot: Package) -> String {
    entity
        .java_package
        .clone()
        .unwrap_or_else(|| model.project.package_for(slot))
}

pub(crate) fn domain_import(model: &AppModel, entity: &Entity) -> String {
    format!(
        "{}.{}",
        crate::emit_java::entity_package(model, entity, Package::Domain),
        entity.names.java_type
    )
}

fn java_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

pub(crate) fn with_suffix(value: &str, suffix: &str) -> String {
    if value.ends_with(suffix) {
        value.to_string()
    } else {
        format!("{value}{suffix}")
    }
}

/// Shift a rendered block right, leaving blank lines blank.
///
/// **An empty line gets no prefix**, or the indent turns it into trailing
/// whitespace -- which spotless removes, so a freshly generated project would
/// fail its own `jails check` on bytes jails wrote.
fn indent(value: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    value
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The row version this transition's caller states, and how it states it.
///
/// **The version travels as `If-Match` and comes back as an `ETag`.** A
/// component of the request body would be a bespoke spelling of a thing HTTP
/// already has -- and one no cache, proxy or client library understands. So
/// the port takes it as its own argument, the body does not carry it, and the
/// controller reads the header.
///
/// `None` when the declaration asked for no precondition, in which case the
/// linker has already refused a version parameter and there is nothing to
/// take.
pub(crate) fn precondition<'a>(
    entity: &'a Entity,
    transition: &jails_model::Transition,
) -> Option<Precondition<'a>> {
    let required = match transition.semantics.precondition? {
        jails_model::Precondition::Required => true,
        jails_model::Precondition::Optional => false,
        jails_model::Precondition::None => return None,
    };
    // The linker holds that an if-match transition's entity has exactly one
    // version field, so a miss here is a model that did not link rather than a
    // shape to render around.
    let field = entity.fields.iter().find(|field| field.semantics.version)?;
    Some(Precondition { field, required })
}

/// The version argument a precondition adds to the port.
pub(crate) struct Precondition<'a> {
    pub(crate) field: &'a Field,
    /// Whether the caller must send one. An optional precondition arrives as
    /// `null`, so its Java type is always the boxed one.
    pub(crate) required: bool,
}

impl Precondition<'_> {
    /// The Java type of the expected version, boxed when it may be absent.
    pub(crate) fn java_type(&self, imports: &mut BTreeSet<String>) -> String {
        let java = java_type(self.field, imports);
        if self.required {
            return java;
        }
        match java.as_str() {
            "long" => "Long".to_string(),
            "int" => "Integer".to_string(),
            "short" => "Short".to_string(),
            other => other.to_string(),
        }
    }

    /// The parameter this adds to `execute`, with its leading comma.
    pub(crate) fn parameter(&self, imports: &mut BTreeSet<String>) -> String {
        format!(", {} expectedVersion", self.java_type(imports))
    }
}

pub(crate) fn transition_key<'a>(
    entity: &'a Entity,
    transition: &jails_model::Transition,
) -> Result<&'a Field, Diagnostic> {
    match transition.semantics.select.first() {
        None => primary_key(entity),
        Some(selected) => entity
            .fields
            .iter()
            .find(|field| &field.id == selected)
            .ok_or_else(|| {
                crate::refuse::unlinked(
                    format!("$.entities.{}", entity.id),
                    format!(
                        "linked entity `{}` does not declare selected field `{selected}`",
                        entity.id
                    ),
                )
            }),
    }
}

pub(crate) fn primary_key(entity: &Entity) -> Result<&Field, Diagnostic> {
    entity
        .fields
        .iter()
        .find(|field| field.primary_key)
        .ok_or_else(|| {
            crate::refuse::unlinked(
                format!("$.entities.{}", entity.id),
                format!("linked entity `{}` has no primary key", entity.id),
            )
        })
}

pub(crate) fn java_type(field: &Field, imports: &mut BTreeSet<String>) -> String {
    java_type_ref(&field.ty, field.required, imports)
}

/// The same type where Java will not take a primitive: a generic argument.
///
/// **`Map<long, Note>` is not a type.** A required `int` or `long` component
/// is spelled with the primitive everywhere it is a parameter, a field or a
/// return -- that is what `java_type` answers, and it is right there -- but a
/// type argument has to be the boxed name, and a generator that reaches for
/// the same string in both places emits a file that does not compile. Only
/// the two integral builtins have a primitive at all, so the failure is
/// invisible on the `uuid` and `string` keys everything else is written
/// against. `BuiltinSemantics` carries both names; asking for the
/// not-required spelling is how the boxed one is reached, and it wraps
/// nothing.
pub(crate) fn boxed_java_type(field: &Field, imports: &mut BTreeSet<String>) -> String {
    java_type_ref(&field.ty, false, imports)
}

pub(crate) fn java_type_ref(
    ty: &TypeRef,
    required: bool,
    imports: &mut BTreeSet<String>,
) -> String {
    match ty {
        TypeRef::Builtin(builtin) => builtin_java(*builtin, required, imports),
        TypeRef::External(qualified) => {
            if let Some((_, simple)) = qualified.rsplit_once('.') {
                imports.insert(qualified.clone());
                simple.to_string()
            } else {
                qualified.clone()
            }
        }
    }
}

pub(crate) fn primitive(ty: &TypeRef, required: bool) -> bool {
    required
        && matches!(ty, TypeRef::Builtin(builtin) if builtin.semantics().java_primitive.is_some())
}

pub(crate) struct RecordComponent<'a> {
    pub(crate) name: String,
    pub(crate) ty: &'a TypeRef,
    pub(crate) required: bool,
    non_blank: bool,
    length: Option<&'a jails_model::LengthRange>,
    positive: bool,
    nonnegative: bool,
}

fn builtin_java(builtin: BuiltinType, required: bool, imports: &mut BTreeSet<String>) -> String {
    let (name, import) = builtin.java_type(required);
    if let Some(import) = import {
        imports.insert(import.to_string());
    }
    name.to_string()
}

/// One Java compilation unit before it is text: the package it declares, the
/// types it imports, and the body between them.
///
/// **The one Java shell.** Every emitter builds one of these, and
/// [`JavaUnit::render`] is the only function in the compiler that writes a
/// `package` line, an import block or the provenance header. An import an
/// emitter needs is a fully-qualified *name* added to the set, never a
/// rendered `import` statement spliced into a template placeholder: a
/// placeholder puts a second, unsorted import block in the file and makes two
/// emitters contributing to one file able to write the same import twice.
///
/// A template's own imports join the same set through [`JavaUnit::from_source`],
/// so the template stays a real `.java` file and the block it renders into is
/// still one sorted, deduplicated list.
pub(crate) struct JavaUnit {
    /// Whatever stands above the `package` line, verbatim. A template
    /// override may open with a comment of its own, and a file that loses it
    /// on the way through here would drop the one line saying jails did not
    /// write this shape.
    preamble: String,
    package: String,
    imports: BTreeSet<String>,
    /// Everything after the import block: the type this unit declares.
    /// Deliberately not `body`, which a board row reserves for the one struct
    /// that carries a file about to be written.
    declarations: String,
}

impl JavaUnit {
    pub(crate) fn new(package: &str, imports: &BTreeSet<String>, body: &str) -> Self {
        Self {
            preamble: String::new(),
            package: package.to_string(),
            imports: imports.clone(),
            declarations: body.to_string(),
        }
    }

    /// The unit a rendered template already is.
    ///
    /// The `package` line and the import lines that follow it are lifted into
    /// this value, so a conditional import the emitter adds lands in the same
    /// block. Only the run of imports directly after the package line is
    /// lifted: anything else -- a comment between two imports, say -- ends the
    /// run and stays in the body, which is still legal Java and still ahead of
    /// the type declaration.
    pub(crate) fn from_source(source: &str) -> Self {
        let mut preamble_end = 0;
        let mut package = String::new();
        let mut imports = BTreeSet::new();
        let mut rest = source;
        let mut seen_package = false;
        loop {
            let (line, tail) = match rest.find('\n') {
                Some(at) => (&rest[..at], &rest[at + 1..]),
                None => (rest, ""),
            };
            let trimmed = line.trim();
            if !seen_package {
                // A comment above the package line documents the *file* -- an
                // override says so there -- so it is kept as the preamble. A
                // comment after it documents whatever it precedes and stays in
                // the declarations.
                if trimmed.is_empty() || trimmed.starts_with("//") {
                    rest = tail;
                    preamble_end = source.len() - rest.len();
                    continue;
                }
                let Some(name) = trimmed.strip_prefix("package ").and_then(strip_statement) else {
                    break;
                };
                package = name.trim().to_string();
                seen_package = true;
                rest = tail;
                continue;
            }
            if trimmed.is_empty() {
                rest = tail;
                continue;
            }
            let Some(name) = trimmed.strip_prefix("import ").and_then(strip_statement) else {
                break;
            };
            imports.insert(name.trim().to_string());
            rest = tail;
        }
        Self {
            preamble: source[..preamble_end].to_string(),
            package,
            imports,
            declarations: rest.to_string(),
        }
    }

    /// Import `owner.class`, unless `owner` is this unit's own package --
    /// importing a sibling is redundant, and with `--package ''` both names
    /// are empty and the statement would not parse.
    pub(crate) fn import_from(&mut self, owner: &str, class: &str) {
        if owner == self.package {
            return;
        }
        self.imports.insert(format!("{owner}.{class}"));
    }

    /// Import one fully-qualified name; `static a.b.C.d` for a static import.
    pub(crate) fn import(&mut self, name: impl Into<String>) {
        self.imports.insert(name.into());
    }

    /// The compilation unit with no provenance header: what a codemod that has
    /// to see a whole file is handed, and what [`JavaUnit::from_source`] reads
    /// back.
    pub(crate) fn source(&self) -> String {
        let mut out = String::with_capacity(self.declarations.len() + 64 * self.imports.len() + 64);
        out.push_str(&self.preamble);
        // Written by hand rather than with `format!`, because an `import`
        // statement is spelled in exactly one place in this crate and this is
        // it -- a gate counts the sites that spell one anywhere else.
        out.push_str("package ");
        out.push_str(&self.package);
        out.push_str(";\n");
        // Static imports first, a blank line, then the rest sorted: what
        // palantir-java-format produces, so `add format` leaves a managed tree
        // that passes `jails check`.
        let (statics, rest): (Vec<_>, Vec<_>) = self
            .imports
            .iter()
            .partition(|name| name.starts_with("static "));
        out.push('\n');
        for group in [statics, rest] {
            if group.is_empty() {
                continue;
            }
            for name in group {
                out.push_str("import ");
                out.push_str(name);
                out.push_str(";\n");
            }
            out.push('\n');
        }
        out.push_str(self.declarations.trim_start_matches('\n'));
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }

    pub(crate) fn render(&self, semantic_id: &str) -> String {
        format!(
            "// Generated by jails from {semantic_id}. Clean hand edits survive regeneration.\n{}",
            self.source()
        )
    }
}

/// So an emitter with a template in hand and no imports of its own can pass
/// the rendered text where a unit is wanted.
impl From<String> for JavaUnit {
    fn from(source: String) -> Self {
        Self::from_source(&source)
    }
}

/// The name a `package` or `import` line declares, trailing `;` removed.
fn strip_statement(rest: &str) -> Option<&str> {
    rest.trim_end().strip_suffix(';')
}

fn lower_first(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_lowercase().to_string() + characters.as_str()
    })
}
