package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

@SpringBootTest
@AutoConfigureMockMvc
class HealthControllerTest {

    @Autowired
    private MockMvcTester mvc;

    @Test
    void getReturnsOk() {
        assertThat(mvc.get().uri("/health"))
                .hasStatusOk()
                .bodyText()
                .isEqualTo("Health");
    }
}
