//! Persistent input epochs for affected-test selection.

use jails_support::Result;
use jails_support::codec::{DIGEST_BYTES, hex, unhex};
use std::path::Path;

const STATE_FILE: &str = "affected-v2.meta";
const SCHEMA: &str = "jails.affected-v2.meta.v1";

pub(super) fn record(
    project: &Path,
    input: [u8; DIGEST_BYTES],
    graph: [u8; DIGEST_BYTES],
) -> Result<u64> {
    let run = jails_support::apply::ensure_runtime_directory(project)?;
    let path = run.join(STATE_FILE);
    let previous = match std::fs::read_to_string(&path) {
        Ok(text) => Some(State::parse(&text)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "affected epoch state is unreadable ({error})\n       fix: remove `.jails/run/{STATE_FILE}` and retry"
            )
            .into());
        }
    };
    if previous
        .as_ref()
        .is_some_and(|state| state.epoch == u64::MAX && state.input != input)
    {
        return Err("affected epoch is exhausted\n       fix: remove `.jails/run/affected-v2.meta` and restart test watch".into());
    }
    let epoch = previous
        .as_ref()
        .map(|state| {
            if state.input == input {
                state.epoch
            } else {
                state.epoch.saturating_add(1)
            }
        })
        .unwrap_or(1);
    let current = State {
        epoch,
        input,
        graph,
    };
    jails_support::apply::put_runtime_state(project, &path, current.render().as_bytes())?;
    Ok(epoch)
}

#[derive(Debug, Eq, PartialEq)]
struct State {
    epoch: u64,
    input: [u8; DIGEST_BYTES],
    graph: [u8; DIGEST_BYTES],
}

impl State {
    fn render(&self) -> String {
        format!(
            "schema={SCHEMA}\nepoch={}\ninput={}\ngraph={}\n",
            self.epoch,
            hex(&self.input),
            hex(&self.graph)
        )
    }

    fn parse(text: &str) -> Result<Self> {
        let field = |name: &str| -> Result<&str> {
            text.lines()
                .find_map(|line| line.strip_prefix(&format!("{name}=")))
                .ok_or_else(|| {
                    format!(
                        "affected epoch state is missing `{name}`\n       fix: remove `.jails/run/{STATE_FILE}` and retry"
                    )
                    .into()
                })
        };
        if field("schema")? != SCHEMA {
            return Err("affected epoch state uses an unknown schema\n       fix: upgrade jails or remove `.jails/run/affected-v2.meta`".into());
        }
        let epoch = field("epoch")?.parse().map_err(|_| {
                "affected epoch state has an invalid epoch\n       fix: remove `.jails/run/affected-v2.meta` and retry"
            })?;
        if epoch == 0 {
            return Err("affected epoch state has an invalid zero epoch\n       fix: remove `.jails/run/affected-v2.meta` and retry".into());
        }
        Ok(Self {
            epoch,
            input: unhex(field("input")?)?,
            graph: unhex(field("graph")?)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips() {
        let state = State {
            epoch: 7,
            input: [1; DIGEST_BYTES],
            graph: [2; DIGEST_BYTES],
        };
        assert_eq!(State::parse(&state.render()).unwrap(), state);
    }

    #[test]
    fn epoch_changes_only_with_the_input_snapshot() {
        let project = jails_support::scratch::ScratchDir::in_temp("affected-epoch").unwrap();
        assert_eq!(record(project.path(), [1; 32], [2; 32]).unwrap(), 1);
        assert_eq!(record(project.path(), [1; 32], [3; 32]).unwrap(), 1);
        assert_eq!(record(project.path(), [4; 32], [3; 32]).unwrap(), 2);
    }
}
