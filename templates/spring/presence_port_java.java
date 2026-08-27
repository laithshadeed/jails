package {{app}};

import java.util.List;

/**
 * Who is currently in a scope, as every node sees it.
 *
 * <p>A scope and a member are strings the caller picks -- a room and a user, a
 * document and an editor. This port knows neither.
 */
public interface {{name}}Presence {

    /** Records that this node believes {@code member} is in {@code scope}. */
    void join(String scope, String member);

    /** Refreshes the claim. A member that stops heartbeating leaves on its own. */
    void heartbeat(String scope, String member);

    /** Withdraws this node's claim. Other nodes' claims are untouched. */
    void leave(String scope, String member);

    /** Every member some node has seen inside the window, in a stable order. */
    List<String> present(String scope);
}
