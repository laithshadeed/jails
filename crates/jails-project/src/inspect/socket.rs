//! WebSocket endpoints, which carry no mapping annotation to find.
//!
//! Split from `inspect.rs` by subject rather than by size: that module reads
//! Spring's mapping annotations and a jails `HttpHandler`'s path constant, and
//! this one reads a registration made in Java code at start-up. They answer
//! the same question -- *what does this application serve* -- from sources
//! with nothing in common.

use super::{Route, line_of};

/// Every `registry.addHandler(<handler>, "<path>"[, "<path>"...])` in a
/// `WebSocketConfigurer`.
///
/// `bugs.md` B56: `jails g socket Chat` writes the registration and `jails
/// routes` then reported "No routes found under src/main/java". A route jails
/// emitted and cannot see is worse than a gap -- the reader has no way to tell
/// an unlisted route from an absent one.
///
/// The verb is `WS`: a WebSocket endpoint answers an HTTP GET carrying an
/// `Upgrade` header and then stops being HTTP, so reporting `GET` would put it
/// in a column where a reader would try to curl it.
///
/// Read the way the `HttpHandler` arm reads its path constant: structure off
/// the *blanked* copy, so an `addHandler(` inside the Javadoc example this
/// template carries is not a registration, and the values sliced out of the
/// original, because blanking replaces the quotes too.
pub(super) fn registered_routes(
    source: &str,
    type_name: &str,
    label: &str,
    info: Option<&crate::java::TypeInfo>,
) -> Vec<Route> {
    const CALL: &str = "addHandler(";
    let masked = crate::java::blanked(source);
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = masked[from..].find(CALL) {
        let open = from + rel + CALL.len() - 1;
        let close = crate::java::match_paren(&masked, open);
        if close <= open {
            break;
        }
        from = close;
        // Arguments split on the blanked copy so a comma inside a literal
        // cannot split one, then read out of the original.
        let mut args = Vec::new();
        let (mut start, mut depth) = (open + 1, 0usize);
        for (at, byte) in masked
            .as_bytes()
            .iter()
            .enumerate()
            .take(close)
            .skip(open + 1)
        {
            match byte {
                b'(' | b'<' => depth += 1,
                b')' | b'>' => depth = depth.saturating_sub(1),
                b',' if depth == 0 => {
                    args.push(source[start..at].trim());
                    start = at + 1;
                }
                _ => {}
            }
        }
        args.push(source[start..close].trim());
        let Some((handler, paths)) = args.split_first() else {
            continue;
        };
        let handler = handler.trim_start_matches("this.").trim();
        // `addHandler(handler, ...)` names a field, and the reader wants the
        // class that answers. The constructor jails already parsed is where a
        // `@Configuration`'s collaborators are declared, so the field's type
        // is there; anything it does not name is reported as written rather
        // than resolved, which is this module's rule everywhere else.
        let handler = info
            .and_then(|info| {
                info.constructor_params
                    .iter()
                    .find(|param| param.name == handler)
            })
            .map(|param| param.type_name.clone())
            .unwrap_or_else(|| format!("{type_name}#{handler}"));
        for path in paths {
            let path = path.trim();
            // Only a literal is a path jails can report. A path assembled at
            // run time is exactly what this command's `limitation:` line says
            // it does not evaluate, and inventing one would be worse than the
            // omission.
            if !(path.starts_with('"') && path.ends_with('"') && path.len() >= 2) {
                continue;
            }
            out.push(Route {
                path: path[1..path.len() - 1].to_string(),
                verb: "WS".to_string(),
                handler: handler.clone(),
                source: label.to_string(),
                line: line_of(source, CALL),
            });
        }
    }
    out
}
