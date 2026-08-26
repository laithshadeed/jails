package {{service}};

{{target_import}}{{event_import}}{{store_import}}{{instant_import}}import java.util.Objects;
import org.springframework.context.annotation.Primary;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** Creates the resource and stages its event in the same database transaction. */
@Primary
@Component
public class Outbox{{usecase}}UseCase implements {{usecase}}UseCase {

    private final Storing{{usecase}}UseCase delegate;
    private final Jdbc{{usecase}}Outbox outbox;

    public Outbox{{usecase}}UseCase(Storing{{usecase}}UseCase delegate, Jdbc{{usecase}}Outbox outbox) {
        this.delegate = Objects.requireNonNull(delegate, "delegate is required");
        this.outbox = Objects.requireNonNull(outbox, "outbox is required");
    }

    @Override
    @Transactional
    public {{target}} execute({{usecase}}Command command) {
        var result = delegate.execute(command);
        outbox.stage(new {{event}}Event(
{{args}}));
        return result;
    }
}
