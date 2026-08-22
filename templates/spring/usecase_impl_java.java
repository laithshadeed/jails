package {{pkg}};

{{target_import}}{{repository_import}}{{imports}}import java.util.Objects;
import org.springframework.stereotype.Component;
{{transaction_import}}
/** The conventional implementation generated from the target record's field model. */
@Component
public class Default{{name}}UseCase implements {{name}}UseCase {

    private final {{target}}Repository repository;

    public Default{{name}}UseCase({{target}}Repository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

{{annotation}}    @Override
    public {{target}} execute({{name}}Command command) {
        Objects.requireNonNull(command, "command is required");
        {{target}} {{var}} = new {{target}}(
{{args}});
        repository.save({{var}});
        return {{var}};
    }
}
