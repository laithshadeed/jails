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

use jails_support::Result;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use crate::compose;
use crate::find_project_root;
use crate::run;

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
        KafkaCommand::Topics => tool(&root, "kafka-topics.sh", &["--list".into()], None, debug),
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
            let spec = crate::process::compose_spec(["exec", "-T", SERVICE])
                .ok_or_else(|| "docker compose is not installed".to_string())?
                .arg(format!("{TOOLS}/kafka-consumer-groups.sh"))
                .args(["--bootstrap-server", BROKER])
                .args(["--describe", "--group", &group])
                .current_dir(&root)
                .output(crate::process::OutputMode::Capture);
            lag(&spec, &group, debug)
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
/// `lag`, which is the one subcommand that reads the broker's answer.
///
/// **A broker exception is a refusal, not a table.** Asking for a group that
/// has never committed is the ordinary first thing a reader does, and
/// `kafka-consumer-groups.sh` answers it with a Java stack trace on stderr
/// and exit 0 -- so jails reported success and printed twenty lines about
/// `GroupIdNotFoundException`. The output is captured rather than inherited
/// so the exception can be recognised; it is a short table, so nothing is
/// lost by printing it after the fact.
fn lag(spec: &crate::process::CommandSpec, group: &str, debug: bool) -> Result<()> {
    let done = crate::process::run(spec, crate::process::Diagnostics::from_flag(debug))?;
    let answer = format!(
        "{}{}",
        done.stdout_string(),
        String::from_utf8_lossy(&done.stderr)
    );

    if answer.contains("GroupIdNotFoundException") {
        return Err(jails_support::Failure::Told(format!(
            "no consumer group `{group}` has committed an offset yet, so it has no lag\n       fix: run the application once with `jails run`, then ask again"
        )));
    }
    if let Some(line) = broker_exception(&answer) {
        return Err(jails_support::Failure::Told(format!(
            "the broker refused the request: {line}\n       fix: check the broker is up with `jails kafka topics`"
        )));
    }
    print!("{}", done.stdout_string());
    eprint!("{}", String::from_utf8_lossy(&done.stderr));
    if done.status.success() {
        Ok(())
    } else {
        Err(jails_support::Failure::Reported)
    }
}

/// The first line of a broker exception, when the answer carries one.
///
/// The tool writes `Error: Executing consumer group command failed due to
/// <fully.qualified.Exception>: <message>` and then a stack trace. One line
/// is the refusal; the trace is about the tool, not the project.
fn broker_exception(answer: &str) -> Option<String> {
    answer
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("Error:") || line.contains("Exception:"))
        .map(str::to_string)
}

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
            // One executor: it prints (when asked) and then runs, delivers
            // stdin, and closes the pipe so the producer sees EOF instead of
            // waiting. `--debug` never decides whether the record is sent.
            let spec = crate::process::compose_spec(["exec", "-T", SERVICE])
                .ok_or_else(|| "docker compose is not installed".to_string())?
                .arg(format!("{TOOLS}/{script}"))
                .args(["--bootstrap-server", BROKER])
                .args(args)
                .current_dir(root)
                .stdin(input.as_bytes().to_vec());
            let done = crate::process::run(&spec, crate::process::Diagnostics::from_flag(debug))?;
            if done.status.success() {
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
    tool(
        root,
        "kafka-console-producer.sh",
        &args,
        Some(&record),
        debug,
    )?;
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
    Ok(declared_topics(root).into_iter().next().ok_or_else(|| {
        "no topic given, and none found in the source -- pass one, or generate a slice \
         with `jails g event <Name>`"
            .to_string()
    })?)
}

/// Topic names this project declares, read from `@KafkaListener` and from the
/// `TOPIC` constants a hand-written consumer converges on.
///
/// Textual, like the rest of jails' Java reading: it answers on a project
/// that does not compile, which is exactly when someone is poking at a topic
/// by hand. The tree it reads is the one answer to "where is the source", so
/// the slice `jails g event` just wrote is in it.
fn declared_topics(root: &Path) -> Vec<String> {
    let roots = crate::inspect::roots::input_roots(root);
    let properties = spring_properties(&roots);
    let mut found = Vec::new();
    for path in crate::inspect::roots::source_files_in(&crate::inspect::roots::source_roots(
        root,
        crate::inspect::roots::SourceSet::Main,
    )) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        found.extend(topics_in(&text, &properties));
    }
    found.sort();
    found.dedup();
    found
}

/// `application.properties` as a key/value map, for resolving the one
/// placeholder shape a `@KafkaListener` topic is spelled with.
fn spring_properties(roots: &[crate::inspect::roots::InputRoot]) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for input in roots
        .iter()
        .filter(|input| input.kind == crate::inspect::roots::RootKind::Resources)
    {
        let Ok(text) = std::fs::read_to_string(input.path.join("application.properties")) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.starts_with('!') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                found
                    .entry(key.trim().to_string())
                    .or_insert_with(|| value.trim().to_string());
            }
        }
    }
    found
}

/// Pull topic names out of one source file.
///
/// Two shapes, both written down in the file being read. A `String TOPIC =
/// "..."` constant is what a hand-written consumer converges on. A
/// `@KafkaListener(topics = "...")` is what `jails g event` emits, and its
/// value is a property placeholder rather than a bare literal, so
/// [`listener_topic`] resolves the one placeholder shape Spring resolves
/// without a profile and skips anything it would have to guess at.
///
/// Deliberately not a parser: `@KafkaListener(topics = TOPIC)` names a
/// constant, not a literal, so following it would mean resolving symbols --
/// and the constant is in the same file anyway.
fn topics_in(source: &str, properties: &BTreeMap<String, String>) -> Vec<String> {
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
    found.extend(listener_topics(source, &blanked, properties));
    found.sort();
    found.dedup();
    found
}

/// The topics every `@KafkaListener(topics = …)` in this file names.
///
/// `blanked` locates the annotation -- a commented-out listener simply is not
/// there -- and the literal is read out of the original at the same offsets.
fn listener_topics(
    source: &str,
    blanked: &str,
    properties: &BTreeMap<String, String>,
) -> Vec<String> {
    const ANNOTATION: &str = "@KafkaListener";
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = blanked[from..].find(ANNOTATION) {
        let at = from + at;
        from = at + ANNOTATION.len();
        let Some(close) = blanked[from..].find(')') else {
            continue;
        };
        let arguments = &source[from..from + close];
        let Some(after_topics) = arguments.find("topics").map(|at| at + "topics".len()) else {
            continue;
        };
        let Some(open) = arguments[after_topics..].find('"') else {
            continue;
        };
        let value_start = after_topics + open + 1;
        let Some(end) = arguments[value_start..].find('"') else {
            continue;
        };
        found.extend(listener_topic(
            &arguments[value_start..value_start + end],
            properties,
        ));
    }
    found
}

/// The topic a `@KafkaListener(topics = "…")` names.
///
/// **Exact or nothing.** A bare literal is the topic. `${key:default}` is the
/// one placeholder shape Spring resolves with no profile set, so it is
/// resolved -- the property when the project states one, the default
/// otherwise. Anything else, a `${key}` with neither, is skipped rather than
/// guessed at: a topic name jails invented is a `kafka consume` that reads an
/// empty topic and reports that nothing arrived.
fn listener_topic(raw: &str, properties: &BTreeMap<String, String>) -> Option<String> {
    let Some(rest) = raw.strip_prefix("${") else {
        return (!raw.is_empty() && !raw.contains('$') && !raw.ends_with(".DLT"))
            .then(|| raw.to_string());
    };
    let placeholder = rest.strip_suffix('}')?;
    let (key, default) = match placeholder.split_once(':') {
        Some((key, default)) => (key, Some(default)),
        None => (placeholder, None),
    };
    properties
        .get(key.trim())
        .cloned()
        .or_else(|| default.map(str::to_string))
        .filter(|topic| !topic.is_empty() && !topic.contains('$') && !topic.ends_with(".DLT"))
}

/// The consumer group: the one given, or `spring.kafka.consumer.group-id`.
fn resolve_group(root: &Path, given: Option<String>) -> Result<String> {
    if let Some(group) = given {
        return Ok(group);
    }
    let properties = root.join("src/main/resources/application.properties");
    let text = std::fs::read_to_string(&properties).unwrap_or_default();
    Ok(text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("spring.kafka.consumer.group-id="))
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
        .ok_or_else(|| {
            "no consumer group given, and spring.kafka.consumer.group-id is not set -- \
             pass --group"
                .to_string()
        })?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one line worth printing out of a stack trace.
    #[test]
    fn a_broker_exception_is_reduced_to_its_first_line() {
        let answer = "\nError: Executing consumer group command failed due to \
                      org.apache.kafka.common.errors.TimeoutException: timed out\n\
                      \tat org.apache.kafka.Admin.describe(Admin.java:1)\n\
                      \tat kafka.admin.ConsumerGroupCommand.main(ConsumerGroupCommand.scala:2)\n";
        assert_eq!(
            super::broker_exception(answer).as_deref(),
            Some(
                "Error: Executing consumer group command failed due to \
                 org.apache.kafka.common.errors.TimeoutException: timed out"
            )
        );
    }

    /// A table is not an exception, however many words it has.
    #[test]
    fn a_group_description_carries_no_exception() {
        let answer = "GROUP TOPIC PARTITION CURRENT-OFFSET LOG-END-OFFSET LAG\n\
                      notes orders 0 12 12 0\n";
        assert_eq!(super::broker_exception(answer), None);
    }

    fn topics(source: &str) -> Vec<String> {
        topics_in(source, &BTreeMap::new())
    }

    #[test]
    fn a_topic_constant_is_read_out_of_the_source() {
        let source = r#"
            class AuditConsumer {
                public static final String TOPIC = "audit.events";
            }
        "#;
        assert_eq!(topics(source), vec!["audit.events"]);
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
        assert_eq!(topics(source), vec!["orders"]);
    }

    /// `blanked()` is what keeps a commented-out constant from being read as
    /// a live one.
    #[test]
    fn a_commented_out_topic_is_not_read() {
        let source = r#"
            // public static final String TOPIC = "old.topic";
            public static final String TOPIC = "new.topic";
        "#;
        assert_eq!(topics(source), vec!["new.topic"]);
    }

    #[test]
    fn a_topic_constant_without_a_literal_is_skipped() {
        let source = "static final String TOPIC = someOtherConstant;";
        assert!(topics(source).is_empty());
    }

    /// The shape `jails g event` writes. Nothing in the slice declares a
    /// `TOPIC` constant any more, so a scan that knows only that shape finds
    /// nothing on the project the command just generated.
    #[test]
    fn a_listener_placeholder_resolves_to_its_default() {
        let source = r#"
            @KafkaListener(topics = "${topics.order-placed:order-placed}")
            public void on(OrderPlacedEvent event) {}
        "#;
        assert_eq!(topics(source), vec!["order-placed"]);
    }

    /// The project stating the property is the project's answer, not the
    /// template's default.
    #[test]
    fn a_property_the_project_sets_wins_over_the_default() {
        let source = r#"@KafkaListener(topics = "${topics.order-placed:order-placed}")"#;
        let properties = BTreeMap::from([("topics.order-placed".to_string(), "orders.v2".into())]);
        assert_eq!(topics_in(source, &properties), vec!["orders.v2"]);
    }

    /// Exact or nothing: a placeholder with no default and no property would
    /// have to be invented, and an invented topic is a `kafka tail` reading an
    /// empty topic and reporting that nothing arrived.
    #[test]
    fn a_placeholder_with_nothing_behind_it_is_not_guessed_at() {
        let source = r#"@KafkaListener(topics = "${topics.order-placed}")"#;
        assert!(topics(source).is_empty());
    }

    #[test]
    fn a_listener_literal_is_the_topic() {
        let source = r#"@KafkaListener(topics = "orders", groupId = "notes")"#;
        assert_eq!(topics(source), vec!["orders"]);
    }

    #[test]
    fn a_commented_out_listener_is_not_read() {
        let source = r#"
            // @KafkaListener(topics = "old.topic")
            @KafkaListener(topics = "new.topic")
        "#;
        assert_eq!(topics(source), vec!["new.topic"]);
    }

    /// `@KafkaListener(topics = TOPIC)` names a constant, and the constant is
    /// in the same file: the other half of the scan reads it.
    #[test]
    fn a_listener_naming_a_constant_is_left_to_the_constant() {
        let source = r#"
            static final String TOPIC = "orders";
            @KafkaListener(topics = TOPIC)
        "#;
        assert_eq!(topics(source), vec!["orders"]);
    }
}
