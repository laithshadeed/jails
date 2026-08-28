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
    let mut block = format!("  # jails:{}\n  {}:\n", service.marker, service.name);
    for line in service.body.lines() {
        block.push_str("    ");
        block.push_str(line);
        block.push('\n');
    }
    block.push_str(&format!("  # /jails:{}\n", service.marker));
    block
}
