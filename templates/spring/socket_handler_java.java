package {{pkg}};

import java.io.IOException;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.stereotype.Component;
import org.springframework.web.socket.CloseStatus;
import org.springframework.web.socket.TextMessage;
import org.springframework.web.socket.WebSocketSession;
import org.springframework.web.socket.handler.ConcurrentWebSocketSessionDecorator;
import org.springframework.web.socket.handler.TextWebSocketHandler;

/**
 * The client-to-server half {@code add sse} does not cover.
 *
 * <p>Every session is wrapped: a {@link WebSocketSession} is not safe for
 * concurrent sends, and two threads on one session produce
 * {@code IllegalStateException: … [TEXT_PARTIAL_WRITING]} -- load-dependent,
 * so it never happens at the desk. A session that throws {@link IOException}
 * is evicted, because letting it out stops the broadcast and swallowing it
 * keeps the corpse. {@code jails explain socket} has the rest.
 */
@Component
public class {{name}}SocketHandler extends TextWebSocketHandler {

    private static final Logger log = LoggerFactory.getLogger({{name}}SocketHandler.class);

    /** The decorator's own limits, named so one unread client cannot grow an unbounded buffer. */
    private static final int SEND_TIME_LIMIT_MS = 10_000;

    private static final int BUFFER_SIZE_LIMIT_BYTES = 512 * 1024;

    private final Map<String, WebSocketSession> sessions = new ConcurrentHashMap<>();

    @Override
    public void afterConnectionEstablished(WebSocketSession session) {
        sessions.put(
                session.getId(),
                new ConcurrentWebSocketSessionDecorator(
                        session, SEND_TIME_LIMIT_MS, BUFFER_SIZE_LIMIT_BYTES));
    }

    @Override
    public void afterConnectionClosed(WebSocketSession session, CloseStatus status) {
        sessions.remove(session.getId());
    }

    /** Echoes to everyone; replace the body with what this endpoint is for. */
    @Override
    protected void handleTextMessage(WebSocketSession session, TextMessage message) {
        broadcast(message.getPayload());
    }

    /** Sends to every live session, and forgets the ones that are not. */
    public void broadcast(String payload) {
        TextMessage message = new TextMessage(payload);
        for (WebSocketSession session : sessions.values()) {
            try {
                session.sendMessage(message);
            } catch (IOException | IllegalStateException unreachable) {
                log.debug("dropping {} socket session {}", "{{name}}", session.getId(), unreachable);
                evict(session);
            }
        }
    }

    /** How many sessions this instance is holding. */
    public int connected() {
        return sessions.size();
    }

    private void evict(WebSocketSession session) {
        sessions.remove(session.getId());
        try {
            session.close(CloseStatus.SESSION_NOT_RELIABLE);
        } catch (IOException alreadyGone) {
            log.trace("session {} was already closed", session.getId(), alreadyGone);
        }
    }
}
