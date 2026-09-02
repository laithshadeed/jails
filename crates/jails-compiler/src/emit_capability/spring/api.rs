//! The error model, and the arms it maps.
//!
//! Every other pack in `spring.rs` declares files and dependencies; this one
//! additionally decides
//! *which failures become which status*, and each arm is conditional on a
//! capability whose types it names -- `DuplicateKeyException` and the
//! `spring-dao` precondition pair only exist once the JDBC starter is in the
//! build. A fragment that substitutes to nothing on a project without it is
//! how an advice file stays compilable, and it is the whole content of this
//! module.

use super::*;

pub(in crate::emit_capability) const API_FILES: &[JavaFile<Capability>] = &[
    JavaFile {
        role: "exception",
        template: crate::template!("spring/api_exception_java.java"),
        before_boot: None,
        imports: &[],
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("ApiException"),
        template_class: Naming::Fixed("ApiException"),
    },
    JavaFile {
        role: "exception_handler",
        template: crate::template!("spring/api_exception_handler_java.java"),
        before_boot: None,
        imports: &[],
        source_set: SourceSet::Main,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("ApiExceptionHandler"),
        template_class: Naming::Fixed("ApiExceptionHandler"),
    },
    JavaFile {
        role: "exception_handler_test",
        // No classic form: `api` refuses below Boot 3, its advice being built
        // on Framework 6's `ProblemDetail`.
        template: crate::template!("spring/api_exception_handler_test_java.java"),
        before_boot: None,
        imports: &[],
        source_set: SourceSet::Test,
        placement: Placement::Default,
        ejectable: true,
        class: Naming::Fixed("ApiExceptionHandlerTest"),
        template_class: Naming::Fixed("ApiExceptionHandlerTest"),
    },
];

pub(in crate::emit_capability) const API_FRAGMENTS: &[Fragment<Capability>] = &[
    Fragment::WhenCapability {
        key: "duplicate_key_import",
        capability: "db",
        body: "import org.springframework.dao.DuplicateKeyException;",
    },
    Fragment::WhenCapability {
        key: "duplicate_key_handler",
        capability: "db",
        body: DUPLICATE_KEY_HANDLER,
    },
    Fragment::WhenCapability {
        key: "duplicate_key_test",
        capability: "db",
        body: DUPLICATE_KEY_TEST,
    },
    Fragment::WhenCapability {
        key: "duplicate_key_route",
        capability: "db",
        body: DUPLICATE_KEY_ROUTE,
    },
    Fragment::WhenCapability {
        key: "precondition_import",
        capability: "db",
        body: "import org.springframework.dao.EmptyResultDataAccessException;\nimport org.springframework.dao.OptimisticLockingFailureException;",
    },
    Fragment::WhenCapability {
        key: "precondition_handler",
        capability: "db",
        body: PRECONDITION_HANDLER,
    },
    Fragment::WhenCapability {
        key: "precondition_test",
        capability: "db",
        body: PRECONDITION_TEST,
    },
    Fragment::WhenCapability {
        key: "precondition_route",
        capability: "db",
        body: PRECONDITION_ROUTE,
    },
];

/// **Spring's own vocabulary, so nothing new has to be declared.** A
/// transition whose `If-Match` did not match raises
/// `OptimisticLockingFailureException` and one whose row is not there raises
/// `EmptyResultDataAccessException` -- both from `spring-dao`, both already on
/// the classpath the moment the JDBC starter is. Mapping them here rather than
/// in each controller is what keeps a generated controller free of HTTP status
/// arithmetic, and what makes a hand-written adapter get the same answer.
const PRECONDITION_HANDLER: &str = r#"
    /**
     * A precondition the caller stated and the row no longer satisfies.
     *
     * <p>412 rather than 409: the caller sent an `If-Match` and it did not
     * match, which is precisely what 412 means. A 500 here is the worse
     * failure it replaces -- alerting pages on it, client libraries retry it,
     * and the retry cannot succeed because the version has moved on.
     */
    @ExceptionHandler(OptimisticLockingFailureException.class)
    public ProblemDetail handleStalePrecondition(OptimisticLockingFailureException failure) {
        return ProblemDetail.forStatusAndDetail(
                HttpStatus.PRECONDITION_FAILED,
                "the resource has changed since the version you sent");
    }

    /**
     * A row the request named and the database does not have.
     *
     * <p>The detail says nothing about which row: an unauthenticated caller
     * learning that an id exists is the difference between 404 and 403, and
     * a generated handler is the wrong place to decide that.
     */
    @ExceptionHandler(EmptyResultDataAccessException.class)
    public ProblemDetail handleMissingRow(EmptyResultDataAccessException failure) {
        return ProblemDetail.forStatusAndDetail(HttpStatus.NOT_FOUND, "no such resource");
    }
"#;

const PRECONDITION_TEST: &str = r#"
    @Test
    void aStalePreconditionBecomesA412() {
        assertThat(mvc.get().uri("/boom/stale")).hasStatus(HttpStatus.PRECONDITION_FAILED);
    }

    @Test
    void aMissingRowBecomesA404() {
        assertThat(mvc.get().uri("/boom/missing")).hasStatus(HttpStatus.NOT_FOUND);
    }
"#;

const PRECONDITION_ROUTE: &str = r#"
        @GetMapping("/boom/stale")
        String stale() {
            throw new OptimisticLockingFailureException("version moved on");
        }

        @GetMapping("/boom/missing")
        String missing() {
            throw new EmptyResultDataAccessException(1);
        }
"#;

const DUPLICATE_KEY_HANDLER: &str = r#"
    /**
     * A unique constraint the database enforced, as the 409 it is.
     *
     * <p>Without this, a duplicate reaches the client as a 500 -- which is
     * what alerting pages on and what a client library retries, so one
     * duplicate becomes an incident and then a retry storm. The row was not
     * written and never will be; that is a conflict, not a server fault.
     *
     * <p>The detail deliberately does not name the column. Spring's message
     * carries the constraint name from the driver, which is a schema
     * identifier rather than anything a caller can act on -- and echoing it
     * tells an unauthenticated client the shape of your database.
     */
    @ExceptionHandler(DuplicateKeyException.class)
    public ProblemDetail handleDuplicateKey(DuplicateKeyException failure) {
        return ProblemDetail.forStatusAndDetail(
                HttpStatus.CONFLICT, "a resource with those values already exists");
    }
"#;

const DUPLICATE_KEY_TEST: &str = r#"
    @Test
    void aDuplicateKeyBecomesA409() {
        // The database rejected a unique constraint; that is a conflict, not
        // a server fault.
        assertThat(mvc.get().uri("/boom/duplicate")).hasStatus(HttpStatus.CONFLICT);
    }
"#;

const DUPLICATE_KEY_ROUTE: &str = r#"
        @GetMapping("/boom/duplicate")
        String duplicate() {
            throw new DuplicateKeyException("unique constraint violated");
        }
"#;
