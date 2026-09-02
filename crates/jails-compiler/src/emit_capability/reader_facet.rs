//! Compiler projection for capability slices embedded in reader documents.

use crate::CompileError;
use jails_contracts::{
    FileMode, ProjectPath, Provenance, ReaderFacetKind, RenderedReaderFacet, RenderedTree,
};
use jails_model::{Capability, StableId};
use std::collections::BTreeSet;

pub(super) struct ComposeService {
    pub(super) name: &'static str,
    pub(super) marker: &'static str,
    pub(super) body: &'static str,
}

pub(super) fn emit_managed_file(
    output: &mut RenderedTree,
    capability: &Capability,
    suffix: &str,
    path: ProjectPath,
    bytes: Vec<u8>,
    mode: FileMode,
) -> Result<(), CompileError> {
    let artifact_id = format!("doc_{}_file_{suffix}", capability.id.as_str());
    output
        .insert_reader_facet(
            artifact_id.clone(),
            RenderedReaderFacet {
                path,
                kind: ReaderFacetKind::ManagedFile { mode },
                bytes,
                provenance: Provenance {
                    artifact_id,
                    ejection_id: Some(capability.id.as_str().to_string()),
                    ejectable: false,
                    semantic_ids: BTreeSet::from([capability.id.as_str().to_string()]),
                    compiler_pass: format!("capability-project-file-{}", capability.kind),
                },
            },
        )
        .map_err(CompileError::new)
}

pub(super) fn emit_compose_service(
    output: &mut RenderedTree,
    capability: &Capability,
    path: &ProjectPath,
    service: &ComposeService,
) -> Result<(), CompileError> {
    let artifact_id = format!("doc_{}_compose_{}", capability.id.as_str(), service.marker);
    output
        .insert_reader_facet(
            artifact_id.clone(),
            RenderedReaderFacet {
                path: path.clone(),
                kind: ReaderFacetKind::ComposeService {
                    service: service.name.to_string(),
                    marker: service.marker.to_string(),
                },
                bytes: compose_block(service).into_bytes(),
                provenance: Provenance {
                    artifact_id,
                    ejection_id: Some(capability.id.as_str().to_string()),
                    ejectable: false,
                    semantic_ids: BTreeSet::from([capability.id.as_str().to_string()]),
                    compiler_pass: format!("capability-pack-{}", capability.kind),
                },
            },
        )
        .map_err(CompileError::new)
}

fn compose_block(service: &ComposeService) -> String {
    // Two spaces, because a marker at column zero inside a YAML mapping is a
    // parse error rather than a comment in the wrong place -- which is the
    // reason `Marked::indented` exists.
    let marked = jails_codemod::Marked::indented(service.marker, "  ");
    // `render` indents every line it is given, so the body arrives one level
    // short of where it lands: the service name at zero becomes two spaces,
    // and its own lines at two become four.
    let body: String = service
        .body
        .lines()
        .map(|line| format!("  {line}\n"))
        .collect();
    marked.render(&format!("{}:\n{body}", service.name))
}

#[cfg(test)]
mod compose_block_tests {
    use super::*;

    /// The block is written by `Marked` rather than by hand here.
    ///
    /// Pinned because a hand-rendered block disagrees in a way that reads as
    /// correct: the splice indents *every* line it is handed, so a body that
    /// already carries the service's two spaces comes out four in and the
    /// compose file silently changes shape.
    #[test]
    fn the_block_is_indented_exactly_as_compose_needs() {
        let service = ComposeService {
            marker: "db",
            name: "postgres",
            body: "image: postgres:17\nports:\n  - \"5432:5432\"",
        };
        // Built from the same `Marked` the production path uses: spelling the
        // markers here would be a second answer to what a `# jails:` block is.
        assert_eq!(
            compose_block(&service),
            jails_codemod::marked::Marked::indented("db", "  ")
                .render("postgres:\n  image: postgres:17\n  ports:\n    - \"5432:5432\"\n")
        );
    }
}
