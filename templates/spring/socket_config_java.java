package {{pkg}};

import org.springframework.context.annotation.Configuration;
import org.springframework.web.socket.config.annotation.EnableWebSocket;
import org.springframework.web.socket.config.annotation.WebSocketConfigurer;
import org.springframework.web.socket.config.annotation.WebSocketHandlerRegistry;

/**
 * Where {@link {{name}}SocketHandler} answers.
 *
 * <p>No {@code setAllowedOrigins}: an empty list accepts a same-origin
 * {@code Origin} header and nothing else, so a browser client from elsewhere
 * is refused at the handshake with a 403 and no line in the application log.
 * Widening it is a security decision, and this is the file. Without
 * {@code @EnableWebSocket} the registration is inert and the endpoint 404s.
 */
@Configuration
@EnableWebSocket
public class {{name}}SocketConfig implements WebSocketConfigurer {

    private final {{name}}SocketHandler handler;

    public {{name}}SocketConfig({{name}}SocketHandler handler) {
        this.handler = handler;
    }

    @Override
    public void registerWebSocketHandlers(WebSocketHandlerRegistry registry) {
        registry.addHandler(handler, "{{path}}");
    }
}
