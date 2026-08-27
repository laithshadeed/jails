package {{pkg}};

{{target_import}}{{repository_import}}{{imports}}import java.util.Objects;
import org.springframework.stereotype.Component;
{{transaction_import}}
/**
 * The implementation that stores the resource and does nothing else.
 *
 * <p>Named for what it does rather than for its position. `Default` is what
 * you call a class when you have not decided what it is, and it gave the
 * reader no way to tell this apart from {@code Outbox{{name}}UseCase}, which
 * stores the resource <em>and</em> stages its event.
 */
@Component
public class Storing{{name}}UseCase implements {{name}}UseCase {

    private final {{target}}Repository repository;

    public Storing{{name}}UseCase({{target}}Repository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

{{annotation}}    @Override
    public {{target}} execute({{name}}Command command) {
        Objects.requireNonNull(command, "command is required");
{{preamble}}        {{target}} {{var}} = new {{target}}(
{{args}});
        return repository.save({{var}});
    }
}
