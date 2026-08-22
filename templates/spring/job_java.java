package {{pkg}};

import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

/**
 * Scheduled work, with the schedule itself left in configuration.
 *
 * <p>{@code fixedDelayString} reads {@code jobs.{{property}}.delay} rather
 * than hard-coding an interval, so a test can run it every 10ms and production
 * every ten minutes without touching this file.
 *
 * <p>{@code fixedDelay} and not {@code fixedRate}: the delay is measured
 * from the end of the previous run, so a slow run delays the next one instead
 * of queueing another on top of it. Reach for {@code fixedRate} only when
 * you genuinely want overlapping executions.
 *
 * <p>The body catches its own failures. An exception escaping a scheduled
 * method kills the schedule for the rest of the JVM's life, silently -- which
 * is a strange default and the most common way a job stops running without
 * anyone noticing.
 */
@Component
public class {{name}}Job {

    private static final Logger log = LoggerFactory.getLogger({{name}}Job.class);

    @Scheduled(fixedDelayString = "${jobs.{{property}}.delay:PT1M}")
    public void run() {
        try {
            work();
        } catch (RuntimeException failure) {
            // Swallowed deliberately: rethrowing here cancels all future runs.
            log.error("{{name}}Job failed; the schedule continues", failure);
        }
    }

    /** The actual work. Package-private so a test can call it directly. */
    void work() {
        log.info("{{name}}Job ran");
    }
}
