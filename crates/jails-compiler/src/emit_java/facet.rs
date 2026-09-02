//! What Java each entity facet means.
//!
//! Split out of [`super`] by secret rather than by size: the parent traverses
//! the model, renders a [`Unit`] and holds the type/import helpers every
//! emitter shares, and this answers one question -- given an entity and a
//! facet, what is the package, the type name and the body. The two grew
//! together because the match arms use those helpers, which is what
//! `use super::*` is for.

use super::*;

pub(super) fn lower_facet(
    model: &AppModel,
    entity: &Entity,
    facet: Facet,
    spring_boot: Option<&str>,
) -> Result<Unit, CompileError> {
    let domain_package = crate::emit_java::entity_package(model, entity, Package::Domain);
    let (package, type_name, body, mut imports) = match facet {
        Facet::Enum => (
            domain_package.clone(),
            entity.names.java_type.clone(),
            crate::emit_enum::shape(entity),
            crate::emit_enum::imports(entity),
        ),
        Facet::Record => {
            let mut imports = BTreeSet::new();
            let fields = entity.fields.iter().collect::<Vec<_>>();
            (
                domain_package.clone(),
                entity.names.java_type.clone(),
                record_shape(&entity.names.java_type, &fields, &mut imports),
                imports,
            )
        }
        Facet::Factory => unreachable!("factory has a test-source backend"),
        Facet::Dto => unreachable!("dto has a multi-file backend"),
        Facet::Repository => {
            let package = crate::emit_java::entity_package(model, entity, Package::Repository);
            let primary_key = primary_key(entity)?;
            let mut imports = BTreeSet::from([
                "java.util.List".to_string(),
                "java.util.Optional".to_string(),
                format!("{domain_package}.{}", entity.names.java_type),
            ]);
            let key_type = java_type(primary_key, &mut imports);
            let type_name = format!("{}Repository", entity.names.java_type);
            let variable = lower_first(&entity.names.java_type);
            let body = format!(
                "public interface {type_name} {{\n\n    Optional<{}> findById({key_type} id);\n\n    List<{}> findAll();\n\n    {} save({} {variable});\n\n    boolean deleteById({key_type} id);\n\n    // Reader extensions belong below this stable boundary.\n}}",
                entity.names.java_type,
                entity.names.java_type,
                entity.names.java_type,
                entity.names.java_type,
            );
            (package, type_name, body, imports)
        }
        // **The whole resource surface, not a one-method stub**, and it is the
        // generated `ArchitectureTest` that settles it. That suite's
        // `CONTROLLERS_DO_NOT_EXPOSE_PERSISTENCE` rule forbids a `*Controller`
        // depending on the repository package, and the controller has four
        // methods to serve -- so a service that only saved left the two
        // generators contradicting each other, and a freshly scaffolded
        // project failed its own architecture test on the first `mvn test`.
        //
        // `modern.md` §6.4 calls a forwarding service ceremony, and it is
        // ceremony right up until the first business rule, which is the moment
        // a project without one has to touch every call site in the web layer.
        // The scaffold is code the reader grows; the boundary is the point.
        Facet::Service => {
            let package = crate::emit_java::entity_package(model, entity, Package::Service);
            let primary_key = primary_key(entity)?;
            let type_name = format!("{}Service", entity.names.java_type);
            let record = &entity.names.java_type;
            let mut imports = BTreeSet::from([
                "java.util.List".to_string(),
                "java.util.Objects".to_string(),
                "java.util.Optional".to_string(),
                format!("{domain_package}.{record}"),
                format!(
                    "{}.{record}Repository",
                    crate::emit_java::entity_package(model, entity, Package::Repository)
                ),
            ]);
            let key_type = java_type(primary_key, &mut imports);
            let variable = lower_first(record);
            // A concrete bean rather than a second port: the repository
            // interface is already the seam a test substitutes at, and a
            // service interface with exactly one implementation is a level of
            // indirection with nothing behind it. Plain Maven gets the same
            // class without the annotation -- the type is the boundary, the
            // annotation only says who constructs it.
            let annotation = if spring_boot.is_some() {
                imports.insert("org.springframework.stereotype.Component".to_string());
                "@Component\n"
            } else {
                ""
            };
            let body = format!(
                "/**\n\
                 \x20* What the application can do with {{@link {record}}}.\n\
                 \x20*\n\
                 \x20* <p>Depends on the port, not on an adapter, so a test can hand it an\n\
                 \x20* in-memory implementation and never start a database.\n\
                 \x20*/\n\
                 {annotation}\
                 public class {type_name} {{\n\n    \
                 private final {record}Repository repository;\n\n    \
                 public {type_name}({record}Repository repository) {{\n        \
                 this.repository = Objects.requireNonNull(repository, \"repository is required\");\n    \
                 }}\n\n    \
                 public Optional<{record}> byId({key_type} id) {{\n        \
                 return repository.findById(id);\n    }}\n\n    \
                 public List<{record}> all() {{\n        \
                 return repository.findAll();\n    }}\n\n    \
                 public {record} save({record} {variable}) {{\n        \
                 return repository.save({variable});\n    }}\n\n    \
                 public boolean delete({key_type} id) {{\n        \
                 return repository.deleteById(id);\n    }}\n\n    \
                 // Reader-owned application methods belong below this stable boundary.\n}}"
            );
            (package, type_name, body, imports)
        }
        // Three files rather than one: the port, the controller that serves
        // the resource, and its test. `emit_resource_http` owns them, and the
        // loop above routes the facet there before reaching this.
        Facet::Http => unreachable!("http has a multi-file backend"),
        Facet::Events => {
            let package = crate::emit_java::entity_package(model, entity, Package::PortsEvents);
            let type_name = format!("{}Events", entity.names.java_type);
            let imports = BTreeSet::from([format!("{domain_package}.{}", entity.names.java_type)]);
            let body = format!(
                "public interface {type_name} {{\n\n    void publish({} event);\n}}",
                entity.names.java_type
            );
            (package, type_name, body, imports)
        }
        // Three files rather than one, so it never reaches here. Falling
        // through to the *factory's* arm instead makes `use seed` link,
        // validate, and emit `<Name>Factory.java` while reporting success
        // (`bugs.md` B59). A wrong artifact reported as written is a worse
        // failure than a missing one, because nothing looks wrong.
        Facet::Seed => unreachable!("seed has a multi-file backend"),
        Facet::Search => {
            let package = crate::emit_java::entity_package(model, entity, Package::PortsSearch);
            let type_name = format!("{}Search", entity.names.java_type);
            let record = &entity.names.java_type;
            let imports = BTreeSet::from([
                "java.util.List".to_string(),
                format!("{domain_package}.{record}"),
            ]);
            // `matching(query, limit)`, not `search(query)`. There is no
            // unbounded overload on purpose: a search with no limit is a full
            // scan waiting for the table to grow, and the caller who wants
            // everything can say so.
            let body = format!(
                "public interface {type_name} {{\n\n    /**\n     * @param query what the reader typed. It is parsed by PostgreSQL, not\n     *     concatenated into SQL -- see the adapter.\n     * @param limit how many rows at most.\n     */\n    List<{record}> matching(String query, int limit);\n}}"
            );
            (package, type_name, body, imports)
        }
    };
    imports.remove(&format!("{package}.{type_name}"));
    let artifact_id = format!("art_{}_{}", entity.id.as_str(), facet_name(facet));
    let rendered = render(&package, &imports, &body, &artifact_id);
    let package_path = package.replace('.', "/");
    let path = ProjectPath::parse(format!("{JAVA_ROOT}/{package_path}/{type_name}.java"))
        .map_err(CompileError::new)?;
    Ok(Unit {
        path,
        file: RenderedFile {
            kind: FileKind::JavaMain,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: false,
                semantic_ids: BTreeSet::from([entity.id.as_str().to_string()]),
                compiler_pass: "java-facets".to_string(),
            },
        },
    })
}
