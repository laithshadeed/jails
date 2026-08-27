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
pub(crate) fn resource_service_java(
    slice: &Slice,
    pkg: &str,
    name: &str,
    extra: &str,
    key: &crate::generate::KeyType,
    columns: &[crate::sql::Column],
) -> String {
    let var = crate::generate::lower_first(name);
    // **Identity is minted here, not in the web layer.** The request record
    // used to call `UUID.randomUUID()` inside `toDomain`, which puts the
    // decision "what is this row called" in the HTTP adapter -- the one layer
    // that is supposed to translate and nothing else. modern.md 7,
    // plan.md P4.3.
    let (created, uuid_import) = match crate::sql::server_generated_key(columns) {
        Some((_, expression)) => (
            crate::generate::rebuilt_record(name, &var, columns, expression, "                ")
                .or_else(|| {
                    // `rebuilt_record` answers only for a database-assigned key;
                    // this one is assigned here, so it is built directly.
                    Some(crate::generate::rebuilt_with(
                        name, &var, columns, expression,
                    ))
                })
                .expect("a server-generated key rebuilds the record"),
            crate::generate::import_of(
                pkg,
                &crate::spring::identity::package(slice),
                crate::spring::identity::TIME_ORDERED_UUID,
            ),
        ),
        None => (var.clone(), String::new()),
    };
    crate::template::render(
        crate::template_here!("spring/resource_service_java.java"),
        &[
            ("pkg", pkg),
            ("extra", extra),
            ("key", &key.java),
            ("key_import", &key.import),
            ("uuid_import", &uuid_import),
            ("name", name),
            ("created", &*created),
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
pub(crate) fn resource_controller_java(
    slice: &Slice,
    name: &str,
    extra: &str,
    has_id: bool,
    fields: &[crate::generate::Field],
    key: &crate::generate::KeyType,
) -> String {
    let pkg: &str = &slice.placed(Layer::Web);
    if fields.iter().any(|field| field.constraints.scoped) {
        // The scoped controller has no id lookup and no delete -- a plain
        // repository operation cannot prove a tenant boundary -- so it never
        // names the key type and must not import it either.
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
            (
                "validation",
                crate::spring::validation_package(slice.project()),
            ),
            ("pkg", pkg),
            ("extra", extra),
            ("location_import", location_import),
            ("status_import", status_import),
            ("key", &key.java),
            ("key_import", &key.import),
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
            (
                "validation",
                crate::spring::validation_package(slice.project()),
            ),
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
pub(crate) fn resource_controller_test_java(
    slice: &Slice,
    name: &str,
    extra: &str,
    fields: &[crate::generate::Field],
    sample: (&str, &[String]),
    key: &crate::generate::KeyType,
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
            crate::spring::mockmvc_template(
                slice.project(),
                crate::template_here!("spring/resource_controller_test_scoped_java.java"),
                crate::template_here!("spring/resource_controller_test_scoped_classic_java.java"),
            ),
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
        crate::spring::mockmvc_template(
            slice.project(),
            crate::template_here!("spring/resource_controller_test_java.java"),
            crate::template_here!("spring/resource_controller_test_classic_java.java"),
        ),
        &[
            ("pkg", pkg),
            ("extra", extra),
            ("key", &key.java),
            ("key_import", &key.import),
            ("absent", &key.samples.1),
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
pub(crate) fn resource_service_test_java(
    pkg: &str,
    name: &str,
    extra: &str,
    key: &crate::generate::KeyType,
) -> String {
    crate::template::render(
        crate::template_here!("spring/resource_service_test_java.java"),
        &[
            ("pkg", pkg),
            ("extra", extra),
            ("name", name),
            ("key", &key.java),
            ("key_import", &key.import),
            ("present", &key.samples.0),
            ("absent", &key.samples.1),
        ],
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
pub(crate) fn in_memory_repository_java(
    pkg: &str,
    name: &str,
    extra: &str,
    key: &crate::generate::StoredKey<'_>,
    is_bean: bool,
) -> String {
    let crate::generate::StoredKey {
        component,
        key_type,
        assigned: generated,
        rebuilt,
    } = key;
    let generated = *generated;
    let var = crate::generate::lower_first(name);
    let (find_by_id, delete_by_id, save_body, note) = match component {
        Some(field) => {
            let accessor = &field.name;
            // Keyed on the *repository's* key component, and stored as the
            // value rather than a rendering of it. Two things were wrong
            // before: this keyed on `id` while the JDBC adapter's `where`
            // clause keyed on the declared `@pk`, and it stringified the
            // value so `findById` could never match a typed lookup.
            let stored = if key_type.is_opaque() {
                format!("String.valueOf({var}.{accessor}())")
            } else {
                format!("{var}.{accessor}()")
            };
            // The fake has to assign what the database would, or it is not a
            // fake of this port: with a `generated always as identity` key
            // every caller hands in the same placeholder, so keying on it
            // would store one row forever. plan.md P4.2.
            let (save_body, note) = if generated {
                (
                    format!(
                        "        {key} assigned = next.incrementAndGet();\n\
                         \x20       {name} stored = {rebuilt};\n\
                         \x20       items.put(assigned, stored);\n\
                         \x20       return stored;",
                        key = key_type.java,
                        name = name,
                        rebuilt = rebuilt
                            .as_deref()
                            .expect("a generated key rebuilds the record"),
                    ),
                    format!(
                        " * <p>Keyed on the {{@code {accessor}}} component, which the database \
                         assigns.\n * This fake assigns it too -- from a counter -- because a \
                         caller hands in\n * a placeholder and expects the stored value back.\n"
                    ),
                )
            } else {
                (
                    format!("        items.put({stored}, {var});\n        return {var};"),
                    format!(
                        " * <p>Keyed on the {{@code {accessor}}} component -- the same one the \
                         JDBC\n * adapter's {{@code where}} clause uses.\n"
                    ),
                )
            };
            (
                "        return Optional.ofNullable(items.get(id));".to_string(),
                "        return items.remove(id) != null;".to_string(),
                save_body,
                note,
            )
        }
        // Fail loudly, the same way the JDBC adapter's composite-key arms
        // do. This branch used to answer `Optional.empty()` under a TODO and
        // `false` from a `remove` that could never match, over a `save` that
        // keyed on a counter -- three methods quietly doing the wrong thing
        // under a comment explaining why. `modern.md` §8.1 is what that reads
        // like from the outside. plan.md P7.1.
        //
        // Unreachable from `g scaffold`, which requires exactly one `@pk`;
        // this is what keeps it unreachable *loudly* if another caller
        // arrives.
        None => (
            format!(
                "        throw new UnsupportedOperationException(\n\
                 \x20               \"{name} declares no single key this port can take\");"
            ),
            format!(
                "        throw new UnsupportedOperationException(\n\
                 \x20               \"{name} declares no single key this port can take\");"
            ),
            format!(
                "        items.put(String.valueOf(items.size()), {var});\n        return {var};"
            ),
            " * <p>This type declares no single key this port can take, so {@code findById}\n\
             \x20* and {@code deleteById} throw rather than quietly answering empty and\n\
             \x20* {@code false} forever. {@code save} and {@code findAll} still work:\n\
             \x20* rows are keyed in insertion order, which is only safe because nothing\n\
             \x20* can remove one.\n"
                .to_string(),
        ),
    };
    // Exactly one adapter is the bean. When the JDBC one is, this is a fake
    // for tests and says so rather than pretending to be a stand-in for a
    // database that now exists.
    let (counter_field, counter_import) = if generated {
        (
            "    private final AtomicLong next = new AtomicLong();\n",
            "import java.util.concurrent.atomic.AtomicLong;\n",
        )
    } else {
        ("", "")
    };
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
            ("note", &*note),
            ("key", &key_type.java),
            ("key_import", &key_type.import),
            ("counter_field", counter_field),
            ("counter_import", counter_import),
            ("role_note", &*role_note),
            ("repository_annotation", repository_annotation),
            ("find_by_id", &*find_by_id),
            ("var", &*var),
            ("save_body", &*save_body),
            ("delete_by_id", &*delete_by_id),
        ],
    )
}
