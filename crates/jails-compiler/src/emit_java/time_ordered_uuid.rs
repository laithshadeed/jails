//! Shared application-owned implementation of the closed `uuid7()` default.

use super::{JAVA_ROOT, Unit, render};
use crate::CompileError;
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile};
use jails_model::{AppModel, Package, StableId, Value};
use std::collections::BTreeSet;

pub(super) fn lower(model: &AppModel) -> Result<Option<Unit>, CompileError> {
    // An outbox mints its event's own identity, which is the second minter in
    // the model and reaches this class the same way a `uuid7()` field default
    // does. Asking only about defaults emitted the call and not the class.
    if !crate::emit_operation::outbox::mints_identity(model)
        && !model.entities.values().any(|entity| {
            entity.fields.iter().any(|field| {
                matches!(
                field.semantics.default.as_ref().map(|default| &default.value),
                Some(Value::Function { name, arguments })
                    if name == "uuid7" && arguments.is_empty()
                )
            })
        })
    {
        return Ok(None);
    }

    let package = model.project.package_for(Package::Domain);
    let type_name = "TimeOrderedUuid";
    let artifact_id = "art_app_time_ordered_uuid";
    let imports = BTreeSet::from([
        "java.security.SecureRandom".to_string(),
        "java.util.UUID".to_string(),
    ]);
    let body = r#"public final class TimeOrderedUuid {

    private static final SecureRandom RANDOM = new SecureRandom();

    private TimeOrderedUuid() {}

    public static UUID next() {
        byte[] value = new byte[16];
        RANDOM.nextBytes(value);
        long milliseconds = System.currentTimeMillis();
        value[0] = (byte) (milliseconds >>> 40);
        value[1] = (byte) (milliseconds >>> 32);
        value[2] = (byte) (milliseconds >>> 24);
        value[3] = (byte) (milliseconds >>> 16);
        value[4] = (byte) (milliseconds >>> 8);
        value[5] = (byte) milliseconds;
        value[6] = (byte) ((value[6] & 0x0f) | 0x70);
        value[8] = (byte) ((value[8] & 0x3f) | 0x80);
        long high = 0;
        long low = 0;
        for (int index = 0; index < 8; index++) {
            high = (high << 8) | (value[index] & 0xffL);
        }
        for (int index = 8; index < 16; index++) {
            low = (low << 8) | (value[index] & 0xffL);
        }
        return new UUID(high, low);
    }
}"#;
    let rendered = render(&package, &imports, body, artifact_id);
    let path = ProjectPath::parse(format!(
        "{JAVA_ROOT}/{}/{}.java",
        package.replace('.', "/"),
        type_name
    ))
    .map_err(CompileError::new)?;
    Ok(Some(Unit {
        path,
        file: RenderedFile {
            kind: FileKind::JavaMain,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id: artifact_id.to_string(),
                ejection_id: None,
                ejectable: false,
                semantic_ids: BTreeSet::from([model.project.id.as_str().to_string()]),
                compiler_pass: "java-default-support".to_string(),
            },
        },
    }))
}
