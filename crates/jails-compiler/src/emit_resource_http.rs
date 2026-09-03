//! The HTTP resource a scaffolded entity serves, and the test that drives it.
//!
//! **A scaffold is supposed to produce a *running* resource.** The `http`
//! facet's port alone -- a one-method `interface <Name>HttpPort` with no
//! implementation, no route and no caller -- serves nothing, and compiling
//! proves nothing: an unimplemented interface compiles. So the facet emits a
//! full CRUD controller beside the port.
//!
//! **It speaks the domain record, not a request/response pair.** That is the
//! shape the *operation* controllers already use -- `emit_http` takes
//! `PORT.Input` and answers with the entity -- so a project ends up with one
//! wire convention rather than two. It also keeps `scaffold` the four-facet
//! profile it is documented to be: a DTO pair would mean either a fifth facet
//! or a second owner for files `emit_dto` already knows how to write.
//!
//! The repository port is what it delegates to, because that is the port with
//! the whole surface: `service` is a one-method stub, and the linker already
//! refuses `http` on an entity without `repo`.

use crate::CompileError;
use crate::emit_java::{
    JAVA_ROOT, JavaUnit, Unit, domain_import, java_type, primary_key, with_suffix,
};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile};
use jails_model::{AppModel, Entity, Package, StableId, boundary};
use std::collections::{BTreeMap, BTreeSet};

const JAVA_TEST_ROOT: &str = jails_contracts::SourceRoot::TestJava.path();

/// The port, the controller, and the controller's test.
pub(crate) fn lower(
    model: &AppModel,
    entity: &Entity,
    spring_boot: Option<&str>,
) -> Result<Vec<Unit>, CompileError> {
    let mut units = vec![port(model, entity)?];
    // **Only a Spring project gets a controller.** The port is plain Java and
    // costs a project nothing; a `@RestController` needs Spring Web, and on a
    // plain Maven build the dependency that would supply it cannot even be
    // spelled -- a versionless `<dependency>` outside `spring-boot-starter-
    // parent` makes Maven refuse to read the pom at all, `validate` included.
    // So the facet degrades to the port alone rather than handing the reader
    // a build that no longer loads.
    if spring_boot.is_none() {
        return Ok(units);
    }
    units.push(controller(model, entity, spring_boot)?);
    if let Some(test) = controller_test(model, entity, spring_boot)? {
        units.push(test);
    }
    units.push(requests(model, entity)?);
    Ok(units)
}

/// The four requests the controller above answers, as a file an editor sends.
///
/// **Only the ones it answers**, which is why this is rendered beside the
/// controller rather than from the entity alone: the two would otherwise be
/// free to disagree, and a collection whose `### List` block returns 405 tells
/// the reader something about this file rather than about their project.
///
/// `{{baseUrl}}` and `{{id}}` are the HTTP Client format's own variable syntax
/// and are written with a Rust `format!` for that reason -- `template!` reads
/// `{{...}}` as a placeholder, so a `.http` rendered through it would have its
/// own syntax substituted away.
fn requests(model: &AppModel, entity: &Entity) -> Result<Unit, CompileError> {
    let path = resource_path(model, entity);
    let name = &entity.names.java_type;
    let key = primary_key(entity)?;
    let body = documented_body(model, entity, key);
    // **Unquoted**, unlike the body above. `@id` is substituted into a path,
    // where a JSON string's own quotes would be sent literally and the request
    // would 400 on a key nobody mistyped.
    let key_sample = crate::emit_companion_test::json_sample(model, &key.ty)
        .map(|sample| sample.trim_matches('"').to_string())
        .unwrap_or_else(|| "1".to_string());
    // **Only the requests the controller answers.** A scoped resource is
    // create-only -- see `scoped` -- and a collection whose `### List` block
    // returns 405 tells the reader something about this file rather than about
    // their project. The generated controller test asserts that same GET is a
    // 405, and the collection has to agree with it.
    let reads = if scoped(entity) {
        String::new()
    } else {
        format!(
            "### List {name}\n\
             GET {{{{baseUrl}}}}{path}\n\
             Accept: application/json\n\n\
             @id = {key_sample}\n\n\
             ### Get {name}\n\
             GET {{{{baseUrl}}}}{path}/{{{{id}}}}\n\
             Accept: application/json\n\n\
             ### Delete {name}\n\
             DELETE {{{{baseUrl}}}}{path}/{{{{id}}}}\n"
        )
    };
    let collection = format!(
        "@baseUrl = http://localhost:8080\n\n\
         ### Create {name}\n\
         POST {{{{baseUrl}}}}{path}\n\
         Content-Type: application/json\n\n\
         {{\n{}\n}}\n\n\
         {reads}",
        body.join(",\n")
    );
    let artifact_id = boundary::HTTP_REQUESTS.owned_by(entity.id.as_str());
    let path = jails_contracts::SourceRoot::TestHttp
        .join(&format!("{}.http", entity.names.sql_table))
        .map_err(CompileError::new)?;
    Ok(Unit {
        path,
        file: RenderedFile {
            kind: FileKind::HttpCollection,
            mode: FileMode::Regular,
            bytes: collection.into_bytes(),
            provenance: Provenance {
                artifact_id: artifact_id.clone(),
                ejection_id: None,
                ejectable: true,
                semantic_ids: BTreeSet::from([entity.id.as_str().to_string()]),
                compiler_pass: "java-facets".to_string(),
            },
        },
    })
}

/// The managed ABI this facet publishes.
///
/// Kept although the controller that serves the resource does not implement
/// it: it is a port, and a port is ABI.
fn port(model: &AppModel, entity: &Entity) -> Result<Unit, CompileError> {
    let package = crate::emit_java::entity_package(model, entity, Package::PortsHttp);
    let type_name = format!("{}HttpPort", entity.names.java_type);
    let imports = BTreeSet::from([domain_import(model, entity)]);
    let record = &entity.names.java_type;
    let body =
        format!("public interface {type_name} {{\n\n    {record} create({record} request);\n}}");
    let artifact_id = boundary::HTTP.owned_by(entity.id.as_str());
    unit(
        entity,
        package,
        type_name,
        artifact_id,
        imports,
        body,
        FileKind::JavaMain,
        JAVA_ROOT,
        "java-facets",
    )
}

/// The collection path this entity is served at.
///
/// The declared `http /path` wins; otherwise the table name. `sql_table`
/// rather than a second pluraliser: a second one does not stay in step, and
/// the divergence shows up as a route that does not match the table it reads.
fn resource_path(model: &AppModel, entity: &Entity) -> String {
    // **Both halves of the predicate belong in the same closure.** Finding the
    // entity's *first* projection and then asking whether it happens to be the
    // HTTP one reads a pinned route only when nothing else sorts ahead of it --
    // and `scaffold` expands to repo, service and http, so the answer would
    // always be the repository and the pin always dropped: `/operators` for a
    // path the reader wrote into the model, with no diagnostic anywhere.
    model
        .projections
        .values()
        .find_map(|projection| match &projection.kind {
            jails_model::ProjectionKind::Http { path } if projection.entity == entity.id => {
                path.clone()
            }
            _ => None,
        })
        .unwrap_or_else(|| format!("/{}", entity.names.sql_table))
}

/// The JSON a caller sends to create one of these, as the `.http` collection
/// documents it and the companion test posts it.
///
/// One derivation because the two must agree: a documented body the generated
/// test does not exercise is a body nothing checks, which is what two
/// renderers resolving the same fact separately produces.
///
/// A component jails cannot sample is left out rather than guessed at: a wrong
/// body documents a payload the record refuses, which is worse than a shorter
/// one the reader completes.
fn documented_body(model: &AppModel, entity: &Entity, _key: &jails_model::Field) -> Vec<String> {
    entity
        .fields
        .iter()
        // **The request record decides, not a second filter beside it.** A
        // rule of this file's own -- everything but the key and anything with
        // a default -- disagrees with it on a required component carrying a
        // literal default: the record asks for it, the documented body does
        // not supply it, and the generated POST fails with `Cannot map null
        // into type boolean`. One predicate, and the body is by construction
        // what the record accepts.
        .filter(|field| crate::emit_dto::caller_supplied(field))
        .filter_map(|field| {
            let sample = crate::emit_companion_test::named_json_sample(
                model,
                &field.ty,
                &field.names.java_member,
            )?;
            Some(format!("  \"{}\": {sample}", field.names.java_member))
        })
        .collect()
}

/// Whether every read of this resource has to carry a tenant.
///
/// **A scoped resource is create-only over its collection**, and the reason is
/// the whole of what `@scope` is for: the field is proved against a JWT claim
/// at the request boundary, so a `GET /notes` that returns `findAll()` answers
/// with every tenant's rows. There is no honest unscoped read, so none is
/// written -- reading a scoped resource is a `jails g query`, which carries
/// the claim into its predicate. Spring answers the absent methods with 405,
/// which is the true answer rather than a leak.
fn scoped(entity: &Entity) -> bool {
    entity
        .fields
        .iter()
        .any(|field| field.semantics.scope.is_some())
}

fn controller(
    model: &AppModel,
    entity: &Entity,
    spring_boot: Option<&str>,
) -> Result<Unit, CompileError> {
    let package = crate::emit_java::entity_package(model, entity, Package::Web);
    let type_name = with_suffix(&entity.names.java_type, "Controller");
    let record = &entity.names.java_type;
    let key = primary_key(entity)?;
    // **What `create` binds is what this entity declares a boundary for.** A
    // scaffold carries the DTO facet, so the caller is asked for the fields
    // they own and every server-assigned value is minted on the way in. A bare
    // `use http` has no request record to bind, and inventing a reference to
    // one would not compile.
    let bounded = entity.facets.contains(&jails_model::Facet::Dto);
    let (create_doc, bound, from_request) = if bounded {
        (
            format!(
                "/**\n     * The request record, not the domain row: a caller supplies what\n     * they are asked for, and every server-assigned value is minted by\n     * {{@link {record}Request#toDomain()}} rather than taken from the body.\n     */\n    "
            ),
            format!("@Valid @RequestBody {record}Request request"),
            "request.toDomain()".to_string(),
        )
    } else {
        (
            String::new(),
            format!("@RequestBody {record} request"),
            "request".to_string(),
        )
    };
    let create_only = scoped(entity);
    let mut imports = BTreeSet::from([
        domain_import(model, entity),
        // **The service, not the repository.** The suite jails generates
        // beside this file forbids a `*Controller` depending on the
        // repository package, so injecting the port here would make a freshly
        // scaffolded project fail its own `ArchitectureTest` on the first
        // `mvn test` -- two of jails' own generators disagreeing about where
        // the boundary is.
        format!(
            "{}.{record}Service",
            crate::emit_java::entity_package(model, entity, Package::Service)
        ),
        "java.net.URI".to_string(),
        "org.springframework.http.ResponseEntity".to_string(),
        "org.springframework.web.bind.annotation.PostMapping".to_string(),
        "org.springframework.web.bind.annotation.RequestBody".to_string(),
        "org.springframework.web.bind.annotation.RequestMapping".to_string(),
        "org.springframework.web.bind.annotation.RestController".to_string(),
    ]);
    if bounded {
        imports.insert(format!(
            "{}.validation.Valid",
            crate::emit_capability::validation_package(crate::emit_capability::boot_major(
                spring_boot
            ))
        ));
    }
    if !create_only {
        imports.extend([
            "java.util.List".to_string(),
            "org.springframework.web.bind.annotation.DeleteMapping".to_string(),
            "org.springframework.web.bind.annotation.GetMapping".to_string(),
            "org.springframework.web.bind.annotation.PathVariable".to_string(),
        ]);
    }
    let key_type = java_type(key, &mut imports);
    let key_member = &key.names.java_member;
    let path = resource_path(model, entity);
    // The reads and the delete, or nothing where every read must carry a
    // tenant. `create_only` is `scoped`; the doc comment there is the reason.
    let reads = if create_only {
        String::new()
    } else {
        format!(
            "\x20   @GetMapping\n\
             \x20   public List<{record}> list() {{\n\
             \x20       return service.all();\n\
             \x20   }}\n\n\
             \x20   /** 404 rather than an empty 200: \"no such thing\" and \"here is nothing\" differ. */\n\
             \x20   @GetMapping(\"/{{id}}\")\n\
             \x20   public ResponseEntity<{record}> byId(@PathVariable(\"id\") {key_type} id) {{\n\
             \x20       return service.byId(id)\n\
             \x20               .map(ResponseEntity::ok)\n\
             \x20               .orElseGet(() -> ResponseEntity.notFound().build());\n\
             \x20   }}\n\n"
        )
    };
    let removal = if create_only {
        String::new()
    } else {
        format!(
            "\x20   /** 204 when something was removed, 404 when there was nothing to remove. */\n\
             \x20   @DeleteMapping(\"/{{id}}\")\n\
             \x20   public ResponseEntity<Void> delete(@PathVariable(\"id\") {key_type} id) {{\n\
             \x20       return service.delete(id)\n\
             \x20               ? ResponseEntity.noContent().build()\n\
             \x20               : ResponseEntity.notFound().build();\n\
             \x20   }}\n\n"
        )
    };
    let body = format!(
        "@RestController\n\
         @RequestMapping({type_name}.PATH)\n\
         public final class {type_name} {{\n\n\
         \x20   /** The collection this controller serves. */\n\
         \x20   public static final String PATH = \"{path}\";\n\n\
         \x20   private final {record}Service service;\n\n\
         \x20   public {type_name}({record}Service service) {{\n\
         \x20       this.service = service;\n\
         \x20   }}\n\n\
         {reads}\
         \x20   {create_doc}@PostMapping\n\
         \x20   public ResponseEntity<{record}> create({bound}) {{\n\
         \x20       {record} created = service.save({from_request});\n\
         \x20       return ResponseEntity.created(URI.create(PATH + \"/\" + created.{key_member}()))\n\
         \x20               .body(created);\n\
         \x20   }}\n\n\
         {removal}\
         \x20   // Reader-owned controller methods belong below this stable boundary.\n\
         }}"
    );
    let artifact_id = boundary::HTTP_API.owned_by(entity.id.as_str());
    unit(
        entity,
        package,
        type_name,
        artifact_id,
        imports,
        body,
        FileKind::JavaMain,
        JAVA_ROOT,
        "java-facets",
    )
}

/// A request through the real dispatcher, against a stub repository.
///
/// Standalone rather than `@SpringBootTest`, like the operation controllers':
/// the dispatcher is built around one controller and the port is an anonymous
/// implementation, so nothing starts and no database is needed.
///
/// An anonymous class rather than a lambda because the repository port has
/// four methods and is not a functional interface. `None` when the entity has
/// a component jails cannot sample -- the stub has no row to answer with, and
/// a guess would not compile.
fn controller_test(
    model: &AppModel,
    entity: &Entity,
    spring_boot: Option<&str>,
) -> Result<Option<Unit>, CompileError> {
    let package = crate::emit_java::entity_package(model, entity, Package::Web);
    let controller = with_suffix(&entity.names.java_type, "Controller");
    let type_name = format!("{controller}Test");
    let record = &entity.names.java_type;
    let key = primary_key(entity)?;
    let mut imports = BTreeSet::from([
        domain_import(model, entity),
        format!(
            "{}.{record}Repository",
            crate::emit_java::entity_package(model, entity, Package::Repository)
        ),
        // The real service over a stub port: it is four forwards, so
        // substituting a second stub for it would assert that the test's own
        // fake forwards correctly.
        format!(
            "{}.{record}Service",
            crate::emit_java::entity_package(model, entity, Package::Service)
        ),
        "java.util.List".to_string(),
        "java.util.Optional".to_string(),
        "org.junit.jupiter.api.Test".to_string(),
    ]);
    let key_type = java_type(key, &mut imports);
    // The same row every operation proof builds. A sample may name a type this
    // file has not imported -- an enum component lives in `domain` and this
    // test lives in `web` -- which the one sampler handles.
    let Some(row) = crate::emit_operation::proof::record_arguments(
        model,
        entity,
        &BTreeMap::new(),
        &mut imports,
    ) else {
        return Ok(None);
    };
    let path = resource_path(model, entity);
    let dialect = crate::emit_mockmvc::Dialect::of(spring_boot);
    // **What the collection documents is what this proves.** A scoped
    // resource answers only the POST, so asserting an OK on the collection
    // would assert the leak `scoped` exists to prevent -- and 405 is what
    // Spring answers for a path that is mapped and a method that is not, which
    // is the property worth pinning.
    let create_only = scoped(entity);
    let read_comment = match create_only {
        true => "        // Every read carries the tenant, so the collection answers none.\n",
        false => "",
    };
    let tester = dialect.tester(&mut imports);
    let field = format!(
        "    private final {tester} mvc = {};",
        dialect.standalone(
            &format!("new {controller}(new {record}Service(REPOSITORY))"),
            &mut imports
        )
    );
    let created = dialect.drive(
        &crate::emit_mockmvc::Drive {
            verb: "post",
            uri: &path,
            uri_arguments: "",
            extras: "\n                .contentType(MediaType.APPLICATION_JSON)\n                .content(CREATE_REQUEST)",
            status: crate::emit_mockmvc::Status::Created,
            body_text: None,
            indent: "        ",
        },
        &mut imports,
    );
    let read = dialect.drive(
        &crate::emit_mockmvc::Drive {
            verb: "get",
            uri: &path,
            uri_arguments: "",
            extras: "",
            status: match create_only {
                true => crate::emit_mockmvc::Status::MethodNotAllowed,
                false => crate::emit_mockmvc::Status::Ok,
            },
            body_text: None,
            indent: "        ",
        },
        &mut imports,
    );
    let request = format!("{created}\n{read_comment}{read}");
    let throws = dialect.throws();
    let method = "theDocumentedCreateRequestIsAccepted";
    // **The same JSON the `.http` collection documents**, so what the reader
    // is shown and what the build proves cannot diverge -- a documented body
    // nothing exercises is a body nothing checks.
    imports.insert("org.springframework.http.MediaType".to_string());
    let request_constant = format!(
        "\x20   private static final String CREATE_REQUEST =\n\
         \x20           \"\"\"\n\
         \x20           {{\n{}\n\
         \x20           }}\"\"\";\n\n",
        documented_body(model, entity, key)
            .iter()
            .map(|line| format!("\x20           {line}"))
            .collect::<Vec<_>>()
            .join(",\n")
    );
    let body = format!(
        "class {type_name} {{\n\n\
         \x20   private static final {record} ROW = new {record}({row});\n\n\
         \x20   private static final {record}Repository REPOSITORY = new {record}Repository() {{\n\
         \x20       @Override\n\
         \x20       public Optional<{record}> findById({key_type} id) {{\n\
         \x20           return Optional.of(ROW);\n\
         \x20       }}\n\n\
         \x20       @Override\n\
         \x20       public List<{record}> findAll() {{\n\
         \x20           return List.of(ROW);\n\
         \x20       }}\n\n\
         \x20       @Override\n\
         \x20       public {record} save({record} value) {{\n\
         \x20           return value;\n\
         \x20       }}\n\n\
         \x20       @Override\n\
         \x20       public boolean deleteById({key_type} id) {{\n\
         \x20           return true;\n\
         \x20       }}\n\
         \x20   }};\n\n\
         {request_constant}\
         {field}\n\n\
         \x20   @Test\n\
         \x20   void {method}(){throws} {{\n\
         {request}\n\
         \x20   }}\n\n\
         \x20   // Reader-owned tests belong below this stable boundary.\n\
         }}"
    );
    let artifact_id = boundary::HTTP_API_TEST.owned_by(entity.id.as_str());
    unit(
        entity,
        package,
        type_name,
        artifact_id,
        imports,
        body,
        FileKind::JavaTest,
        JAVA_TEST_ROOT,
        "java-facets-test",
    )
    .map(Some)
}

#[allow(clippy::too_many_arguments)]
fn unit(
    entity: &Entity,
    package: String,
    type_name: String,
    artifact_id: String,
    imports: BTreeSet<String>,
    body: String,
    kind: FileKind,
    root: &str,
    compiler_pass: &str,
) -> Result<Unit, CompileError> {
    let rendered = JavaUnit::new(&package, &imports, &body).render(&artifact_id);
    let package_path = package.replace('.', "/");
    let path = ProjectPath::parse(format!("{root}/{package_path}/{type_name}.java"))
        .map_err(CompileError::new)?;
    Ok(Unit {
        path,
        file: RenderedFile {
            kind,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: true,
                semantic_ids: BTreeSet::from([entity.id.as_str().to_string()]),
                compiler_pass: compiler_pass.to_string(),
            },
        },
    })
}
