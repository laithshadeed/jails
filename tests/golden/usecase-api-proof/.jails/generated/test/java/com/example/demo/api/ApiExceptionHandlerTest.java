package com.example.demo.api;

import static org.assertj.core.api.Assertions.assertThat;

import java.util.List;
import org.junit.jupiter.api.Test;
import org.springframework.dao.DuplicateKeyException;
import org.springframework.dao.EmptyResultDataAccessException;
import org.springframework.dao.OptimisticLockingFailureException;
import org.springframework.http.HttpStatus;
import org.springframework.test.web.servlet.assertj.MockMvcTester;
import org.springframework.test.web.servlet.setup.StandaloneMockMvcBuilder;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;

/**
 * Drives the advice through a standalone MockMvc rather than a
 * {@code @SpringBootTest}: no application context, no database, no port, so
 * it runs in milliseconds and keeps failing for exactly one reason.
 *
 * <p>The controller below exists only to throw. Testing the advice against
 * the real controllers would couple this test to whatever they happen to do
 * today.
 */
class ApiExceptionHandlerTest {

    private final MockMvcTester mvc =
            MockMvcTester.of(
                    List.of(new ThrowingController()),
                    builder -> builder.setControllerAdvice(new ApiExceptionHandler()).build());

    @Test
    void aMissingThingBecomesA404Problem() {
        assertThat(mvc.get().uri("/boom/not-found"))
                .hasStatus(HttpStatus.NOT_FOUND)
                .bodyJson()
                .extractingPath("$.detail")
                .isEqualTo("no such thing");
    }

    @Test
    void aConflictBecomesA409() {
        assertThat(mvc.get().uri("/boom/conflict")).hasStatus(HttpStatus.CONFLICT);
    }

    @Test
    void aDuplicateKeyBecomesA409() {
        // The database rejected a unique constraint; that is a conflict, not
        // a server fault.
        assertThat(mvc.get().uri("/boom/duplicate")).hasStatus(HttpStatus.CONFLICT);
    }

    @Test
    void aStalePreconditionBecomesA412() {
        assertThat(mvc.get().uri("/boom/stale")).hasStatus(HttpStatus.PRECONDITION_FAILED);
    }

    @Test
    void aMissingRowBecomesA404() {
        assertThat(mvc.get().uri("/boom/missing")).hasStatus(HttpStatus.NOT_FOUND);
    }

    @Test
    void aDomainRejectionBecomesA422() {
        // 422, not 400: the request was read successfully and the domain said
        // no. A 400 would tell the client to fix its syntax.
        assertThat(mvc.get().uri("/boom/rejected"))
                .hasStatus(HttpStatus.UNPROCESSABLE_ENTITY);
    }

    @RestController
    static class ThrowingController {

        @GetMapping("/boom/not-found")
        String notFound() {
            throw new ApiException.NotFound("no such thing");
        }

        @GetMapping("/boom/conflict")
        String conflict() {
            throw new ApiException.Conflict("already exists");
        }

        @GetMapping("/boom/duplicate")
        String duplicate() {
            throw new DuplicateKeyException("unique constraint violated");
        }

        @GetMapping("/boom/stale")
        String stale() {
            throw new OptimisticLockingFailureException("version moved on");
        }

        @GetMapping("/boom/missing")
        String missing() {
            throw new EmptyResultDataAccessException(1);
        }

        @GetMapping("/boom/rejected")
        String rejected() {
            throw new ApiException.Rejected("amount must be positive");
        }
    }
}
