package {{pkg}};

/** Mutable test-data builder for {@link {{name}}}. */
public final class {{class}} {

    // State derived from canonical entity components.
{{declarations}}

    public static {{class}} a{{name}}() {
        return new {{class}}();
    }

    // Fluent overrides derived from canonical entity components.
{{methods}}

    public {{name}} build() {
{{guards}}        return new {{name}}(
{{arguments}});
    }
}
