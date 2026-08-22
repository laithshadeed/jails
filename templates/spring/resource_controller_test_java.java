package {{pkg}};

{{extra}}import java.util.List;
import java.util.Optional;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.test.context.bean.override.mockito.MockitoBean;
import org.springframework.test.web.servlet.assertj.MockMvcTester;
import {{webmvc_test_import}};

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.BDDMockito.given;

@WebMvcTest({{name}}Controller.class)
class {{name}}ControllerTest {

    @Autowired
    private MockMvcTester mvc;

    @MockitoBean
    private {{name}}Service service;

    @Test
    void anEmptyCollectionIsAnEmptyArray() {
        given(service.findAll()).willReturn(List.of());

        assertThat(mvc.get().uri({{name}}Controller.PATH))
                .hasStatusOk()
                .bodyJson()
                .isEqualTo("[]");
    }

    @Test
    void aMissingItemIs404() {
        given(service.findById("nope")).willReturn(Optional.empty());

        assertThat(mvc.get().uri({{name}}Controller.PATH + "/nope")).hasStatus(404);
    }

    @Test
    void aDeleteThatRemovedNothingIs404() {
        given(service.deleteById("nope")).willReturn(false);

        assertThat(mvc.delete().uri({{name}}Controller.PATH + "/nope")).hasStatus(404);
    }
}
