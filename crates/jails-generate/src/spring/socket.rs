//! `g socket`: the half of a chat `add sse` cannot do.
//!
//! `missing.md` M4: four of the six ported originals are bidirectional chat
//! over Django Channels, and jails had no WebSocket anything -- no kind, no
//! capability, no subcommand. `add sse` covers server-to-client and none of
//! client-to-server, so every one of them was written by hand outside jails.
//!
//! Three things here are decisions rather than boilerplate, and each is the
//! kind that is wrong in a way nothing reports:
//!
//! - **A `WebSocketSession` is not safe for concurrent sends.** Two threads
//!   calling `sendMessage` on one session produce
//!   `IllegalStateException: The remote endpoint was in state
//!   [TEXT_PARTIAL_WRITING]` -- intermittent, load-dependent, and impossible to
//!   reproduce at the desk. Broadcast is exactly that shape, so every session
//!   is wrapped in `ConcurrentWebSocketSessionDecorator`
//!   (`deps/spring-framework/spring-websocket/.../handler/`), which serialises
//!   sends and buffers behind a slow one rather than corrupting the frame.
//! - **A dead session must leave the registry.** `sendMessage` on a closed
//!   session throws `IOException`; a broadcast that lets it propagate stops
//!   before the sessions after it, and one that swallows it keeps the corpse
//!   forever. The generated broadcast removes and closes it.
//! - **The handshake is same-origin by default.** An empty `allowedOrigins`
//!   list installs an `OriginHandshakeInterceptor` that accepts only a
//!   same-origin `Origin` header, so a browser client served from anywhere
//!   else is refused at the handshake with a 403 and no message in the
//!   application log. That default is right; the config says where to change
//!   it rather than changing it, because widening it is a security decision
//!   only the project can make.

use super::*;

/// The starter that brings in `spring-websocket`. Versionless on purpose:
/// under `spring-boot-starter-parent` the parent manages it, and a pinned
/// version here would drift from the Boot line the project is on.
pub(crate) const WEBSOCKET_STARTER: Dependency = Dependency {
    group_id: "org.springframework.boot",
    artifact_id: "spring-boot-starter-websocket",
    version: None,
    scope: None,
    optional: false,
};

pub(crate) fn socket_files(slice: &Slice, name: &str) -> Result<Vec<Artifact>> {
    let root: &Path = slice.project().root();
    let web: &str = &slice.placed(Layer::Web);
    let main = crate::generate::main_dir(root, web);
    let test = crate::generate::test_dir(root, web);
    let path = format!("/ws/{}", crate::sql::snake_case(name).replace('_', "-"));
    Ok(vec![
        Artifact {
            kind: "socket handler",
            path: main.join(format!("{name}SocketHandler.java")),
            contents: crate::template::render(
                crate::template_here!("spring/socket_handler_java.java"),
                &[("pkg", web), ("name", name)],
            ),
        },
        Artifact {
            kind: "socket registration",
            path: main.join(format!("{name}SocketConfig.java")),
            contents: crate::template::render(
                crate::template_here!("spring/socket_config_java.java"),
                &[("pkg", web), ("name", name), ("path", &path)],
            ),
        },
        Artifact {
            kind: "socket handler test",
            path: test.join(format!("{name}SocketHandlerTest.java")),
            contents: crate::template::render(
                crate::template_here!("spring/socket_handler_test_java.java"),
                &[("pkg", web), ("name", name)],
            ),
        },
    ])
}
