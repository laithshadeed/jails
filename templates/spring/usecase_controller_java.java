package {{web}};

{{command_import}}{{usecase_import}}{{scope_import}}import {{validation}}.validation.Valid;
import java.net.URI;
import java.util.Objects;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.PostMapping;
{{binding_import}}import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for one application use case; the operation itself knows nothing about HTTP. */
@RestController
@RequestMapping({{name}}Controller.PATH)
public final class {{name}}Controller {

    public static final String PATH = "{{path}}";
    private static final String RESOURCE_PATH = "{{resource_path}}";

    private final {{name}}UseCase useCase;
{{scope_field}}

    public {{name}}Controller({{name}}UseCase useCase{{scope_constructor}}) {
        this.useCase = Objects.requireNonNull(useCase, "useCase is required");
{{scope_assignment}}
    }

    @PostMapping
    public ResponseEntity<{{target}}Response> execute(
            @Valid @{{binding}} {{name}}Command command{{scope_parameter}}) {
{{scope_checks}}
        var created = useCase.execute(command);
        return ResponseEntity.created(URI.create(RESOURCE_PATH + "/" + created.id()))
                .body({{target}}Response.from(created));
    }
}
