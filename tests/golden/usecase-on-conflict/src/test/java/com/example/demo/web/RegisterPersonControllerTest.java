package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Person;
import com.example.demo.service.RegisterPersonCommand;
import com.example.demo.service.RegisterPersonUseCase;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class RegisterPersonControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new RegisterPersonController(
            command -> new Person(
                    UUID.fromString("00000000-0000-0000-0000-000000000001"),
                    "sample",
                    Instant.parse("2024-01-01T00:00:00Z"))));

    @Test
    void postExecutesTheUseCase() {
        assertThat(mvc.post()
                .uri(RegisterPersonController.PATH)
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "email": "sample"
}
"""))
                .hasStatus(201);
    }

}
