package {{web}};

{{hub_import}}import org.springframework.http.MediaType;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.servlet.mvc.method.annotation.SseEmitter;

/**
 * The stream endpoint.
 *
 * <p>{@code produces} is not decoration: without {@code text/event-stream} a
 * browser's {@code EventSource} refuses the response, and the failure surfaces
 * in the browser console rather than in any server log.
 */
@RestController
public class {{name}}StreamController {

    private final {{name}}Hub hub;

    public {{name}}StreamController({{name}}Hub hub) {
        this.hub = hub;
    }

    @GetMapping(path = "/{{path}}/{topic}/stream", produces = MediaType.TEXT_EVENT_STREAM_VALUE)
    public SseEmitter stream(@PathVariable String topic) {
        return hub.subscribe(topic);
    }
}
