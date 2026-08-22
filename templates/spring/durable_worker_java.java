package {{pkg}};

{{command_import}}{{usecase_import}}{{repo_import}}import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

/** At-least-once worker; an expired lease is reclaimed after process death. */
@Component
public final class {{name}}Worker {

    private static final Logger log = LoggerFactory.getLogger({{name}}Worker.class);
    private final Jdbc{{name}}Store store;
    private final {{usecase}}UseCase useCase;
    private final {{target}}Repository results;

    public {{name}}Worker(Jdbc{{name}}Store store, {{usecase}}UseCase useCase,
                       {{target}}Repository results) {
        this.store = store;
        this.useCase = useCase;
        this.results = results;
    }

    @Scheduled(
            fixedDelayString = "${jobs.{{property}}.delay:PT1S}",
            initialDelayString = "${jobs.{{property}}.initial-delay:PT1S}")
    public void run() {
        try {
            runOnce();
        } catch (RuntimeException infrastructureFailure) {
            log.error("{{name}} could not claim durable work; the schedule continues", infrastructureFailure);
        }
    }

    public void runOnce() {
        store.claim().ifPresent(this::execute);
    }

    private void execute(Jdbc{{name}}Store.Claimed claimed) {
        var work = claimed.work();
        try {
            // A process can die after the use-case transaction commits and
            // before this queue row is acknowledged. The stable shared id is
            // the recovery proof: do not repeat an already-visible effect.
            if (results.findById(String.valueOf(work.id())).isEmpty()) {
                useCase.execute(new {{usecase}}Command(
{{args}}));
            }
            store.succeed(work.id());
        } catch (RuntimeException failure) {
            store.fail(work.id(), failure);
            log.warn("{{name}} attempt {} failed", claimed.attempt(), failure);
        }
    }
}
