package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.BDDMockito.given;
import static org.mockito.Mockito.mock;

import com.example.demo.service.ArticleService;
import java.util.List;
import java.util.Optional;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class ArticleControllerTest {

    private ArticleService service;
    private MockMvcTester mvc;

    @BeforeEach
    void setUp() {
        service = mock(ArticleService.class);
        mvc = MockMvcTester.of(new ArticleController(service));
    }

    @Test
    void anEmptyCollectionIsAnEmptyArray() {
        given(service.findAll()).willReturn(List.of());

        assertThat(mvc.get().uri(ArticleController.PATH))
                .hasStatusOk()
                .bodyJson()
                .isEqualTo("[]");
    }

    @Test
    void aMissingItemIs404() {
        given(service.findById("nope")).willReturn(Optional.empty());

        assertThat(mvc.get().uri(ArticleController.PATH + "/nope")).hasStatus(404);
    }

    @Test
    void aDeleteThatRemovedNothingIs404() {
        given(service.deleteById("nope")).willReturn(false);

        assertThat(mvc.delete().uri(ArticleController.PATH + "/nope")).hasStatus(404);
    }
}
