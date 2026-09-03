//! The operation kinds -- command, query, transition, event, usecase -- and the
//! request boundary the compiler decides once for all of them.
//!
use super::*;

/// **A refusal surfaces on the command that caused it.**
///
/// `g usecase CreateNote --on Note` with no fields was accepted and written
/// into the model; `set`, `rename` and every `g` kept working; and the first
/// `add db` refused, several commands later, about a declaration the reader
/// had stopped thinking about. The insert emitter's rule is the exact one
/// but runs only for an entity with storage, so the linker takes the narrow
/// half: a command carrying nothing constructs nothing, whatever a compiler
/// with a database would later resolve.
#[test]
fn a_command_that_carries_nothing_is_refused_where_it_is_declared() {
    let root = jdl_project("command-constructs-nothing", NOTES_JDL);
    write_spring_fixture(&root);
    let scaffolded = jails_cmd(&root, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string"])
        .output()
        .unwrap();
    assert!(
        scaffolded.status.success(),
        "{}",
        String::from_utf8_lossy(&scaffolded.stderr)
    );
    let before = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();

    let refused = jails_cmd(&root, None)
        .args(["g", "usecase", "CreateNote", "--on", "Note"])
        .output()
        .unwrap();
    assert!(!refused.status.success(), "the command was accepted");
    let told = String::from_utf8_lossy(&refused.stderr);
    assert!(told.contains("model-command-constructs-nothing"), "{told}");
    assert!(told.contains("`note` requires `title`"), "{told}");
    assert!(told.contains("fix: carry `title` in the command"), "{told}");
    assert_eq!(
        fs::read_to_string(root.join(".jails/model.jdl")).unwrap(),
        before,
        "a refusal writes nothing"
    );

    // Carrying the field is accepted, and that is still the emitter's
    // judgement field by field once there is a database.
    let accepted = jails_cmd(&root, None)
        .args(["g", "usecase", "CreateNote", "title:string", "--on", "Note"])
        .output()
        .unwrap();
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
}

#[test]
fn scoped_execution_context_survives_evolution_and_binds_tenant_at_runtime() {
    let root = jdl_project(
        "jdl-v1-scoped-execution-context",
        r#"jdl 1
app Work {
  pkg com.example.work
  java 26
  platform spring
  build maven
  storage postgres
}

cap api
cap security

entity Task {
  use scaffold
  id: uuid @pk
  tenantId: uuid @scope(claim: "tenant")
  title: string
  version: long @version @nonnegative
  updatedAt: instant @default(now()) @updated

  command Create(title) {
    route POST "/tasks"
  }

  query All() {
    route GET "/tasks"
  }

  transition Rename(version, title) {
    update [title]
    if-match required
    route PATCH "/tasks/{id}"
  }
}
"#,
    );
    write_spring_fixture(&root);
    apply_canonical_model(&root, "scoped-context");

    let generated = root.join("src/main/java/com/example/work");
    let context = generated.join("application/ExecutionContext.java");
    let query = generated.join("adapters/jdbc/JdbcAllQuery.java");
    let transition = generated.join("adapters/jdbc/JdbcRenameTransition.java");
    let controller = generated.join("adapters/http/CreateController.java");
    for path in [&context, &query, &transition, &controller] {
        assert!(path.is_file(), "missing {}", path.display());
    }
    assert!(
        fs::read_to_string(&query)
            .unwrap()
            .contains("tenant_id = :scope_tenant_id")
    );
    assert!(
        fs::read_to_string(&transition)
            .unwrap()
            .contains("tenant_id = :scope_tenant_id")
    );
    let controller_source = fs::read_to_string(&controller).unwrap();
    assert!(
        controller_source
            .contains("Map.entry(\"tenant\", scopes.claim(authentication, \"tenant\"))"),
        "{controller_source}"
    );

    let context_source = fs::read_to_string(&context).unwrap();
    let split = context_source.rfind("\n}").unwrap();
    fs::write(
        &context,
        format!(
            "{}\n\n    public String readerMarker() {{ return \"kept\"; }}{}",
            &context_source[..split],
            &context_source[split..]
        ),
    )
    .unwrap();
    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Task", "priority:int?"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    assert!(
        fs::read_to_string(&context)
            .unwrap()
            .contains("readerMarker()"),
        "regeneration lost a clean edit in the managed execution-context ABI"
    );
    assert!(
        fs::read_to_string(&query)
            .unwrap()
            .contains("select id, tenant_id, title, version, updated_at, priority from task"),
        "model evolution did not reach the scoped query"
    );

    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );

    if real_mvn_available() && real_java_supports_target_release() {
        let test_dir = common::generated(&root, "src/test/java/com/example/work");
        fs::create_dir_all(&test_dir).unwrap();
        fs::write(
            test_dir.join("ScopedExecutionTest.java"),
            r#"package com.example.work;

import com.example.work.adapters.http.CreateController;
import com.example.work.adapters.jdbc.JdbcAllQuery;
import com.example.work.application.ExecutionContext;
import com.example.work.application.commands.CreateCommand;
import com.example.work.application.queries.AllQuery;
import java.lang.reflect.Proxy;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.jupiter.api.Test;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.mock.env.MockEnvironment;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

class ScopedExecutionTest {

    @Test
    void httpAuthenticationBecomesManagedExecutionContext() {
        var tenant = UUID.randomUUID();
        var captured = new AtomicReference<ExecutionContext>();
        CreateCommand operation = (context, input) -> {
            captured.set(context);
            return null;
        };
        var environment = new MockEnvironment()
                .withProperty("app.security.dev.scopes.tenant", tenant.toString());
        var controller = new CreateController(operation, new ScopeAuthorizer(environment));

        controller.execute(new CreateCommand.Input("one"), null);

        assertEquals(tenant.toString(), captured.get().claim("tenant"));
    }

    @Test
    @SuppressWarnings({"unchecked", "rawtypes"})
    void queryAlwaysBindsTenantFromContext() {
        var tenant = UUID.randomUUID();
        var sql = new AtomicReference<String>();
        var scoped = new AtomicReference<Object>();
        JdbcClient.MappedQuerySpec<com.example.work.domain.Task> rows =
                (JdbcClient.MappedQuerySpec<com.example.work.domain.Task>) Proxy.newProxyInstance(
                        getClass().getClassLoader(),
                        new Class<?>[] {JdbcClient.MappedQuerySpec.class},
                        (proxy, method, arguments) -> {
                            if (method.getName().equals("list")) {
                                return List.of();
                            }
                            throw new UnsupportedOperationException(method.toString());
                        });
        var statement = (JdbcClient.StatementSpec) Proxy.newProxyInstance(
                getClass().getClassLoader(),
                new Class<?>[] {JdbcClient.StatementSpec.class},
                (proxy, method, arguments) -> {
                    if (method.getName().equals("param")) {
                        if ("scope_tenant_id".equals(arguments[0])) {
                            scoped.set(arguments[1]);
                        }
                        return proxy;
                    }
                    if (method.getName().equals("query")) {
                        return rows;
                    }
                    throw new UnsupportedOperationException(method.toString());
                });
        var jdbc = (JdbcClient) Proxy.newProxyInstance(
                getClass().getClassLoader(),
                new Class<?>[] {JdbcClient.class},
                (proxy, method, arguments) -> {
                    if (method.getName().equals("sql")) {
                        sql.set((String) arguments[0]);
                        return statement;
                    }
                    throw new UnsupportedOperationException(method.toString());
                });

        var result = new JdbcAllQuery(jdbc).execute(
                new ExecutionContext(Map.of("tenant", tenant.toString())),
                new AllQuery.Input());

        assertTrue(sql.get().contains("tenant_id = :scope_tenant_id"));
        assertEquals(tenant, scoped.get());
        assertEquals(List.of(), result);
    }
}
"#,
        )
        .unwrap();
        let path = real_path_without_mvnd();
        let tested = real_maven_cmd(&root, &path)
            .args(["-q", "-B", "-Dtest=ScopedExecutionTest", "test"])
            .output()
            .unwrap();
        assert!(
            tested.status.success(),
            "generated scoped execution path failed its real Maven test:\n{}\n{}",
            String::from_utf8_lossy(&tested.stdout),
            String::from_utf8_lossy(&tested.stderr)
        );
    }
}

#[test]
fn compiler_managed_create_values_stay_out_of_the_request_and_compile_end_to_end() {
    let root = jdl_project(
        "jdl-v1-command-default-ownership",
        r#"jdl 1
app Jobs {
  pkg com.example.jobs
  java 26
  platform spring
  build maven
  storage postgres
}

entity Job {
  use scaffold
  id: uuid @pk
  title: string @notBlank
  status: string
  version: long @version @nonnegative
  createdAt: instant @default(now())
  updatedAt: instant @updated

  command CreateJob(title) {
    set status = QUEUED
  }

  transition Archive() {
    set status = ARCHIVED
    if-match none
  }
}
"#,
    );
    write_spring_fixture(&root);
    apply_canonical_model(&root, "command-default-ownership");

    let generated = root.join("src/main/java/com/example/jobs");
    let command =
        fs::read_to_string(generated.join("application/commands/CreateJobCommand.java")).unwrap();
    assert!(command.contains("public record Input("), "{command}");
    assert!(command.contains("String title"), "{command}");
    for managed in [
        "UUID id",
        "long version",
        "Instant createdAt",
        "Instant updatedAt",
    ] {
        assert!(
            !command.contains(managed),
            "request exposed `{managed}`:\n{command}"
        );
    }

    let adapter =
        fs::read_to_string(generated.join("adapters/jdbc/JdbcCreateJobCommand.java")).unwrap();
    assert!(adapter.contains("TimeOrderedUuid.next()"), "{adapter}");
    assert!(
        adapter.contains(
            "insert into jobs (id, title, status, updated_at) values (:id, :title, 'QUEUED', current_timestamp) returning id, title, status, version, created_at, updated_at"
        ),
        "{adapter}"
    );
    assert!(!adapter.contains("param(\"status\""), "{adapter}");
    assert!(!adapter.contains("param(\"created_at\""), "{adapter}");
    assert!(!adapter.contains("param(\"version\""), "{adapter}");
    let transition =
        fs::read_to_string(generated.join("adapters/jdbc/JdbcArchiveTransition.java")).unwrap();
    assert!(
        transition.contains(
            "update jobs set status = 'ARCHIVED', version = version + 1, updated_at = current_timestamp where"
        ),
        "{transition}"
    );
    assert!(transition.contains("id = :id"), "{transition}");
    let uuid7 = generated.join("domain/TimeOrderedUuid.java");
    let mut helper = fs::read_to_string(&uuid7).unwrap();
    let closing = helper.rfind('}').unwrap();
    helper.insert_str(
        closing,
        "\n    public static String readerMarker() { return \"kept\"; }\n",
    );
    fs::write(&uuid7, helper).unwrap();
    let evolved = jails_cmd(&root, None)
        .args(["g", "field", "Job", "priority:int?"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    assert!(
        fs::read_to_string(&uuid7).unwrap().contains("readerMarker"),
        "regeneration lost a clean edit in compiler default support"
    );
    let evolved_adapter =
        fs::read_to_string(generated.join("adapters/jdbc/JdbcCreateJobCommand.java")).unwrap();
    assert!(evolved_adapter.contains("'QUEUED'"), "{evolved_adapter}");
    let evolved_transition =
        fs::read_to_string(generated.join("adapters/jdbc/JdbcArchiveTransition.java")).unwrap();
    assert!(
        evolved_transition.contains("status = 'ARCHIVED'"),
        "{evolved_transition}"
    );

    if real_mvn_available() && real_java_supports_target_release() {
        common::assert_main_sources_compile(
            &root,
            &real_path_without_mvnd(),
            "generated default-owning command",
        );
    }
}

#[test]
fn every_operation_kind_lowers_to_a_typed_managed_abi() {
    // One of every routed kind beside [`MODEL`]'s command, plus the event the
    // transition emits.
    let source = MODEL.replace(
        "  }\n}\n",
        "  }\n\n  \
         event NoteCreated(id, title) {\n  }\n\n  \
         query OpenNotes(title) {\n    order by [id]\n    limit 50\n    \
         route GET \"/notes\"\n  }\n\n  \
         transition RenameNote(title) {\n    select [id]\n    \
         update [title]\n    emit note_created\n    route PATCH \"/notes/{id}\"\n  }\n}\n",
    );
    assert_ne!(
        source, MODEL,
        "the entity block moved and this splice missed it"
    );
    let root = model_project("model-operation-abi", &source);
    let plan = root.join("plan.json");
    let planned = jails_cmd(&root, None)
        .args(["model", "plan", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        planned.status.success(),
        "{}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let applied = jails_cmd(&root, None)
        .args(["model", "apply", "--bundle"])
        .arg(&plan)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );

    let generated = root.join("src/main/java/com/example/notes");
    let query =
        fs::read_to_string(generated.join("application/queries/OpenNotesQuery.java")).unwrap();
    assert!(query.contains("String ROUTE = \"GET /notes\""), "{query}");
    assert!(query.contains("int DEFAULT_LIMIT = 50"), "{query}");
    assert!(query.contains("List<Note> execute(Input input)"), "{query}");

    let transition =
        fs::read_to_string(generated.join("application/transitions/RenameNoteTransition.java"))
            .unwrap();
    assert!(
        transition.contains("String ROUTE = \"PATCH /notes/{id}\""),
        "{transition}"
    );
    assert!(
        transition.contains("Note execute(UUID id, Input input)"),
        "{transition}"
    );

    let event = fs::read_to_string(generated.join("domain/events/NoteCreatedEvent.java")).unwrap();
    assert!(event.contains("public record NoteCreatedEvent"), "{event}");
    assert!(event.contains("UUID id"), "{event}");
    assert!(event.contains("String title"), "{event}");

    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
}

#[test]
fn familiar_operation_commands_edit_nested_jdl_and_compile_typed_abis() {
    let root = jdl_project("model-jdl-operation-frontends", NOTES_JDL);
    for arguments in [
        vec!["g", "scaffold", "Note", "id:uuid@pk", "title:string!"],
        vec!["g", "event", "NoteCreated", "id", "title", "--on", "Note"],
        vec![
            "g",
            "usecase",
            "CreateNote",
            "title",
            "--on",
            "Note",
            "--path",
            "/notes",
        ],
        vec![
            "g",
            "query",
            "OpenNotes",
            "title",
            "--on",
            "Note",
            "--limit",
            "50",
            "--path",
            "/notes/search",
        ],
        vec![
            "g",
            "transition",
            "RenameNote",
            "title",
            "--on",
            "Note",
            "--yields",
            "NoteCreated",
            "--path",
            "/notes/{id}",
            "--method",
            "patch",
        ],
    ] {
        let output = jails_cmd(&root, None).args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let jdl = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    for declaration in [
        "event NoteCreated(id, title)",
        "command CreateNote(title)",
        "query OpenNotes(title)",
        "transition RenameNote(title)",
        r#"route POST "/notes""#,
        r#"route GET "/notes/search""#,
        r#"route PATCH "/notes/{id}""#,
        "emit note_created",
    ] {
        assert!(jdl.contains(declaration), "missing `{declaration}`:\n{jdl}");
    }
    let generated = root.join("src/main/java/com/example/notes");
    for relative in [
        "domain/events/NoteCreatedEvent.java",
        "application/commands/CreateNoteCommand.java",
        "application/queries/OpenNotesQuery.java",
        "application/transitions/RenameNoteTransition.java",
    ] {
        assert!(generated.join(relative).is_file(), "missing {relative}");
    }
    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
}

#[test]
fn operation_frontend_refuses_field_type_drift_without_writing() {
    let root = model_project("model-operation-field-drift", EMPTY_MODEL);
    let scaffold = jails_cmd(&root, None)
        .args(["g", "scaffold", "Note", "id:uuid@pk", "title:string!"])
        .output()
        .unwrap();
    assert!(scaffold.status.success());
    let before = snapshot_tree(&root);
    let refused = jails_cmd(&root, None)
        .args(["g", "query", "OpenNotes", "title:int", "--on", "Note"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let stderr = String::from_utf8(refused.stderr).unwrap();
    assert!(stderr.contains("disagrees with entity field"), "{stderr}");
    assert!(stderr.contains("fix:"), "{stderr}");
    assert_eq!(snapshot_tree(&root), before, "refusal mutated the project");
}

#[test]
fn canonical_api_adapters_merge_then_eject_at_the_operation_boundary() {
    // [`MODEL`]'s command, and a query and a transition beside it, so the
    // `api` capability has one operation of each routed kind to adapt.
    let source = MODEL.replace(
        "  }\n}\n",
        "  }\n\n  \
         query OpenNotes(title) {\n    route GET \"/notes/search\"\n  }\n\n  \
         transition RenameNote(title) {\n    select [id]\n    \
         update [title]\n    route PATCH \"/notes/{id}\"\n  }\n}\n",
    );
    assert_ne!(
        source, MODEL,
        "the entity block moved and this splice missed it"
    );
    let root = model_project("model-api-operation-adapters", &source);
    write_spring_fixture(&root);

    // `db` first: an operation's controller takes a port that only the JDBC
    // adapter implements, so an `api` project without storage compiles and
    // fails to start.
    assert!(
        jails_cmd(&root, None)
            .args(["add", "db", "--no-start"])
            .status()
            .unwrap()
            .success()
    );
    let added = jails_cmd(&root, None)
        .args(["add", "api"])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let generated = root.join("src/main/java/com/example/notes");
    let command = generated.join("adapters/http/CreateNoteController.java");
    let query =
        fs::read_to_string(generated.join("adapters/http/OpenNotesController.java")).unwrap();
    let transition =
        fs::read_to_string(generated.join("adapters/http/RenameNoteController.java")).unwrap();
    assert!(
        fs::read_to_string(&command)
            .unwrap()
            .contains("RequestMethod.POST")
    );
    assert!(query.contains("@ModelAttribute OpenNotesQuery.Input input"));
    assert!(query.contains("RequestMethod.GET"));
    assert!(
        transition.contains("@PathVariable(\"id\") UUID id"),
        "{transition}"
    );
    assert!(transition.contains("RequestMethod.PATCH"), "{transition}");
    assert!(
        fs::read_to_string(root.join("pom.xml"))
            .unwrap()
            .contains("spring-boot-starter-web")
    );

    let contents = fs::read_to_string(&command).unwrap();
    let split = contents.rfind("\n}").unwrap();
    fs::write(
        &command,
        format!(
            "{}\n\n    public String handWritten() {{ return \"reader\"; }}{}",
            &contents[..split],
            &contents[split..]
        ),
    )
    .unwrap();
    let evolved = jails_cmd(&root, None)
        .args(["entity", "field", "add", "Note", "summary:string?"])
        .output()
        .unwrap();
    assert!(
        evolved.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved.stderr)
    );
    assert!(
        fs::read_to_string(&command)
            .unwrap()
            .contains("handWritten")
    );

    let ejected = jails_cmd(&root, None)
        .args(["model", "eject", "art_op_create_note_http"])
        .output()
        .unwrap();
    assert!(
        ejected.status.success(),
        "{}",
        String::from_utf8_lossy(&ejected.stderr)
    );
    let reader =
        root.join("src/main/java/com/example/notes/adapters/http/CreateNoteController.java");
    assert!(fs::read_to_string(&reader).unwrap().contains("handWritten"));
    assert!(
        generated
            .join("application/commands/CreateNoteCommand.java")
            .is_file(),
        "ejecting the controller must not eject its managed ABI"
    );
    let reader_bytes = fs::read(&reader).unwrap();
    let evolved_again = jails_cmd(&root, None)
        .args(["entity", "field", "add", "Note", "priority:int?"])
        .output()
        .unwrap();
    assert!(
        evolved_again.status.success(),
        "{}",
        String::from_utf8_lossy(&evolved_again.stderr)
    );
    assert_eq!(fs::read(&reader).unwrap(), reader_bytes);

    let frozen = jails_cmd(&root, None)
        .args(["model", "check", "--frozen"])
        .output()
        .unwrap();
    assert!(
        frozen.status.success(),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
    if real_mvn_available() && real_java_supports_target_release() {
        let path = real_path_without_mvnd();
        common::assert_main_sources_compile(&root, &path, "canonical API adapters");
    }
}

/// A transition's field roles come from `select`/`update`/`emit` and nowhere
/// else, and every emitted event is published.
///
/// The four assertions are the four roles JDL v1 §12.4 separates: `id`
/// selects, `version` guards and is incremented by the compiler, `status`
/// updates, and both events publish. It reads the emitted SQL rather than the
/// model, because the property is what reaches Java.
#[test]
fn a_transition_separates_selector_guard_and_update_and_emits_every_event() {
    let root = jdl_project(
        "model-transition-roles",
        r#"jdl 1

app Roles {
  pkg com.example.roles
  java 26
  platform spring
  build maven
  storage postgres
}

entity Task {
  use scaffold

  id:      uuid   @pk
  title:   string @notBlank
  status:  string
  version: long   @version @nonnegative

  transition Close(id, version, status) {
    select [id]
    if-match required
    emit TaskClosed
    emit TaskAudited
  }

  event TaskClosed(id, status) {}
  event TaskAudited(id, status) {}
}
"#,
    );
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/roles/adapters/jdbc/JdbcCloseTransition.java"),
    )
    .unwrap();
    assert!(
        adapter.contains(r#""id = :id""#),
        "the primary key selects the row: {adapter}"
    );
    assert!(
        adapter.contains(r#""version = :expected_version""#),
        "a version parameter guards rather than updates: {adapter}"
    );
    assert!(
        adapter.contains("set status = :status, version = version + 1"),
        "only `status` is written from input, and the compiler owns the version: {adapter}"
    );
    assert!(
        !adapter.contains("id = :id, ") && !adapter.contains("set id ="),
        "the selector is never an update target: {adapter}"
    );
    assert!(
        adapter.contains("new TaskClosedEvent(") && adapter.contains("new TaskAuditedEvent("),
        "every emitted event is published, not just the first: {adapter}"
    );
}

/// A command publishes the events it declares: `command Create(...) { emit
/// TaskCreated }` connects the payload record to the adapter. Commands and
/// transitions go through one `publications` helper so the rule cannot get
/// two answers.
#[test]
fn a_command_publishes_every_event_it_declares() {
    let root = jdl_project(
        "model-command-emits",
        r#"jdl 1

app Cmd {
  pkg com.example.cmd
  java 26
  platform spring
  build maven
  storage postgres
}

entity Task {
  use scaffold

  id:     uuid   @pk
  title:  string @notBlank
  status: string

  command Create(title, status) {
    emit TaskCreated
    emit TaskAudited
  }

  event TaskCreated(id, status) {}
  event TaskAudited(id, status) {}
}
"#,
    );
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/cmd/adapters/jdbc/JdbcCreateCommand.java"),
    )
    .unwrap();
    assert!(
        adapter.contains("new TaskCreatedEvent(result.id(), result.status())")
            && adapter.contains("new TaskAuditedEvent(result.id(), result.status())"),
        "both declared events publish, from the row the database returned: {adapter}"
    );
    assert!(
        adapter.contains("ApplicationEventPublisher events"),
        "the publisher is injected only where it is used: {adapter}"
    );
}

/// A declared sort direction reaches the SQL: `order by [createdAt desc, id]`
/// compiles to `order by created_at desc, id`, so a query declared
/// newest-first returns newest-first.
#[test]
fn a_query_orders_by_the_direction_it_declares() {
    let root = jdl_project(
        "model-query-ordering",
        r#"jdl 1

app Ord {
  pkg com.example.ord
  java 26
  platform spring
  build maven
  storage postgres
}

entity Task {
  use scaffold

  id:        uuid    @pk
  title:     string  @notBlank
  createdAt: instant @default(now())

  query Recent(title?) {
    order by [createdAt desc, id]
    limit 25
  }
}
"#,
    );
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).args(["sync"]).output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let adapter = fs::read_to_string(
        root.join("src/main/java/com/example/ord/adapters/jdbc/JdbcRecentQuery.java"),
    )
    .unwrap();
    assert!(
        adapter.contains(r#"" order by created_at desc, id""#),
        "`desc` survives and `asc` stays implicit: {adapter}"
    );
    assert!(
        adapter.contains(r#"" limit 25""#),
        "the declared ceiling survives: {adapter}"
    );
}

/// Five operation flags the frontend translates into the model: `set x = 1`,
/// `select [a]`, `if-match optional`, `bind p from form "wire"` and `consumes
/// form`, which `TransitionSemantics` carries as `select`, `assignments` and
/// `precondition`.
///
/// The selector is subtracted from the update, which is the one part that
/// is not a straight pass-through: a transition does not write the column it
/// selects by, and naming a primary key in both is what the compiler refuses
/// as rewriting a key.
#[test]
fn a_canonical_transition_carries_its_selector_pins_and_precondition() {
    let root = jdl_project(
        "jdl-v1-transition-flags",
        r#"jdl 1
app Shop {
 pkg com.example.shop
 java 26
 platform spring
 build maven
 storage postgres
}
entity Note {
 id: uuid @pk
 seen: boolean
 version: long @version
 body: string
 use repo
}
"#,
    );
    // The pom the canonical JDBC adapters need: `storage postgres` renders
    // Spring JDBC, and refuses without a captured Spring Boot project.
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    let applied = jails_cmd(&root, None)
        .args([
            "g",
            "transition",
            "MarkSeen",
            "id:uuid",
            "seen:boolean",
            "version:long",
            "--on",
            "Note",
            "--select",
            "id",
            "--set",
            "seen=true",
            "--if-match",
            "optional",
            "--consumes",
            "form",
            "--bind",
            "id=note_id",
        ])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "the five flags should reach the model rather than be refused:\n{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    for expected in [
        "select [id]",
        "set seen = true",
        "if-match optional",
        "bind id from form \"note_id\"",
        "consumes form",
    ] {
        assert!(
            model.contains(expected),
            "`{expected}` is missing from the model:\n{model}"
        );
    }
    // Subtracted, not merely listed: `id` selects the row and `seen` is
    // pinned, so neither is a caller-supplied update. The linker refuses the
    // overlap by name, which is what makes the three roles distinct.
    assert!(
        !model.contains("update ["),
        "with the selector pinned and the rest managed there is nothing to update:\n{model}"
    );
}

/// Four operation flags the frontend translates into nodes the model, the JDL
/// grammar and the compiler support -- `--via`, `--order-by`, an optional
/// filter and `--on-conflict` -- asked for together, as a proof-application
/// manifest does.
#[test]
fn a_canonical_query_carries_its_join_ordering_and_optional_filters() {
    let root = jdl_project(
        "jdl-v1-operation-flags",
        r#"jdl 1
app Post {
 pkg com.example.post
 java 26
 platform spring
 build maven
 storage postgres
}
enum Channel {
 EMAIL
 SMS
}
entity Sender {
 id: uuid @pk
 email: string
 use repo
}
entity Note {
 id: uuid @pk
 senderId: uuid
 body: string
 channel: Channel
 use repo
}
"#,
    );
    write_spring_fixture(&root);
    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    // `--via` is a join, and its filter may name a column of the *joined*
    // entity: that is what the flag is for.
    let via = jails_cmd(&root, None)
        .args([
            "g", "query", "ByEmail", "email", "--on", "Note", "--via", "Sender",
        ])
        .output()
        .unwrap();
    assert!(
        via.status.success(),
        "`--via` should reach the model's own join:\n{}",
        String::from_utf8_lossy(&via.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("join Sender as sender on sender_id -> sender.id"),
        "the join is derived from the two entities:\n{model}"
    );
    assert!(
        model.contains("sender.email"),
        "and a joined filter is qualified by the alias:\n{model}"
    );

    // `--order-by` keeps its direction, and `?` marks an optional filter
    // rather than a nullable column.
    let listing = jails_cmd(&root, None)
        .args([
            "g",
            "query",
            "Recent",
            "channel:Channel?",
            "--on",
            "Note",
            "--order-by",
            "id desc",
            "--limit",
            "20",
        ])
        .output()
        .unwrap();
    assert!(
        listing.status.success(),
        "`--order-by ... desc` and an optional filter should both translate:\n{}",
        String::from_utf8_lossy(&listing.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("order by [id desc]"),
        "the direction rides beside the field:\n{model}"
    );
    assert!(
        model.contains("channel?"),
        "and `?` is the optional-filter marker the grammar already parsed:\n{model}"
    );

    // `--on-conflict` is the retained-result insert.
    let upsert = jails_cmd(&root, None)
        .args([
            "g",
            "usecase",
            "EnsureSender",
            "email",
            "--on",
            "Sender",
            "--on-conflict",
            "email",
        ])
        .output()
        .unwrap();
    assert!(
        upsert.status.success(),
        "`--on-conflict` should reach `conflict_key`:\n{}",
        String::from_utf8_lossy(&upsert.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(
        model.contains("conflict on [email]"),
        "the conflict key is a declaration, not a flag jails drops:\n{model}"
    );

    // Declaring the same association twice is a no-op, not a duplicate
    // relation the parser then refuses.
    for _ in 0..2 {
        let association = jails_cmd(&root, None)
            .args([
                "g",
                "association",
                "NoteSender",
                "--on",
                "Note",
                "--yields",
                "Sender",
                "senderId=id",
            ])
            .output()
            .unwrap();
        assert!(
            association.status.success(),
            "re-declaring an association must be idempotent:\n{}",
            String::from_utf8_lossy(&association.stderr)
        );
    }
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert_eq!(
        model.matches("relation noteSender").count(),
        1,
        "and must not append a second relation under the same name:\n{model}"
    );
}

/// `g usecase --yields <Event>` writes both lines an outbox needs.
///
/// The flag is a delivery policy, not just an event: `--yields` asks for a
/// transactional outbox, so writing `emit E` alone would honour it with
/// direct publication -- a write and a publish that can fail independently --
/// under a flag that asked for the stronger guarantee. That substitution is
/// exactly what `deliver` exists to make impossible, so it is what this pins.
#[test]
fn canonical_usecase_yields_writes_an_outbox_delivery_policy() {
    let root = temp_dir("canonical-usecase-yields");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage postgres\n}\n",
    )
    .unwrap();
    for command in [
        ["g", "scaffold", "Task", "id:uuid@pk", "title:string!"].as_slice(),
        // The store encodes the staged payload with `Json`, so the capability
        // that writes it is a prerequisite.
        ["add", "json"].as_slice(),
        // `id: uuid` rather than `id`: the event's own identity, minted, not
        // the row's. An outbox staged on the row's id makes
        // `on conflict (id) do nothing` discard the second event about it.
        [
            "g",
            "event",
            "TaskCreated",
            "id:uuid",
            "title",
            "--on",
            "Task",
        ]
        .as_slice(),
    ] {
        let output = jails_cmd(&root, None).args(command).output().unwrap();
        assert!(
            output.status.success(),
            "`jails {}`: {}",
            command.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let generated = jails_cmd(&root, None)
        .args([
            "g",
            "usecase",
            "CreateTask",
            "title",
            "--on",
            "Task",
            "--yields",
            "TaskCreated",
        ])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let model = fs::read_to_string(root.join(".jails/model.jdl")).unwrap();
    assert!(model.contains("emit task_created"), "{model}");
    assert!(model.contains("deliver outbox"), "{model}");
    // And the policy is honoured rather than recorded: the adapter stages.
    let command = fs::read_to_string(
        root.join("src/main/java/com/example/demo/adapters/jdbc/JdbcCreateTaskCommand.java"),
    )
    .unwrap();
    assert!(command.contains("outbox.stage("), "{command}");
    assert!(command.contains("@Transactional"), "{command}");
}

/// `deliver outbox` end to end: the model says it, the project has it.
///
/// The compiler's own tests pin what is rendered; this pins what reaches disk,
/// and one property only the executor can show -- **the table is written
/// once**. A migration is irreproducible, so a second `sync` that re-emitted
/// `create <name>_outbox` would leave a project that was working yesterday
/// failing its next `flyway migrate`, and nothing between here and there says
/// so.
#[test]
fn canonical_outbox_delivery_reaches_disk_and_its_table_is_written_once() {
    let root = temp_dir("canonical-outbox-sync");
    write_spring_fixture(&root);
    fs::create_dir_all(root.join(".jails")).unwrap();
    fs::write(
        root.join(".jails/model.jdl"),
        "jdl 1\napp Demo @id(project_demo) {\n  pkg com.example.demo\n  java 26\n  \
         platform spring\n  build maven\n  storage postgres\n}\n\ncap json\n\n\
         entity Task {\n  use repo\n  id: uuid @pk\n  title: string @notBlank\n\n  \
         command Create(title) {\n    emit TaskCreated\n    deliver outbox\n  }\n\n  \
         event TaskCreated(id: uuid, title)\n}\n",
    )
    .unwrap();

    let synced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );

    let generated = root.join("src/main/java/com/example/demo");
    let read = |relative: &str| {
        fs::read_to_string(generated.join(relative))
            .unwrap_or_else(|error| panic!("{relative}: {error}"))
    };
    // The staging happens in the transaction that made the row, which is the
    // entire guarantee; a store, a port and a relay to carry it out of there.
    let command = read("adapters/jdbc/JdbcCreateCommand.java");
    assert!(command.contains("@Transactional"), "{command}");
    assert!(
        command.contains("outbox.stage(new TaskCreatedEvent("),
        "{command}"
    );
    read("jobs/JdbcCreateOutbox.java");
    read("jobs/CreateOutboxSink.java");
    read("jobs/CreateLoggingOutboxSink.java");
    read("jobs/CreateOutboxWorker.java");
    read("jobs/SchedulingConfig.java");
    // The minted identity, and the class that mints it.
    read("domain/TimeOrderedUuid.java");
    let event = read("domain/events/TaskCreatedEvent.java");
    assert!(event.contains("UUID id"), "{event}");

    let migrations = || {
        let mut names = fs::read_dir(root.join("src/main/resources/db/migration"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        names.sort();
        names
    };
    let after_first = migrations();
    assert_eq!(
        after_first
            .iter()
            .filter(|name| name.contains("create_outbox"))
            .count(),
        1,
        "{after_first:?}"
    );

    let resynced = jails_cmd(&root, None).arg("sync").output().unwrap();
    assert!(
        resynced.status.success(),
        "{}",
        String::from_utf8_lossy(&resynced.stderr)
    );
    assert_eq!(migrations(), after_first, "sync re-emitted a migration");
}
