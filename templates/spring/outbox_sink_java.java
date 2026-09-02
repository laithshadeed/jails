package {{pkg}};

/** One independently configurable destination for a staged event. */
public interface {{usecase}}OutboxSink {
    String name();
    void deliver({{event}} event);
}
