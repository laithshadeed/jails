package {{pkg}};

{{event_import}}/** One independently configurable destination for a staged event. */
public interface {{usecase}}OutboxSink {
    String name();
    void deliver({{event}}Event event);
}
