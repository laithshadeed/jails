//! Reader-facing explanations for values outside the CLI's closed vocabulary.

/// A closed vocabulary jails does not have, and the thing it has instead.
///
/// Only list words with a real answer. A synonym pointing at nothing would be
/// worse than clap's list, which at least tells the reader what does exist.
const INSTEAD: &[(&str, &str)] = &[
    (
        "websocket",
        "there is no `websocket` capability -- `jails g socket <Name>` is the slice: a \
         TextWebSocketHandler,\n       its WebSocketConfigurer registration, a test, and the \
         spring-boot-starter-websocket that makes them run.\n       fix: jails g socket Chat \
         (or, for the dependency alone, jails add dependency \
         org.springframework.boot:spring-boot-starter-websocket)",
    ),
    (
        "socket",
        "there is no `socket` capability -- `jails g socket <Name>` is a generator kind rather \
         than a capability,\n       because it needs a name to write a handler for.\n       fix: \
         jails g socket Chat",
    ),
    (
        "devtools",
        "there is no `devtools` capability -- it is one dependency and no code, so it goes \
         through the verb for that.\n       fix: jails add dependency \
         org.springframework.boot:spring-boot-devtools --scope runtime",
    ),
    (
        "flyway",
        "there is no `flyway` capability -- `db` installs Flyway along with the datasource, the \
         compose service\n       and the test wiring, because a migration tool with no database \
         is not a slice.\n       fix: jails add db",
    ),
    (
        "websockets",
        "there is no `websockets` capability -- `jails g socket <Name>` is the slice.\n       fix: \
         jails g socket Chat",
    ),
];

/// Render clap's parse error, replacing a few known dead ends with a concrete
/// command that expresses what the reader was trying to do.
pub(crate) fn render(error: clap::Error) -> std::process::ExitCode {
    let rendered = error.render().to_string();
    let named = rendered
        .split_once("invalid value '")
        .and_then(|(_, rest)| rest.split_once('\''))
        .map(|(value, _)| value.to_ascii_lowercase());
    if let Some(named) = named
        && let Some((_, instead)) = INSTEAD.iter().find(|(word, _)| *word == named)
    {
        eprintln!("jails: {instead}");
        return std::process::ExitCode::from(2);
    }
    let _ = error.print();
    match error.use_stderr() {
        true => std::process::ExitCode::from(2),
        false => std::process::ExitCode::SUCCESS,
    }
}
