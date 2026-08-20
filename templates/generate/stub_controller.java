package {{pkg}};

import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;

/**
 * Package-private, and so is every handler on it.
 *
 * <p>Spring instantiates and calls this by reflection, so {@code public} buys
 * it nothing -- it only widens the surface other packages can compile
 * against. A controller is an entry point, not module API.
 */
@RestController
class {{name}}Controller {

    @GetMapping("/{{route}}")
    String get() {
        return "{{name}}";
    }
}
