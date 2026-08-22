package {{pkg}};

{{domain_import}}{{sample_imports}}{{disabled_import}}import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * The round trip is the property worth pinning: whatever a request describes
 * must survive being turned into the domain type and back into a response.
 *
 * <p>Two records that drift apart still compile -- a component added to one
 * and not the other is silently dropped on the wire -- so this is the test
 * that notices.
 */
{{disabled}}class {{name}}DtoTest {

    @Test
    void aRequestSurvivesTheRoundTripToAResponse() {
        {{name}}Request request = sample();
        {{name}} {{var}} = request.toDomain();
        {{name}}Response response = {{name}}Response.from({{var}});

        assertThat(response).isNotNull();
        // Every component exists on both records -- the compiler has already
        // checked that much. What to assert here is which ones matter.
    }

    private static {{name}}Request sample() {
        return new {{name}}Request(
{{arguments}});
    }
}
