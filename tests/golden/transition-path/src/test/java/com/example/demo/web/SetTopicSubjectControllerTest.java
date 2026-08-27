package com.example.demo.web;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Topic;
import com.example.demo.service.SetTopicSubjectCommand;
import com.example.demo.service.SetTopicSubjectUseCase;
import org.junit.jupiter.api.Test;
import org.springframework.http.HttpHeaders;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.assertj.MockMvcTester;

class SetTopicSubjectControllerTest {

    private final MockMvcTester mvc = MockMvcTester.of(new SetTopicSubjectController(
            (userId, command, expectedVersion) -> new SetTopicSubjectUseCase.Result.Applied(new Topic(
                    1L,
                    1L,
                    "sample",
                    1L))));

    @Test
    void patchExecutesTheTransitionAndReturnsTheNewVersionAsAnETag() {
        assertThat(mvc.patch().uri(SetTopicSubjectController.PATH, "7")
                .header(HttpHeaders.IF_MATCH, "\"1\"")
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "subject": "sample"
}
"""))
                .hasStatusOk()
                .hasHeader(HttpHeaders.ETAG, "\"1\"");
    }

    @Test
    void aRequestWithNoIfMatchIsRefusedRatherThanAppliedBlind() {
        assertThat(mvc.patch().uri(SetTopicSubjectController.PATH, "7")
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
{
  "subject": "sample"
}
"""))
                .hasStatus(400);
    }

}
