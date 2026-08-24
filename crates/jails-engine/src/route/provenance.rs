//! Where an output's bytes came from, as the value §R5.2 requires.
//!
//! Every managed output carries a non-optional `RendererStamp`, and the reason
//! is a question a two-way comparison cannot answer. When a generated file is
//! different from what jails would write today, four things could have
//! happened: a new jails version, a moved template, an edited input, or a
//! changed declaration. Without provenance they look identical, and the
//! reconciliation that follows has to guess.
//!
//! ## What is recorded, and what is honestly not
//!
//! The context is what these renderers actually saw: the subject's identity
//! and spec, its resolved references, the base package, the eleven layer
//! packages after any `jails.toml` rename, the Java release, and the
//! capabilities the project declares.
//!
//! Two fields are deliberately empty, and saying why is the point of writing
//! them down rather than inventing something plausible:
//!
//! - **`template` is `None`.** §R5.2 allows `Some` only when template bytes
//!   contributed, and carries exactly one `TemplateStamp`. A recipe here
//!   renders one output from several built-in templates, which one stamp
//!   cannot describe -- and the built-in bytes are pinned by `jails_version`
//!   anyway, because they are `include_str!`d into the binary. `Some` becomes
//!   answerable when §R6.3's `template::{install,resolve}` row lands and a
//!   template can be overridden per project.
//! - **`relevant_inputs` is the canonical empty set.** §R5.2 is explicit that
//!   a renderer records which input IDs it consumed *through `SnapshotView`*,
//!   and that a caller may not hand it an unverified hash. These recipes read
//!   the project directly, so nothing declared a consumed set. Hashing the
//!   whole request's read set instead would be worse than empty: it would make
//!   every unrelated edit appear to explain the change, which §R5.2 names as
//!   the exact failure the field exists to avoid.

use super::*;

use jails_protocol::context::{
    ReferenceRole, RenderedSubjectContext, RendererContextV1, ResolvedReferenceContext,
    relevant_inputs,
};
use jails_protocol::provenance::{RendererId, RendererStamp};
use jails_protocol::render::{DesiredProvenance, TemplateBindings};
use jails_protocol::request::CanonicalCapability;
use jails_spec::spec::layout::Layer;

/// Stamp every file this change writes.
///
/// One renderer per change, so the files it writes share a stamp -- but the
/// stamp is attached per file, because §R5.2 asks the question of the output.
pub(super) fn stamp_files(
    change: &mut DesiredChange,
    project: &Project,
    renderer: RendererId,
    subject: Option<RenderedSubjectContext>,
) -> Result<()> {
    let provenance = provenance(project, renderer, subject)?;
    for file in &mut change.files {
        // Only a *managed output* gets one. A file with no resource is bytes
        // this change states for something nobody owns, and stamping it would
        // claim provenance for a path that has no output row.
        if file.resource.is_some() {
            file.renderer = Some(provenance.clone());
        }
    }
    Ok(())
}

fn provenance(
    project: &Project,
    renderer: RendererId,
    subject: Option<RenderedSubjectContext>,
) -> Result<DesiredProvenance> {
    let references = references(subject.as_ref())?;
    let context = RendererContextV1 {
        renderer: renderer.clone(),
        subject,
        references,
        base_package: Package::parse(project.base())?,
        layers: layers(project)?,
        // The release the generated code targets, which is a fact about the
        // Java these renderers emit rather than about this project's POM.
        java_release: jails_project::pom::TARGET_RELEASE
            .parse()
            .map_err(|_| "the target release is not a number".to_string())?,
        capabilities: capabilities(project)?,
        // §R5.2: empty when `template` is `None`, and `template` is `None`
        // here for the reason this module's header gives.
        bindings: TemplateBindings::new(),
    };
    let body = context.to_object()?;
    Ok(DesiredProvenance {
        stamp: RendererStamp {
            renderer,
            renderer_schema: 1,
            jails_version: env!("CARGO_PKG_VERSION").to_string(),
            template: None,
            context_schema: 1,
            context_object: ObjectId::from_bytes(jails_support::codec::sha256(&body)),
            relevant_inputs: relevant_inputs(&[])?,
            tools: Vec::new(),
        },
        context: std::sync::Arc::from(body.as_slice()),
    })
}

/// The eleven layer packages, in the fixed order, after any rename.
///
/// Not a map and not a subset: §R5.2 requires exactly the eleven roles in
/// order, because an omitted layer and a layer resolved to its default would
/// otherwise encode the same.
fn layers(project: &Project) -> Result<Vec<jails_protocol::context::LayerContext>> {
    let mut out = Vec::new();
    for layer in Layer::ALL {
        out.push(jails_protocol::context::LayerContext {
            layer,
            package: Package::parse(project.layers().layer(layer.package()))?,
        });
    }
    Ok(out)
}

/// Every capability the project declares, sorted by identity.
///
/// Resolved through the one constructor the routes use, so the context a
/// render is stamped with names the same entities the transition declares. A
/// label rebuilt as a singleton would describe a project whose named
/// capabilities are all the default instance -- true of no project that has
/// one.
fn capabilities(project: &Project) -> Result<Vec<CanonicalCapability>> {
    let mut out: Vec<CanonicalCapability> = Vec::new();
    for declaration in project.declarations() {
        let (id, spec) = declaration.resolve(project)?;
        out.push(CanonicalCapability { id, spec });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.dedup_by(|a, b| a.id == b.id);
    Ok(out)
}

/// The subject's `--on`/`--yields`, as resolved references.
///
/// `managed` stays `None` until R1.2's graph validation says which recorded
/// entity a target names; the resolved qualified type is what the renderer
/// actually used, and recording that alone is true rather than incomplete.
fn references(subject: Option<&RenderedSubjectContext>) -> Result<Vec<ResolvedReferenceContext>> {
    let Some(RenderedSubjectContext::Entity {
        spec: EntitySpec::Intent(intent),
        ..
    }) = subject
    else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for (role, target) in [
        (ReferenceRole::On, intent.on.as_ref()),
        (ReferenceRole::Yields, intent.yields.as_ref()),
    ] {
        if let Some(target) = target {
            out.push(ResolvedReferenceContext {
                role,
                target: target.clone(),
                managed: None,
            });
        }
    }
    out.sort_by(|a, b| (a.role, &a.target).cmp(&(b.role, &b.target)));
    Ok(out)
}
