//! The HTTP resource a scaffolded entity serves, and the test that drives it.
//!
//! **A scaffold is supposed to produce a *running* resource**, and the
//! canonical `http` facet emitted a one-method `interface <Name>HttpPort` with
//! no implementation, no route and no caller. Nothing served the entity, so a
//! canonical scaffold had no HTTP surface at all while the legacy engine wrote
//! a full CRUD controller for the same declaration.
//!
//! **It speaks the domain record, not a request/response pair.** That is the
//! shape the canonical *operation* controllers already use -- `emit_http`
//! takes `PORT.Input` and answers with the entity -- so a scaffold controller
//! doing the same keeps one wire convention in a project rather than two. It
//! also keeps `scaffold` the four-facet profile it is documented to be: a DTO
//! pair would mean either a fifth facet or a second owner for files
//! `emit_dto` already knows how to write.
//!
//! The repository port is what it delegates to, because that is the port with
//! the whole surface: `service` is a one-method stub, and `validate_
//! prerequisites` already refuses `http` on an entity without `repo`.

use crate::CompileError;
use crate::emit_java::{
    JAVA_ROOT, JAVA_TEST_ROOT, Unit, domain_import, java_type, primary_key, render, with_suffix,
};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile};
use jails_model::{AppModel, Entity, Package, StableId};
use std::collections::BTreeSet;

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
    Ok(units)
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
        model,
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
/// pluralisation the legacy engine used, so a project moving between engines
/// keeps its URLs. `sql_table` rather than a second pluraliser for the reason
/// `CLAUDE.md` gives: a second one does not stay in step, and the divergence
/// shows up as a route that does not match the table it reads.
fn resource_path(model: &AppModel, entity: &Entity) -> String {
    model
        .projections
        .values()
        .find(|projection| projection.entity == entity.id)
        .and_then(|projection| match &projection.kind {
            jails_model::ProjectionKind::Http { path } => path.clone(),
            _ => None,
        })
        .unwrap_or_else(|| format!("/{}", entity.names.sql_table))
}

fn controller(model: &AppModel, entity: &Entity) -> Result<Unit, CompileError> {
    let package = model.project.package_for(Package::Web);
    let type_name = with_suffix(&entity.names.java_type, "Controller");
    let record = &entity.names.java_type;
    let key = primary_key(entity)?;
    let mut imports = BTreeSet::from([
        domain_import(model, entity),
        format!(
            "{}.{record}Repository",
            model.project.package_for(Package::Repository)
        ),
        "java.net.URI".to_string(),
        "java.util.List".to_string(),
        "org.springframework.http.ResponseEntity".to_string(),
        "org.springframework.web.bind.annotation.DeleteMapping".to_string(),
        "org.springframework.web.bind.annotation.GetMapping".to_string(),
        "org.springframework.web.bind.annotation.PathVariable".to_string(),
        "org.springframework.web.bind.annotation.PostMapping".to_string(),
        "org.springframework.web.bind.annotation.RequestBody".to_string(),
        "org.springframework.web.bind.annotation.RequestMapping".to_string(),
        "org.springframework.web.bind.annotation.RestController".to_string(),
    ]);
    let key_type = java_type(key, &mut imports);
    let key_member = &key.names.java_member;
    let path = resource_path(model, entity);
    let body = format!(
        "@RestController\n\
         @RequestMapping({type_name}.PATH)\n\
         public final class {type_name} {{\n\n\
         \x20   /** The collection this controller serves. */\n\
         \x20   public static final String PATH = \"{path}\";\n\n\
         \x20   private final {record}Repository repository;\n\n\
         \x20   public {type_name}({record}Repository repository) {{\n\
         \x20       this.repository = repository;\n\
         \x20   }}\n\n\
         \x20   @GetMapping\n\
         \x20   public List<{record}> list() {{\n\
         \x20       return repository.findAll();\n\
         \x20   }}\n\n\
         \x20   /** 404 rather than an empty 200: \"no such thing\" and \"here is nothing\" differ. */\n\
         \x20   @GetMapping(\"/{{id}}\")\n\
         \x20   public ResponseEntity<{record}> byId(@PathVariable(\"id\") {key_type} id) {{\n\
         \x20       return repository.findById(id)\n\
         \x20               .map(ResponseEntity::ok)\n\
         \x20               .orElseGet(() -> ResponseEntity.notFound().build());\n\
         \x20   }}\n\n\
         \x20   @PostMapping\n\
         \x20   public ResponseEntity<{record}> create(@RequestBody {record} request) {{\n\
         \x20       {record} created = repository.save(request);\n\
         \x20       return ResponseEntity.created(URI.create(PATH + \"/\" + created.{key_member}()))\n\
         \x20               .body(created);\n\
         \x20   }}\n\n\
         \x20   /** 204 when something was removed, 404 when there was nothing to remove. */\n\
         \x20   @DeleteMapping(\"/{{id}}\")\n\
         \x20   public ResponseEntity<Void> delete(@PathVariable(\"id\") {key_type} id) {{\n\
         \x20       return repository.deleteById(id)\n\
         \x20               ? ResponseEntity.noContent().build()\n\
         \x20               : ResponseEntity.notFound().build();\n\
         \x20   }}\n\n\
         \x20   // Reader-owned controller methods belong below this stable boundary.\n\
         }}"
    );
    let artifact_id = format!("art_{}_http_controller", entity.id.as_str());
    unit(
        model,
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
        "java.util.List".to_string(),
        "java.util.Optional".to_string(),
        "org.junit.jupiter.api.Test".to_string(),
    ]);
    let key_type = java_type(key, &mut imports);
    let Some(row) = crate::emit_companion_test::constructor_call(model, entity, &mut imports)
    else {
        return Ok(None);
    };
    let path = resource_path(model, entity);
    let boot_major = crate::emit_capability::boot_major(spring_boot);
    let modern = boot_major.is_some_and(|major| major >= 4);
    let (field, request) = if modern {
        imports.insert("static org.assertj.core.api.Assertions.assertThat".to_string());
        imports.insert("org.springframework.test.web.servlet.assertj.MockMvcTester".to_string());
        (
            format!(
                "    private final MockMvcTester mvc = MockMvcTester.of(new {controller}(REPOSITORY));"
            ),
            format!("        assertThat(mvc.get().uri(\"{path}\")).hasStatusOk();"),
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
                "    private final MockMvc mvc = standaloneSetup(new {controller}(REPOSITORY)).build();"
            ),
            format!("        mvc.perform(get(\"{path}\")).andExpect(status().isOk());"),
        )
    };
    let throws = if modern { "" } else { " throws Exception" };
    let body = format!(
        "class {type_name} {{\n\n\
         \x20   private static final {record} ROW = {row};\n\n\
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
         {field}\n\n\
         \x20   @Test\n\
         \x20   void theCollectionAnswers(){throws} {{\n\
         {request}\n\
         \x20   }}\n\n\
         \x20   // Reader-owned tests belong below this stable boundary.\n\
         }}"
    );
    let artifact_id = format!("art_{}_http_controller_test", entity.id.as_str());
    unit(
        model,
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
    _model: &AppModel,
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
