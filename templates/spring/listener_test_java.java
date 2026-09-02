package {{pkg}};

import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatCode;

/**
 * The listener routes to the port, and says so when nothing is listening.
 *
 * <p>No broker and no Spring context: the listener is an ordinary object with
 * a constructor, which is the point of putting the reaction behind
 * {@link {{name}}Handler}. The companion {@code MessagingIT} proves a record
 * makes the round trip through a real broker; this proves the class in the
 * middle does something with it.
 *
 * <p>The empty case is the one worth keeping. A listener that logs a record
 * and drops it passes every test that only checks delivery, so the assertion
 * is that consuming without a handler is survivable and visible rather than
 * silent.
 */
{{disabled}}class {{name}}ListenerTest {

    @Test
    void everyRegisteredHandlerSeesTheEvent() {
        List<{{name}}Event> first = new ArrayList<>();
        List<{{name}}Event> second = new ArrayList<>();
        {{name}}Listener listener = new {{name}}Listener(List.of(first::add, second::add));
        {{name}}Event event = new {{name}}Event({{event_args}});

        listener.on(event);

        assertThat(first).containsExactly(event);
        assertThat(second).containsExactly(event);
    }

    @Test
    void aRecordWithNoHandlerIsSurvivable() {
        {{name}}Listener listener = new {{name}}Listener(List.of());

        assertThatCode(() -> listener.on(new {{name}}Event({{event_args}})))
                .doesNotThrowAnyException();
    }
}
