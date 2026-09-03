package {{pkg}};

import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;

class {{class}}Test {

    @Test
    @Disabled("todo: state what {{class}} is supposed to do, then assert it")
    void todo() {
        {{class}} {{variable}} = new {{class}}();

        // Replace this with the behaviour {{class}} exists for. Asserting that
        // it is not null would pass while the class is entirely broken.
    }
}
