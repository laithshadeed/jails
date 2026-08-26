package {{web}};

{{command_import}}{{usecase_import}}{{scope_import}}{{failure_imports}}import {{validation}}.validation.Valid;
import java.util.Objects;
import org.springframework.web.bind.annotation.PutMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for one optimistic state transition. */
@RestController
@RequestMapping({{name}}Controller.PATH)
public final class {{name}}Controller {

    public static final String PATH = "{{path}}";
    private final {{name}}UseCase useCase;
{{scope_field}}

    public {{name}}Controller({{name}}UseCase useCase{{scope_constructor}}) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");
{{scope_assignment}}
    }

    @PutMapping
    public {{target}}Response execute(
            @Valid @RequestBody {{name}}Command command{{scope_parameter}}) {
{{scope_checks}}
        try {
            return {{target}}Response.from(useCase.execute(command));
{{failure_arms}}        }
    }
}
