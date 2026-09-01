package {{pkg}};

import java.util.List;
import org.junit.jupiter.api.Test;
import org.springframework.http.HttpStatus;
{{duplicate_key_import}}
{{precondition_import}}
import org.springframework.test.web.servlet.assertj.MockMvcTester;
import org.springframework.test.web.servlet.setup.StandaloneMockMvcBuilder;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;

import static org.assertj.core.api.Assertions.assertThat;

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

{{duplicate_key_test}}
{{precondition_test}}
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

{{duplicate_key_route}}
{{precondition_route}}
        @GetMapping("/boom/rejected")
        String rejected() {
            throw new ApiException.Rejected("amount must be positive");
        }
    }
}
