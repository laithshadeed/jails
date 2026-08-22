package {{pkg}};

{{domain_import}}{{imports}}
/**
 * What this application returns. Deliberately not {{name}} itself.
 *
 * <p>No validation annotations here: constraints describe what is acceptable
 * as input, and re-stating them on the way out only invites someone to
 * enforce them on data that already exists.
 *
 * <p>{@code from} is a static factory rather than a constructor so the
 * mapping has a name and one place to change.
 */
public record {{name}}Response(
{{components}}) {

    /** @return the response describing {@code {{var}}}. */
    public static {{name}}Response from({{name}} {{var}}) {
        return new {{name}}Response(
{{arguments}});
    }
}
