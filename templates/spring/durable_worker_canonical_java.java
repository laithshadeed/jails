package {{pkg}};

{{input_import}}{{repository_import}}{{context_import}}import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

/**
 * At-least-once worker: an expired lease is reclaimed after process death.
 *
 * <p><strong>The recovery check is the interesting line.</strong> A process
 * can die after the command's transaction commits and before this queue row is
 * acknowledged, and the lease then hands the same item to the next worker. So
 * the shared id is the proof: if the row is already there, the effect already
 * happened and repeating it would be a second one. Without that check
 * at-least-once delivery becomes at-least-once <em>effect</em>, which for a
 * command that creates something is a duplicate every time a worker restarts.
 *
 * <p>{@code fixedDelay} means two runs never overlap, so a slow item delays
 * the next tick rather than racing it.
 */
@Component
public final class {{name}}Worker {

    private static final Logger log = LoggerFactory.getLogger({{name}}Worker.class);
    private final Jdbc{{name}}Store store;
    private final {{usecase}}Command command;
    private final {{target}}Repository results;

    public {{name}}Worker(
            Jdbc{{name}}Store store, {{usecase}}Command command, {{target}}Repository results) {
        this.store = store;
        this.command = command;
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
        try {
            if (results.findById(claimed.id()).isEmpty()) {
                command.execute({{context}}claimed.work());
            }
            store.succeed(claimed.id());
        } catch (RuntimeException failure) {
            store.fail(claimed.id(), failure);
            log.warn("{{name}} attempt {} failed", claimed.attempt(), failure);
        }
    }
}
