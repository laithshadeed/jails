package {{pkg}};

/**
 * What the application does when a {{name}}Event arrives.
 *
 * <p>This is the seam the listener delegates across, and it exists so the
 * reaction is reachable without a broker. A handler is an ordinary bean: a
 * unit test constructs one and calls {@link #handle}, where testing the same
 * logic inside the listener would need a running Kafka.
 *
 * <p>Implement it and annotate the implementation {@code @Component}. Every
 * registered handler is called, in no guaranteed order; a project with none
 * is warned about on the first record rather than dropping it silently.
 *
 * <p>Throwing is meaningful. An exception propagates out of the listener, the
 * offset is not committed, and the record is retried and eventually
 * dead-lettered. Catching everything here acknowledges a record that was
 * never processed, which is data loss that looks like success.
 */
@FunctionalInterface
public interface {{name}}Handler {

    void handle({{name}}Event event);
}
