//! `jails kafka` -- send messages to the compose broker and look at what is
//! on it.
//!
//! The counterpart to `jails db`. A database has `psql`, and the reason
//! `jails db` earns its place is that it reads the credentials out of
//! `compose.yaml` so nobody has to. Kafka's equivalent is worse: the tools
//! exist, but every one of them wants `--bootstrap-server`, they live at an
//! absolute path inside the image, and the three commands you actually need
//! when something is wrong (what is on the topic, what is in the dead-letter
//! topic, how far behind is the consumer) are each a different script with a
//! different flag spelling.
//!
//! Everything here runs **inside the broker container**, so there is nothing
//! to install: the Kafka CLI tools ship in the image. That also means these
//! commands work identically on a machine that has never seen a Kafka
//! download.
//!
//! ## Why a `send` at all
//!
//! Because the single most confusing Kafka failure is a message you published
//! by hand that the application will not consume. Spring's JSON deserializer
//! looks for a type header that `kafka-console-producer` does not send, and
//! the symptom is silence. `jails add kafka` sets
//! `spring.json.use.type.headers=false` for exactly this reason, and `jails
//! kafka send` is what makes that setting testable in one line.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::compose;
use crate::generate::find_project_root;
use crate::run;

type Result<T> = std::result::Result<T, String>;

/// The compose service name, and the directory the tools live in inside the
/// official `apache/kafka` image.
const SERVICE: &str = "kafka";
const TOOLS: &str = "/opt/kafka/bin";

/// The broker address *from inside the container*.
///
/// Not `localhost:9092` -- that is the host-side advertised listener. Inside
/// the container the tools reach the broker on the inter-broker listener, and
/// using the host one here works only by accident of the port mapping.
const BROKER: &str = "kafka:19092";

#[derive(clap::Subcommand, Debug, Clone)]
pub enum KafkaCommand {
    /// List the topics on the broker
    Topics,
    /// Partitions, leaders and offsets for a topic
    Describe {
        /// Defaults to the topic `jails g event` declared, when there is one
        topic: Option<String>,
    },
    /// Publish one raw JSON record
    Send {
        /// The JSON body, as a single argument
        json: String,
        /// Record key. Ordering is per partition, and a null key
        /// round-robins, so a key is how related records stay in order.
        #[arg(long)]
        key: Option<String>,
        /// Defaults to the topic `jails g event` declared
        #[arg(long)]
        topic: Option<String>,
    },
    /// Publish a record that cannot be deserialized, to watch it reach the DLT
    Poison {
        #[arg(long)]
        topic: Option<String>,
    },
    /// Print every record on a topic from the beginning, with keys
    Tail {
        topic: Option<String>,
        /// Stop after this many records instead of following
        #[arg(long)]
        max: Option<u32>,
    },
    /// Tail the dead-letter topic, showing why each record failed
    Dlt {
        /// The *source* topic; `.DLT` is appended
        topic: Option<String>,
    },
    /// Consumer group members, offsets and lag
    Lag {
        /// Defaults to spring.kafka.consumer.group-id
        #[arg(long)]
        group: Option<String>,
    },
    /// Rewind a consumer group to the start of the topic
    Reset {
        #[arg(long)]
        group: Option<String>,
        topic: Option<String>,
    },
}

pub fn kafka(command: KafkaCommand, no_start: bool, debug: bool) -> Result<()> {
    let root = find_project_root()?;
    let yaml = compose::read(&root)?;
    if !yaml.contains("kafka:") {
        return Err("no kafka in compose.yaml -- run `jails add kafka` first".into());
    }
    if !no_start {
        compose::up(&root, &[SERVICE], debug);
    }

    match command {
        KafkaCommand::Topics => tool(
            &root,
            "kafka-topics.sh",
            &["--list".into()],
            None,
            debug,
        ),
        KafkaCommand::Describe { topic } => {
            let topic = resolve_topic(&root, topic)?;
            tool(
                &root,
                "kafka-topics.sh",
                &["--describe".into(), "--topic".into(), topic],
                None,
                debug,
            )
        }
        KafkaCommand::Send { json, key, topic } => {
            let topic = resolve_topic(&root, topic)?;
            send(&root, &topic, key.as_deref(), &json, debug)
        }
        KafkaCommand::Poison { topic } => {
            let topic = resolve_topic(&root, topic)?;
            // Deliberately not JSON. This is the record that proves the error
            // handler routes rather than retries. It fails as a
            // `DeserializationException`, which Spring itself classifies as
            // fatal -- so this exercises the inherited half of the policy. The
            // half jails generates, `NonRetryableException`, needs a record
            // that parses and *then* fails: `jails kafka send` with a value the
            // domain has no constant for.
            println!("publishing an unparseable record to {topic}");
            println!("watch where it lands:  jails kafka dlt {topic}");
            send(&root, &topic, Some("poison"), "{ not json", debug)
        }
        KafkaCommand::Tail { topic, max } => {
            let topic = resolve_topic(&root, topic)?;
            let mut args = vec![
                "--topic".to_string(),
                topic,
                "--from-beginning".to_string(),
                "--property".to_string(),
                "print.key=true".to_string(),
                "--property".to_string(),
                "key.separator=\t".to_string(),
            ];
            if let Some(max) = max {
                args.push("--max-messages".into());
                args.push(max.to_string());
            }
            tool(&root, "kafka-console-consumer.sh", &args, None, debug)
        }
        KafkaCommand::Dlt { topic } => {
            let topic = format!("{}.DLT", resolve_topic(&root, topic)?);
            println!("tailing {topic} (ctrl-c to stop)");
            tool(
                &root,
                "kafka-console-consumer.sh",
                &[
                    "--topic".into(),
                    topic,
                    "--from-beginning".into(),
                    "--property".into(),
                    "print.headers=true".into(),
                    "--property".into(),
                    "print.key=true".into(),
                    "--property".into(),
                    "key.separator=\t".into(),
                ],
                None,
                debug,
            )
        }
        KafkaCommand::Lag { group } => {
            let group = resolve_group(&root, group)?;
            tool(
                &root,
                "kafka-consumer-groups.sh",
                &["--describe".into(), "--group".into(), group],
                None,
                debug,
            )
        }
        KafkaCommand::Reset { group, topic } => {
            let group = resolve_group(&root, group)?;
            let topic = resolve_topic(&root, topic)?;
            // `--execute` rather than `--dry-run`: the command exists to do
            // the thing, and it refuses anyway while the group has a live
            // member, which is the guard that matters.
            tool(
                &root,
                "kafka-consumer-groups.sh",
                &[
                    "--group".into(),
                    group,
                    "--topic".into(),
                    topic,
                    "--reset-offsets".into(),
                    "--to-earliest".into(),
                    "--execute".into(),
                ],
                None,
                debug,
            )
        }
    }
}

/// Run one of the broker's own CLI tools inside the container.
fn tool(
    root: &Path,
    script: &str,
    args: &[String],
    stdin: Option<&str>,
    debug: bool,
) -> Result<()> {
    let mut cmd = Command::new("docker");
    cmd.args(["compose", "exec"]);
    // No TTY: these are piped into a terminal jails does not own, and
    // `-T` is what keeps the output usable when stdout is redirected.
    cmd.arg("-T");
    cmd.arg(SERVICE);
    cmd.arg(format!("{TOOLS}/{script}"));
    cmd.args(["--bootstrap-server", BROKER]);
    cmd.args(args);
    cmd.current_dir(root);

    match stdin {
        None => run::run_inherited(cmd, debug),
        Some(input) => {
            if debug {
                crate::debug_cmd(&cmd);
                return Ok(());
            }
            cmd.stdin(Stdio::piped());
            let mut child = cmd
                .spawn()
                .map_err(|e| format!("failed to run docker compose exec: {e}"))?;
            {
                use std::io::Write;
                let pipe = child
                    .stdin
                    .as_mut()
                    .ok_or_else(|| "failed to open stdin to the producer".to_string())?;
                pipe.write_all(input.as_bytes())
                    .map_err(|e| format!("failed to write the record: {e}"))?;
            }
            let status = child
                .wait()
                .map_err(|e| format!("failed to wait for the producer: {e}"))?;
            if status.success() {
                Ok(())
            } else {
                Err("the producer exited non-zero".into())
            }
        }
    }
}

fn send(root: &Path, topic: &str, key: Option<&str>, json: &str, debug: bool) -> Result<()> {
    // `parse.key` plus a tab separator, so a key can be given at all. Without
    // it the console producer sends a null key and every record
    // round-robins across partitions -- which silently breaks any ordering
    // the application depends on.
    let record = match key {
        Some(key) => format!("{key}\t{json}\n"),
        None => format!("{json}\n"),
    };
    let mut args = vec!["--topic".to_string(), topic.to_string()];
    if key.is_some() {
        args.extend([
            "--property".to_string(),
            "parse.key=true".to_string(),
            "--property".to_string(),
            "key.separator=\t".to_string(),
        ]);
    }
    tool(root, "kafka-console-producer.sh", &args, Some(&record), debug)?;
    println!("sent to {topic}");
    Ok(())
}

/// The topic to act on: the one given, or the one this project declares.
///
/// Reading it out of the source is the whole point -- the alternative is
/// remembering a topic name that is already written down in a `TOPIC`
/// constant three directories away.
fn resolve_topic(root: &Path, given: Option<String>) -> Result<String> {
    if let Some(topic) = given {
        return Ok(topic);
    }
    declared_topics(root).into_iter().next().ok_or_else(|| {
        "no topic given, and none found in the source -- pass one, or generate a slice \
         with `jails g event <Name>`"
            .to_string()
    })
}

/// Topic names this project declares, read from `@KafkaListener` and from the
/// `TOPIC` constants `jails g event` writes.
///
/// Textual, like the rest of jails' Java reading: it answers on a project
/// that does not compile, which is exactly when someone is poking at a topic
/// by hand.
fn declared_topics(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![root.join("src/main/java")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path.extension().is_some_and(|e| e == "java") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            found.extend(topics_in(&text));
        }
    }
    found.sort();
    found.dedup();
    found
}

/// Pull topic names out of one source file.
///
/// A `String TOPIC = "..."` constant, which is the shape `jails g event`
/// emits and the shape a hand-written consumer converges on. Deliberately
/// not a parser: a `@KafkaListener(topics = TOPIC)` names a constant, not a
/// literal, so following it would mean resolving symbols -- and the constant
/// is in the same file anyway.
fn topics_in(source: &str) -> Vec<String> {
    // `blanked` replaces comments *and literal contents* with spaces of the
    // same length. So it is the right thing to search for the declaration
    // (a commented-out constant simply is not there) and the wrong thing to
    // read the value out of -- the quotes are blanked too. Byte offsets line
    // up between the two, which is what makes the split safe.
    let blanked = crate::java::blanked(source);
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = blanked[from..].find("TOPIC") {
        let at = from + at;
        from = at + "TOPIC".len();

        // The `=` of this declaration, on this line.
        let Some(eq) = blanked[from..].find('=') else {
            continue;
        };
        let eq = from + eq;
        if blanked[from..eq].contains(';') || blanked[from..eq].contains('\n') {
            continue;
        }

        // The literal itself, read from the original.
        let Some(open) = source[eq..].find('"') else {
            continue;
        };
        let open = eq + open;
        if source[eq..open].contains(';') || source[eq..open].contains('\n') {
            continue;
        }
        let value_start = open + 1;
        let Some(close) = source[value_start..].find('"') else {
            continue;
        };
        let value = &source[value_start..value_start + close];
        // A dead-letter constant is derived from its source topic and is not
        // itself the topic anyone means when they say "the topic".
        if !value.is_empty() && !value.ends_with(".DLT") {
            found.push(value.to_string());
        }
    }
    found.sort();
    found.dedup();
    found
}

/// The consumer group: the one given, or `spring.kafka.consumer.group-id`.
fn resolve_group(root: &Path, given: Option<String>) -> Result<String> {
    if let Some(group) = given {
        return Ok(group);
    }
    let properties = root.join("src/main/resources/application.properties");
    let text = std::fs::read_to_string(&properties).unwrap_or_default();
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("spring.kafka.consumer.group-id="))
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
        .ok_or_else(|| {
            "no consumer group given, and spring.kafka.consumer.group-id is not set -- \
             pass --group"
                .to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_topic_constant_is_read_out_of_the_source() {
        let source = r#"
            class LedgerConsumer {
                public static final String TOPIC = "ledger.transactions";
            }
        "#;
        assert_eq!(topics_in(source), vec!["ledger.transactions"]);
    }

    /// The DLT is derived from the source topic, so `jails kafka tail` with no
    /// argument must not pick it -- that would tail the dead letters and
    /// report an empty topic as if it were the real one.
    #[test]
    fn a_dead_letter_constant_is_not_offered_as_the_topic() {
        let source = r#"
            public static final String TOPIC = "orders";
            public static final String DEAD_LETTER_TOPIC = "orders.DLT";
        "#;
        assert_eq!(topics_in(source), vec!["orders"]);
    }

    /// `blanked()` is what keeps a commented-out constant from being read as
    /// a live one.
    #[test]
    fn a_commented_out_topic_is_not_read() {
        let source = r#"
            // public static final String TOPIC = "old.topic";
            public static final String TOPIC = "new.topic";
        "#;
        assert_eq!(topics_in(source), vec!["new.topic"]);
    }

    #[test]
    fn a_topic_constant_without_a_literal_is_skipped() {
        let source = "static final String TOPIC = someOtherConstant;";
        assert!(topics_in(source).is_empty());
    }
}
