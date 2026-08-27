package {{web}};

{{command_import}}{{usecase_import}}{{target_import}}{{scope_import}}{{imports}}import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;
import static org.springframework.test.web.servlet.setup.MockMvcBuilders.standaloneSetup;

{{disabled_import}}import org.junit.jupiter.api.Test;
{{media_type_import}}import org.springframework.test.web.servlet.MockMvc;

/**
 * Written against plain {@code MockMvc} rather than {@code MockMvcTester},
 * because this project's Spring Framework predates the AssertJ entry point.
 */
class {{name}}ControllerTest {

    private final MockMvc mvc = standaloneSetup(new {{name}}Controller(
            command -> {{fake_result}}{{scope_argument}})).build();

{{disabled}}    @Test
    void postExecutesTheUseCase() throws Exception {
        mvc.perform(post({{name}}Controller.PATH)
{{request}})
                .andExpect(status().isCreated());
    }

}
