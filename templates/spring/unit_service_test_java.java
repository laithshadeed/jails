package {{pkg}};

import static org.junit.jupiter.api.Assertions.assertNotNull;

import org.junit.jupiter.api.Test;

class {{class}}Test {

    @Test
    void instantiates() {
        assertNotNull(new {{class}}());
    }
}
