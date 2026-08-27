package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Widget;
import org.junit.jupiter.api.Test;

/**
 * The shipped seed file still binds to {@link Widget}. Nothing else reads
 * {@code db/seeds/widgets.json} until somebody starts under the seed profile, so a
 * renamed component would otherwise surface as a start-up that dies.
 */
class WidgetSeederTest {

    @Test
    void the_shipped_seed_data_binds_to_the_record() {
        assertThat(WidgetSeeder.read()).isNotEmpty();
    }
}
