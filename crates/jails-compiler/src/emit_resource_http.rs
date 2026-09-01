//! The HTTP resource a scaffolded entity serves, and the test that drives it.
//!
//! **A scaffold is supposed to produce a *running* resource**, and the `http`
//! facet emits a one-method `interface <Name>HttpPort` with no implementation,
//! no route and no caller. Nothing serves the entity, so a canonical scaffold
//! has no HTTP surface at all while the engine it replaces writes a full CRUD
//! controller for the same declaration. Compiling proves nothing here: an
//! unimplemented interface compiles.
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
    JAVA_ROOT, Unit, domain_import, import_declared_type, java_type, primary_key, render,
    with_suffix,
};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile};
use jails_model::{AppModel, Entity, Package, StableId};
use std::collections::BTreeSet;

const JAVA_TEST_ROOT: &str = ".jails/generated/test/java";

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
    // So the facet degrades to what it always emitted rather than handing the
    // reader a build that no longer loads.
    if spring_boot.is_none() {
        return Ok(units);
    }
    units.push(controller(model, entity)?);
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
    // their project. The generated controller test already asserted that same
    // GET was a 405: the test knew and the collection did not.
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
    let artifact_id = format!("art_{}_http_requests", entity.id.as_str());
    let path = ProjectPath::parse(format!(
        ".jails/generated/requests/{}.http",
        entity.names.sql_table
    ))
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

/// The managed ABI this facet has always published.
///
/// Unchanged, and kept for that reason: it is a port, and a port is ABI even
/// when the thing that now serves the resource does not implement it.
fn port(model: &AppModel, entity: &Entity) -> Result<Unit, CompileError> {
    let package = model.project.package_for(Package::PortsHttp);
    let type_name = format!("{}HttpPort", entity.names.java_type);
    let imports = BTreeSet::from([domain_import(model, entity)]);
    let record = &entity.names.java_type;
    let body =
        format!("public interface {type_name} {{\n\n    {record} create({record} request);\n}}");
    let artifact_id = format!("art_{}_http", entity.id.as_str());
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
/// The declared `http /path` wins; otherwise the table name, which is the same
/// pluralisation the engine this replaces used, so a project moving between
/// them keeps its URLs. `sql_table` rather than a second pluraliser: a second
/// one does not stay in step, and the divergence shows up as a route that does
/// not match the table it reads.
fn resource_path(model: &AppModel, entity: &Entity) -> String {
    // **Both halves of the predicate belong in the same closure.** Finding the
    // entity's *first* projection and then asking whether it happens to be the
    // HTTP one reads a pinned route only when nothing else sorts ahead of it --
    // and `scaffold` expands to repo, service and http, so the answer was
    // always the repository and the pin was always dropped. The reader saw
    // `/operators` for a path they had written into the model, with no
    // diagnostic anywhere.
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

/// Whether every read of this resource has to carry a tenant.
///
/// **A scoped resource is create-only over its collection**, and the reason is
/// the whole of what `@scope` is for: the field is proved against a JWT claim
/// at the request boundary, so a `GET /notes` that returns `findAll()` answers
/// with every tenant's rows. There is no honest unscoped read, so none is
/// written -- reading a scoped resource is a `jails g query`, which carries
/// the claim into its predicate. Spring answers the absent methods with 405,
/// which is the true answer rather than a leak.
/// The JSON a caller sends to create one of these, as the `.http` collection
/// documents it and the companion test posts it.
///
/// One derivation because the two must agree: a documented body the generated
/// test does not exercise is a body nothing checks, which is how `bugs.md` B48
/// -- two renderers resolving the same fact separately -- reads in this file.
///
/// A component jails cannot sample is left out rather than guessed at: a wrong
/// body documents a payload the record refuses, which is worse than a shorter
/// one the reader completes.
fn documented_body(model: &AppModel, entity: &Entity, key: &jails_model::Field) -> Vec<String> {
    entity
        .fields
        .iter()
        .filter(|field| field.id != key.id && field.semantics.default.is_none())
        .filter_map(|field| {
            let sample = crate::emit_companion_test::json_sample(model, &field.ty)?;
            Some(format!("  \"{}\": {sample}", field.names.java_member))
        })
        .collect()
}

fn scoped(entity: &Entity) -> bool {
    entity
        .fields
        .iter()
        .any(|field| field.semantics.scope.is_some())
}

fn controller(model: &AppModel, entity: &Entity) -> Result<Unit, CompileError> {
    let package = model.project.package_for(Package::Web);
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
        // repository package, so injecting the port here made a freshly
        // scaffolded project fail its own `ArchitectureTest` on the first
        // `mvn test` -- two of jails' own generators disagreeing about where
        // the boundary is.
        format!(
            "{}.{record}Service",
            model.project.package_for(Package::Service)
        ),
        "java.net.URI".to_string(),
        "org.springframework.http.ResponseEntity".to_string(),
        "org.springframework.web.bind.annotation.PostMapping".to_string(),
        "org.springframework.web.bind.annotation.RequestBody".to_string(),
        "org.springframework.web.bind.annotation.RequestMapping".to_string(),
        "org.springframework.web.bind.annotation.RestController".to_string(),
    ]);
    if bounded {
        imports.insert("jakarta.validation.Valid".to_string());
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
    let artifact_id = format!("art_{}_http_controller", entity.id.as_str());
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
    let package = model.project.package_for(Package::Web);
    let controller = with_suffix(&entity.names.java_type, "Controller");
    let type_name = format!("{controller}Test");
    let record = &entity.names.java_type;
    let key = primary_key(entity)?;
    let mut imports = BTreeSet::from([
        domain_import(model, entity),
        format!(
            "{}.{record}Repository",
            model.project.package_for(Package::Repository)
        ),
        // The real service over a stub port: it is four forwards, so
        // substituting a second stub for it would assert that the test's own
        // fake forwards correctly.
        format!(
            "{}.{record}Service",
            model.project.package_for(Package::Service)
        ),
        "java.util.List".to_string(),
        "java.util.Optional".to_string(),
        "org.junit.jupiter.api.Test".to_string(),
    ]);
    let key_type = java_type(key, &mut imports);
    let Some(row) = sampled_row(model, entity, &mut imports) else {
        return Ok(None);
    };
    let path = resource_path(model, entity);
    let boot_major = crate::emit_capability::boot_major(spring_boot);
    let modern = boot_major.is_some_and(|major| major >= 4);
    // **What the collection documents is what this proves.** A scoped
    // resource answers only the POST, so asserting an OK on the collection
    // would assert the leak `scoped` exists to prevent -- and 405 is what
    // Spring answers for a path that is mapped and a method that is not, which
    // is the property worth pinning.
    let create_only = scoped(entity);
    let (field, request) = if modern {
        imports.insert("static org.assertj.core.api.Assertions.assertThat".to_string());
        imports.insert("org.springframework.test.web.servlet.assertj.MockMvcTester".to_string());
        (
            format!(
                "    private final MockMvcTester mvc = MockMvcTester.of(new {controller}(new {record}Service(REPOSITORY)));"
            ),
            match create_only {
                true => format!(
                    "        assertThat(mvc.post().uri(\"{path}\")\n\
                     \x20               .contentType(MediaType.APPLICATION_JSON)\n\
                     \x20               .content(CREATE_REQUEST)).hasStatus(201);\n\
                     \x20       // Every read carries the tenant, so the collection answers none.\n\
                     \x20       assertThat(mvc.get().uri(\"{path}\")).hasStatus(405);"
                ),
                false => format!("        assertThat(mvc.get().uri(\"{path}\")).hasStatusOk();"),
            },
        )
    } else {
        imports.insert(
            "static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get"
                .to_string(),
        );
        imports.insert(
            "static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status"
                .to_string(),
        );
        imports.insert("org.springframework.test.web.servlet.MockMvc".to_string());
        imports.insert(
            "static org.springframework.test.web.servlet.setup.MockMvcBuilders.standaloneSetup"
                .to_string(),
        );
        (
            format!(
                "    private final MockMvc mvc = standaloneSetup(new {controller}(new {record}Service(REPOSITORY))).build();"
            ),
            match create_only {
                true => format!(
                    "        mvc.perform(post(\"{path}\")\n\
                     \x20               .contentType(MediaType.APPLICATION_JSON)\n\
                     \x20               .content(CREATE_REQUEST)).andExpect(status().isCreated());\n\
                     \x20       // Every read carries the tenant, so the collection answers none.\n\
                     \x20       mvc.perform(get(\"{path}\")).andExpect(status().isMethodNotAllowed());"
                ),
                false => {
                    format!("        mvc.perform(get(\"{path}\")).andExpect(status().isOk());")
                }
            },
        )
    };
    let throws = if modern { "" } else { " throws Exception" };
    let method = match create_only {
        true => "theDocumentedCreateRequestIsAccepted",
        false => "theCollectionAnswers",
    };
    // The same JSON the `.http` collection documents, so what the reader is
    // shown and what the build proves cannot diverge.
    let request_constant = match create_only {
        false => String::new(),
        true => {
            imports.insert("org.springframework.http.MediaType".to_string());
            if modern {
                imports.insert(
                    "org.springframework.test.web.servlet.assertj.MockMvcTester".to_string(),
                );
            } else {
                imports.insert(
                    "static org.springframework.test.web.servlet.request.MockMvcRequestBuilders::post"
                        .replace("::", ".")
                        .to_string(),
                );
            }
            format!(
                "\x20   private static final String CREATE_REQUEST =\n\
                 \x20           \"\"\"\n\
                 \x20           {{\n{}\n\
                 \x20           }}\"\"\";\n\n",
                documented_body(model, entity, key)
                    .iter()
                    .map(|line| format!("\x20           {line}"))
                    .collect::<Vec<_>>()
                    .join(",\n")
            )
        }
    };
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
    let artifact_id = format!("art_{}_http_controller_test", entity.id.as_str());
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

/// Every component sampled, through the one sampler the proofs use.
///
/// A sample may name a type this file has not imported: an enum component
/// lives in `domain` and this test lives in `web`.
fn sampled_row(
    model: &AppModel,
    entity: &Entity,
    imports: &mut BTreeSet<String>,
) -> Option<String> {
    entity
        .fields
        .iter()
        .map(|field| {
            import_declared_type(model, &field.ty, imports);
            crate::emit_companion_test::sample(model, field, imports)
        })
        .collect::<Option<Vec<_>>>()
        .map(|arguments| arguments.join(", "))
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
    let rendered = render(&package, &imports, &body, &artifact_id);
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
