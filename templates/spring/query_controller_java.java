package {{web}};

{{query_import}}{{port_import}}{{scope_import}}import {{validation}}.validation.Valid;
import java.util.List;
import java.util.Objects;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

/** HTTP adapter for a typed read-side port. */
@RestController
@RequestMapping({{name}}QueryController.PATH)
public final class {{name}}QueryController {

    public static final String PATH = "{{path}}";

    private final {{name}}QueryPort queryPort;
{{scope_field}}

    public {{name}}QueryController({{name}}QueryPort queryPort{{scope_constructor}}) {
        this.queryPort = Objects.requireNonNull(queryPort, "queryPort is required");
{{scope_assignment}}
    }

    @PostMapping
    public List<{{target}}Response> execute(
            @Valid @RequestBody {{name}}Query query{{scope_parameter}}) {
{{scope_checks}}
        return queryPort.execute(query).stream().map({{target}}Response::from).toList();
    }
}
