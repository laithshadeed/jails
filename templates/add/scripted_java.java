package {{pkg}};

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * A collaborator that replays a fixed script and records how it was called.
 *
 * <p>Attach it to any interface with a lambda -- which is why this is one class
 * rather than one fake per interface, and why it needs no mocking framework:
 *
 * {@snippet :
 * var model = Fake.of(Fake.value("ok"), Fake.failure(new IllegalStateException("timeout")));
 * ModelProvider provider = prompt -> model.next(prompt);
 *
 * assertThat(provider.generate("hello")).isEqualTo("ok");
 * assertThat(model.calls()).containsExactly(List.of("hello"));
 * }
 *
 * <p>Once the script runs out the last step repeats, so a test that only cares
 * about the first response does not have to pad the script to match.
 */
public final class Fake<T> {

    /** One scripted turn. Sealed, so a switch over it is checked for exhaustiveness. */
    public sealed interface Step<T> {}

    public record Value<T>(T value) implements Step<T> {}

    public record Failure<T>(RuntimeException error) implements Step<T> {}

    private final List<Step<T>> script;
    private final List<List<Object>> calls = new ArrayList<>();
    private int index = 0;

    private Fake(List<Step<T>> script) {
        if (script.isEmpty()) {
            throw new IllegalArgumentException("a fake needs at least one step");
        }
        this.script = List.copyOf(script);
    }

    @SafeVarargs
    public static <T> Fake<T> of(Step<T>... steps) {
        return new Fake<>(List.of(steps));
    }

    public static <T> Step<T> value(T value) {
        return new Value<>(value);
    }

    public static <T> Step<T> failure(RuntimeException error) {
        return new Failure<>(error);
    }

    /**
     * Records the arguments it was called with, then plays the next step.
     *
     * <p>{@code Stream.toList()} rather than {@code List.of}: a null argument
     * is a perfectly ordinary thing to want to assert a collaborator was
     * called with, and {@code List.of} rejects it.
     */
    public T next(Object... arguments) {
        calls.add(Arrays.stream(arguments).toList());
        var step = script.get(Math.min(index++, script.size() - 1));
        return switch (step) {
            case Value<T>(var value) -> value;
            case Failure<T>(var error) -> throw error;
        };
    }

    /** Every call so far, in order, each as its argument list. */
    public List<List<Object>> calls() {
        return List.copyOf(calls);
    }

    public int callCount() {
        return calls.size();
    }
}
