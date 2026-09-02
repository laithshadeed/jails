package {{pkg}};

import static org.assertj.core.api.Assertions.assertThat;

import org.junit.jupiter.api.Test;

/**
 * The shipped seed file still binds to {@link {{name}}}. Nothing else reads
 * {@code {{resource}}} until somebody starts under the seed profile, so a
 * renamed component would otherwise surface as a start-up that dies.
 */
class {{name}}SeederTest {

{{disabled}}    @Test
    void the_shipped_seed_data_binds_to_the_record() {
        assertThat({{name}}Seeder.read()).isNotEmpty();
    }
}
