package {{pkg}};

{{imports}}

/**
 * Package-private, and so is every handler on it.
 *
 * <p>Spring instantiates and calls this by reflection, so {@code public} buys
 * it nothing -- it only widens the surface other packages can compile
 * against. A controller is an entry point, not module API.
 */
@RestController
class {{name}}Controller {

    @{{mapping}}("/{{route}}")
    {{returns}} {{handler}}({{parameters}}) {
        {{body}}
    }
}
