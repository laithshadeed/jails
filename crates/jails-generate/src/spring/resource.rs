//! The Spring artifacts `g scaffold` emits: the service, the controller in
//! both its scoped and unscoped shapes, their tests, and the in-memory
//! adapter.
//!
//! Split out of `spring.rs` under `plan.md` §6.5's rule -- a module is one
//! secret, and this one is "what a scaffolded resource looks like on Spring",
//! which nothing else here shares. `scope_controller_parts` stays in the
//! parent because `g query` needs it too.

use super::*;

// ---------------------------------------------------------------------------
// The scaffold's service and controller -- working CRUD rather than stubs.
// ---------------------------------------------------------------------------

/// The application service a scaffolded resource gets.
///
/// Thin on purpose: it delegates to the port and returns domain types. What
/// it buys is a seam -- the controller depends on this rather than on a
/// repository, so the day one of these operations grows a rule (a permission
/// check, an event to publish) there is somewhere for it to go that is not a
/// controller method.
pub fn resource_service_java(pkg: &str, name: &str, extra: &str) -> String {
    let var = crate::generate::lower_first(name);
    crate::template::render(
        crate::template_here!("spring/resource_service_java.java"),
        &[
            ("pkg", pkg),
            ("extra", extra),
            ("name", name),
            ("var", &*var),
        ],
    )
}

/// A REST resource with the four operations that actually exist, wired to
/// the service and speaking in DTOs.
///
/// The status codes are the ones the situations mean, which is most of what
/// distinguishes a REST API from a set of methods reachable over HTTP: 201
/// with a `Location` for a creation, 204 for a delete that removed
/// something, 404 for one that did not, and 404 rather than an empty 200 for
/// a missing item.
pub fn resource_controller_java(
    slice: &Slice,
    name: &str,
    extra: &str,
    has_id: bool,
    fields: &[crate::generate::Field],
) -> String {
    let pkg: &str = &slice.placed(Layer::Web);
    if fields.iter().any(|field| field.constraints.scoped) {
        return scoped_resource_controller_java(slice, name, extra, has_id, fields);
    }
    let path = format!("/{}", crate::sql::table_name(name).replace('_', "-"));
    // A `Location` header needs something to point at. Without an `id`
    // component there is no per-item URL to build, and inventing one would
    // be worse than omitting the header.
    let (location_import, created) = if has_id {
        (
            "import java.net.URI;\n",
            format!(
                "        return ResponseEntity.created(URI.create(PATH + \"/\" + created.id()))\n\
                 \x20               .body({name}Response.from(created));"
            ),
        )
    } else {
        (
            "",
            format!(
                "        // No `id` component, so there is no per-item URL to\n\
                 \x20       // advertise in a Location header.\n\
                 \x20       return ResponseEntity.status(HttpStatus.CREATED).body({name}Response.from(created));"
            ),
        )
    };
    let status_import = if has_id {
        ""
    } else {
        "import org.springframework.http.HttpStatus;\n"
    };
    crate::template::render(
        crate::template_here!("spring/resource_controller_java.java"),
        &[
            ("pkg", pkg),
            ("extra", extra),
            ("location_import", location_import),
            ("status_import", status_import),
            ("name", name),
            ("path", &*path),
            ("created", &*created),
        ],
    )
}

fn scoped_resource_controller_java(
    slice: &Slice,
    name: &str,
    extra: &str,
    has_id: bool,
    fields: &[crate::generate::Field],
) -> String {
    let security: &str = slice.base();
    let pkg: &str = &slice.placed(Layer::Web);
    let path = format!("/{}", crate::sql::table_name(name).replace('_', "-"));
    let (
        scope_import,
        scope_field,
        scope_constructor,
        scope_assignment,
        scope_parameter,
        scope_checks,
    ) = scope_controller_parts(security, pkg, fields, "request");
    let (location_import, created) = if has_id {
        (
            "import java.net.URI;\n",
            format!(
                "        return ResponseEntity.created(URI.create(PATH + \"/\" + created.id()))\n                 .body({name}Response.from(created));"
            ),
        )
    } else {
        (
            "",
            format!(
                "        return ResponseEntity.status(HttpStatus.CREATED).body({name}Response.from(created));"
            ),
        )
    };
    let status_import = if has_id {
        ""
    } else {
        "import org.springframework.http.HttpStatus;\n"
    };
    crate::template::render(
        crate::template_here!("spring/scoped_resource_controller_java.java"),
        &[
            ("pkg", pkg),
            ("extra", extra),
            ("scope_import", &*scope_import),
            ("location_import", location_import),
            ("status_import", status_import),
            ("name", name),
            ("path", &*path),
            ("scope_field", &*scope_field),
            ("scope_constructor", &*scope_constructor),
            ("scope_assignment", &*scope_assignment),
            ("scope_parameter", &*scope_parameter),
            ("scope_checks", &*scope_checks),
            ("created", &*created),
        ],
    )
}

/// The controller's test: a standalone MVC harness with the service replaced.
///
/// `MockMvcTester.of` installs the real Spring MVC mappings, argument
/// conversion, response conversion, and exception handling around this exact
/// controller without starting a Boot application context per generated
/// resource. Security configuration and full-context behavior retain their
/// dedicated Spring tests; this companion test stays focused on HTTP and uses
/// a fresh Mockito service for every method.
pub fn resource_controller_test_java(
    slice: &Slice,
    name: &str,
    extra: &str,
    fields: &[crate::generate::Field],
    sample: (&str, &[String]),
) -> String {
    let security: &str = slice.base();
    let pkg: &str = &slice.placed(Layer::Web);
    let route_file = crate::sql::snake_case(name);
    let (body, unsampled) = sample;
    // A Java text block strips the indentation of its least-indented line, so
    // the whole object is written at the closing delimiter's column and the
    // literal comes out flush.
    let create_body = body
        .lines()
        .map(|line| format!("            {line}\n"))
        .collect::<String>();
    let create_body = format!("            {{\n{create_body}            }}\n");
    // jails' one rule for a test it cannot fully write: emit it whole and
    // disabled, naming what is missing. A required component whose type jails
    // has no sample for is `null` in the collection, which the record refuses
    // -- so this test would fail on every build of a project nobody has
    // touched yet.
    let (disabled, disabled_import) = if unsampled.is_empty() {
        (String::new(), String::new())
    } else {
        (
            format!(
                "@Disabled(\"todo: supply a request sample for {} -- jails cannot know how to \
                 build one\")\n    ",
                unsampled.join(", ")
            ),
            "import org.junit.jupiter.api.Disabled;\n".to_string(),
        )
    };
    if fields.iter().any(|field| field.constraints.scoped) {
        let guard_import = crate::generate::import_of(pkg, security, "ScopeAuthorizer");
        return crate::template::render(
            crate::template_here!("spring/resource_controller_test_scoped_java.java"),
            &[
                ("pkg", pkg),
                ("extra", extra),
                ("guard_import", &*guard_import),
                ("name", name),
                ("route_file", &*route_file),
                ("create_body", &*create_body),
                ("disabled", &*disabled),
                ("disabled_import", &*disabled_import),
            ],
        );
    }
    crate::template::render(
        crate::template_here!("spring/resource_controller_test_java.java"),
        &[
            ("pkg", pkg),
            ("extra", extra),
            ("name", name),
            ("route_file", &*route_file),
            ("create_body", &*create_body),
            ("disabled", &*disabled),
            ("disabled_import", &*disabled_import),
        ],
    )
}

/// The scaffolded service's test.
///
/// The repository is a Mockito mock rather than a hand-written fake, for one
/// reason: a fake has to key items by something, and jails cannot know which
/// component of an arbitrary record is its identity. A mock needs no such
/// knowledge, so this test compiles for every field spec.
///
/// What it pins is delegation and the two boolean-ish outcomes that are easy
/// to get backwards -- an absent item is `Optional.empty()`, and a delete
/// reports whether anything was actually removed.
pub fn resource_service_test_java(pkg: &str, name: &str, extra: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/resource_service_test_java.java"),
        &[("pkg", pkg), ("extra", extra), ("name", name)],
    )
}

/// An in-memory adapter, so a freshly scaffolded application starts and
/// serves requests before anyone has wired a database.
///
/// This is the piece that makes `jails g scaffold` produce something you can
/// actually run: the JDBC adapter is deliberately not a bean (it takes a
/// `Connection`, which the caller owns), so without this the context fails to
/// start with "no qualifying bean of type ...Repository" -- a scaffold that
/// compiles and cannot run.
///
/// It is also the honest default for the stage a scaffold is generated at.
/// Swap the `@Component` annotation onto the JDBC adapter when there is a
/// real `DataSource`; keeping both annotated would make two beans qualify for
/// one injection point, which Spring refuses to choose between (`jails
/// beans` reports exactly that).
/// The in-memory adapter.
///
/// `is_bean` decides whether it carries `@Component`, and exactly one of
/// this and the JDBC adapter may -- see `generate::RepositoryWiring`. Two
/// annotated adapters make two beans qualify for one injection point, and the
/// scaffold then compiles and refuses to start.
pub fn in_memory_repository_java(
    pkg: &str,
    name: &str,
    extra: &str,
    id_accessor: Option<&str>,
    is_bean: bool,
) -> String {
    let var = crate::generate::lower_first(name);
    let (find_by_id, delete_by_id, save_body, note) = match id_accessor {
        Some(accessor) => (
            "        return Optional.ofNullable(items.get(id));".to_string(),
            "        return items.remove(id) != null;".to_string(),
            format!("        items.put(String.valueOf({var}.{accessor}()), {var});"),
            " * <p>Keyed on the record's own {@code id} component.\n",
        ),
        None => (
            "        // TODO: this type has no `id` component, so jails cannot\n\
             \x20       // tell which part of it is the identity. Pick one and key\n\
             \x20       // `items` on it.\n\
             \x20       return Optional.empty();"
                .to_string(),
            "        return items.remove(id) != null;".to_string(),
            format!("        items.put(String.valueOf(items.size()), {var});"),
            " * <p>This type declares no {@code id} component, so lookups by id are\n\
             \x20* left unimplemented -- see the TODO in {@code findById}.\n",
        ),
    };
    // Exactly one adapter is the bean. When the JDBC one is, this is a fake
    // for tests and says so rather than pretending to be a stand-in for a
    // database that now exists.
    let repository_annotation = if is_bean { "@Component\n" } else { "" };
    let repository_import = if is_bean {
        "import org.springframework.stereotype.Component;\n"
    } else {
        ""
    };
    let role_note = if is_bean {
        " * <p>When a real {@code DataSource} arrives, `jails add db` makes\n * {@code Jdbc"
            .to_string()
            + name
            + "Repository} the bean and drops the annotation here. Annotating\n * both makes two beans qualify for one injection point, which Spring\n * refuses to choose between.\n"
    } else {
        " * <p>Not a bean: this project has a {@code DataSource}, so {@code Jdbc".to_string()
            + name
            + "Repository}\n * is the {@code @Component}. This stays as a fake for tests that want a\n * repository without a container -- construct it directly.\n"
    };
    crate::template::render(
        crate::template_here!("spring/in_memory_repository_java.java"),
        &[
            ("pkg", pkg),
            ("extra", extra),
            ("repository_import", repository_import),
            ("name", name),
            ("note", note),
            ("role_note", &*role_note),
            ("repository_annotation", repository_annotation),
            ("find_by_id", &*find_by_id),
            ("var", &*var),
            ("save_body", &*save_body),
            ("delete_by_id", &*delete_by_id),
        ],
    )
}
