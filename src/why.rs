//! `jails why` -- read a failure and say what it actually means.
//!
//! Java's startup failures are verbose and indirect: a hundred lines of
//! Spring internals whose real cause is one missing annotation, one unset
//! environment variable, or one container that is not running. The set of
//! causes is small and enumerable, so this file enumerates it. Each rule is
//! a signature to look for, a plain-language explanation, and the command
//! that fixes it.
//!
//! Every rule here was written against a failure that actually happened
//! while building a project with jails, not against the documentation --
//! which is why, for instance, the Testcontainers rule talks about podman
//! sockets rather than about Docker Desktop.
//!
//! The rules are matched, never inferred: an unrecognised failure is
//! reported as unrecognised. Guessing at a cause is worse than silence,
//! because a wrong explanation costs more time than no explanation.

use std::io::{IsTerminal, Read};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::Result;
use crate::generate::find_project_root;
use crate::pom;
use crate::run;

/// One recognised failure.
struct Diagnosis {
    /// The failure in one line, in the reader's terms rather than Spring's.
    headline: String,
    /// Why it happens. The part a stack trace never says.
    because: String,
    /// What to do about it, most likely first.
    fixes: Vec<String>,
}

/// A rule: a signature in the log, and how to explain a match.
struct Rule {
    /// Every fragment must appear somewhere in the log for the rule to fire.
    /// Multiple fragments are how a specific rule outranks a generic one.
    signatures: &'static [&'static str],
    /// Rules sharing a group describe one underlying failure through
    /// different messages -- Spring reports a missing bean as both
    /// "required a bean of type" and "No qualifying bean of type", and
    /// printing both explanations says the same thing twice. Only the
    /// most specific match in a group is reported.
    group: &'static str,
    explain: fn(&str) -> Diagnosis,
}

const RULES: &[Rule] = &[
    Rule {
        // Testcontainers caches a failed environment probe for the life of
        // the JVM, so this is what every *subsequent* test in the same run
        // reports. Observed 7 times against 4 of the original message, which
        // means the retry text is the one more often pasted into a search.
        signatures: &["Previous attempts to find a Docker environment failed"],
        group: "docker-env",
        explain: |_| {
            Diagnosis {
            headline: "Testcontainers gave up looking for a container engine".into(),
            because: "This is the *second* failure, not the first: Testcontainers probes for an \
                      engine once per JVM and caches the answer, so every test after the first \
                      reports this instead of the real message. The original is earlier in the \
                      log -- search up for \"Could not find a valid Docker environment\". The \
                      cause is almost always that Testcontainers reads DOCKER_HOST or \
                      /var/run/docker.sock and finds neither, while the `docker` CLI works fine \
                      because it is podman's shim talking to a different socket."
                .into(),
            fixes: vec![
                "export DOCKER_HOST=unix://$XDG_RUNTIME_DIR/podman/podman.sock".into(),
                "export TESTCONTAINERS_RYUK_DISABLED=true   # rootless podman cannot run Ryuk".into(),
                "jails doctor                                # its testcontainers check reports this".into(),
            ],
        }
        },
    },
    Rule {
        // `jails console` shells out to jshell, whose default execution
        // engine forks a JVM and talks to it over a loopback socket.
        signatures: &["FailOverExecutionControlProvider"],
        group: "jshell",
        explain: |_| Diagnosis {
            headline: "jshell could not start its execution engine".into(),
            because: "By default jshell runs your snippets in a *second* JVM and talks to it over \
                      a loopback socket (the `jdi` execution provider). Where opening that socket \
                      is not permitted -- a sandbox, a locked-down container, some corporate \
                      endpoint agents -- the launch fails with this provider-chain message, which \
                      says nothing about sockets. The `local` provider runs snippets in the same \
                      JVM and needs no socket at all; the trade is isolation, so a snippet that \
                      calls System.exit takes the shell with it."
                .into(),
            fixes: vec!["jails console -- --execution local".into()],
        },
    },
    Rule {
        signatures: &["mvnd/registry", "Read-only file system"],
        group: "mvnd-registry",
        explain: |_| {
            Diagnosis {
            headline: "The Maven daemon cannot write its registry".into(),
            because: "mvnd keeps a registry of running daemons under ~/.m2/mvnd, and cannot start \
                      when that path is read-only -- which is what a sandbox with a read-only home \
                      looks like from inside. This is the daemon failing to launch, not your build \
                      failing."
                .into(),
            fixes: vec![
                "jails mvn -- <goal>   # jails prefers the project's ./mvnw, which is not mvnd".into(),
                "rm -rf ~/.m2/mvnd/registry   # if the registry is merely stale rather than read-only".into(),
            ],
        }
        },
    },
    Rule {
        signatures: &["Could not find a valid Docker environment"],
        group: "docker-env",
        explain: |_| Diagnosis {
            headline: "Testcontainers cannot reach a container engine".into(),
            because: "Testcontainers looks for DOCKER_HOST or /var/run/docker.sock. It does not \
                      read podman's rootless socket, so on a podman machine the `docker` CLI \
                      works (it is a shim) while every @SpringBootTest that starts a container \
                      fails. `jails start` succeeding proves nothing here -- the CLI and \
                      Testcontainers look at two different sockets."
                .into(),
            fixes: vec![
                "export DOCKER_HOST=unix://$XDG_RUNTIME_DIR/podman/podman.sock".into(),
                "export TESTCONTAINERS_RYUK_DISABLED=true   # rootless podman cannot run Ryuk"
                    .into(),
                "systemctl --user start podman.socket        # if the socket does not exist yet"
                    .into(),
                "jails doctor                                 # confirms which of these is missing"
                    .into(),
            ],
        },
    },
    Rule {
        // Two signatures, so this outranks any generic startup failure it
        // cascades into. It fires before the datasource rule on purpose:
        // when compose never starts, "no suitable driver class" is the
        // symptom and this is the cause.
        signatures: &["podman-compose", "spring-boot-docker-compose"],
        group: "compose-provider",
        explain: |_| {
            Diagnosis {
            headline: "Spring Boot's Docker Compose module cannot drive podman-compose".into(),
            because: "`spring-boot-docker-compose` shells out to the compose provider with \
                      Docker Compose v2 syntax -- `--ansi never` and `config --format=json`. \
                      podman-compose accepts neither (it spells the first `--no-ansi` and has no \
                      `--format` at all), so the call exits 2 and the application dies during \
                      startup, before any of your code runs. Nothing is wrong with the compose \
                      file or the containers. jails already starts compose services itself in \
                      `jails run` and `jails start`, so Spring's own integration is redundant \
                      here -- turning it off loses nothing."
                .into(),
            fixes: vec![
                "echo 'spring.docker.compose.enabled=false' >> src/main/resources/application.properties".into(),
                "jails start db   # jails starts the services instead, which it already did".into(),
            ],
        }
        },
    },
    Rule {
        signatures: &["Failed to determine a suitable driver class"],
        group: "datasource",
        explain: |_| {
            Diagnosis {
            headline: "Spring has no database URL, so it cannot pick a JDBC driver".into(),
            because: "JDBC auto-configuration is active (the starter is on the classpath) but \
                      nothing supplied a datasource URL. In tests this is the usual case: Spring \
                      Boot skips Docker Compose during tests by default \
                      (spring.docker.compose.skip.in-tests=true), so the compose database is not \
                      started and no URL is contributed. `jails add db` writes a \
                      TestcontainersConfig holding an @ServiceConnection container bean, and \
                      splices @Import(TestcontainersConfig.class) onto the @SpringBootTest \
                      classes for exactly this; a test class missing that @Import fails here."
                .into(),
            fixes: vec![
                "jails doctor        # the 'test datasource' check reports whether it is registered".into(),
                "jails add db        # idempotent: re-writes only what is missing".into(),
                "For a main-app (not test) failure: jails start db, then check compose.yaml".into(),
            ],
        }
        },
    },
    Rule {
        signatures: &["required a bean of type"],
        group: "missing-bean",
        explain: |log| {
            let missing = capture_between(log, "required a bean of type '", "'")
                .unwrap_or_else(|| "the type named above".into());
            let short = missing.rsplit('.').next().unwrap_or(&missing).to_string();
            Diagnosis {
                headline: format!("Nothing registers a bean of type {short}"),
                because: format!(
                    "A constructor asks for {short}, but no class in the context is registered \
                     as one. Two ways that happens: the implementation exists but has no \
                     stereotype annotation (@Component), or it has one but \
                     sits outside the @SpringBootApplication class's package tree, which is what \
                     Spring actually scans."
                ),
                fixes: vec![
                    "jails beans      # lists every registered bean and flags unresolvable dependencies".into(),
                    format!("Annotate the implementation of {short} with @Component"),
                    "Check the implementation lives under the application class's package".into(),
                ],
            }
        },
    },
    Rule {
        // The other half of the injection failure. The zero case and the
        // many case read almost identically in a stack trace and have
        // opposite fixes, so they are separate rules -- and this one has to
        // outrank the zero-case rule, whose message it partly shares.
        signatures: &["required a single bean", "were found"],
        group: "ambiguous-bean",
        explain: |log| {
            let candidates: Vec<String> = log
                .lines()
                .filter_map(|line| {
                    let line = line.trim();
                    let name = line.strip_prefix("- ")?.split(':').next()?;
                    (!name.is_empty() && name.len() < 80).then(|| name.to_string())
                })
                .collect();
            let named = if candidates.is_empty() {
                String::new()
            } else {
                format!(" The candidates are: {}.", candidates.join(", "))
            };
            Diagnosis {
                headline: "Two or more beans qualify for one injection point".into(),
                because: format!(
                    "Spring found several candidates and will not choose for you.{named} This is \
                     the usual shape of \"I added a real adapter and kept the in-memory one\": \
                     both carry a stereotype annotation, both implement the same port, and \
                     nothing says which one the application should use."
                ),
                fixes: vec![
                    "jails beans     # shows every candidate and what each one provides".into(),
                    "Mark the one you want with @Primary".into(),
                    "Or drop the stereotype from the other -- an in-memory fake usually wants to \
                     be constructed by tests, not registered in the application context"
                        .into(),
                ],
            }
        },
    },
    Rule {
        // The generic wrapper. Deliberately one signature, so the specific
        // rules above (which name the type) outrank it whenever the fuller
        // message is present -- this one is for a log where only the
        // exception line survived.
        signatures: &["UnsatisfiedDependencyException"],
        group: "missing-bean",
        explain: |_| Diagnosis {
            headline:
                "A bean could not be constructed because one of its dependencies could not be"
                    .into(),
            because:
                "Spring reports the outermost bean, but the cause is further down: read to the \
                      last `Caused by:` and it will name a type. That type either has no \
                      implementation registered, or has more than one and Spring will not choose. \
                      Both are visible without starting the application."
                    .into(),
            fixes: vec![
                "jails beans     # every registered bean, and which dependencies do not resolve"
                    .into(),
                "jails doctor    # the same check, as a pass/fail line".into(),
            ],
        },
    },
    Rule {
        signatures: &["No qualifying bean of type"],
        group: "missing-bean",
        explain: |log| {
            let missing = capture_between(log, "No qualifying bean of type '", "'")
                .unwrap_or_else(|| "the type named above".into());
            let short = missing.rsplit('.').next().unwrap_or(&missing).to_string();
            Diagnosis {
                headline: format!("No bean qualifies as {short}"),
                because: format!(
                    "Spring found zero candidates for {short}. If {short} is an interface, its \
                     implementation is what needs the annotation -- annotating the interface does \
                     nothing. If there are several candidates the message would instead complain \
                     about ambiguity, so this is the zero case."
                ),
                fixes: vec![
                    "jails beans      # shows which types are registered and what they provide"
                        .into(),
                    format!("Annotate the class that implements {short}, not {short} itself"),
                ],
            }
        },
    },
    Rule {
        signatures: &["Port", "was already in use"],
        group: "port",
        explain: |log| {
            let port = capture_between(log, "Port ", " was already in use")
                .unwrap_or_else(|| "8080".into());
            Diagnosis {
                headline: format!("Port {port} is held by another process"),
                because: "The embedded web server could not bind. Almost always a previous run of \
                          this same app that did not exit -- `jails run --watch` and a detached \
                          `spring-boot:run` are the usual culprits."
                    .into(),
                fixes: vec![
                    format!("lsof -i :{port}     # find it"),
                    format!("kill $(lsof -t -i :{port})"),
                    format!("Or set server.port to something free in application.properties"),
                ],
            }
        },
    },
    Rule {
        signatures: &["Connection to", "refused"],
        group: "db-unreachable",
        explain: |_| {
            Diagnosis {
            headline: "Nothing is listening where the database should be".into(),
            because: "The JDBC URL points at a host and port with no server on it. With a \
                      jails-managed compose database this means the container is not running, or \
                      is running but still starting up -- postgres accepts TCP only after it has \
                      finished recovery, so a connection attempt immediately after `up` can lose \
                      the race."
                .into(),
            fixes: vec![
                "jails start db".into(),
                "jails doctor      # its postgres check makes a real connection, not just a container check".into(),
            ],
        }
        },
    },
    Rule {
        signatures: &["does not exist", "relation"],
        group: "missing-table",
        explain: |log| {
            let table = capture_between(log, "relation \"", "\"").unwrap_or_default();
            let named = if table.is_empty() {
                "A table the query names".to_string()
            } else {
                format!("Table \"{table}\"")
            };
            Diagnosis {
                headline: format!("{named} is not in the database"),
                because: "The schema is behind the code. Either no migration creates it, or a \
                          migration does but Flyway has not run -- Flyway migrates on application \
                          startup, so a query run against a database that was created before the \
                          migration was written sees the old schema."
                    .into(),
                fixes: vec![
                    "jails g migration create_<table>   # write it".into(),
                    "jails doctor                        # its migrations check counts the .sql files".into(),
                    "jails db -- -c '\\dt'                # see what the database actually has".into(),
                ],
            }
        },
    },
    Rule {
        signatures: &["Validate failed", "checksum"],
        group: "flyway-checksum",
        explain: |_| {
            Diagnosis {
            headline: "A migration file changed after it was applied".into(),
            because: "Flyway records a checksum for every migration it runs. Editing an already-\
                      applied file breaks the record, on purpose: the database in front of you no \
                      longer matches the file that supposedly produced it. Migrations are \
                      forward-only, which is why `jails destroy` refuses to delete one."
                .into(),
            fixes: vec![
                "Revert the edit and write a new migration for the change instead".into(),
                "For a throwaway local database: jails stop db && docker volume rm <project>_postgres-data".into(),
            ],
        }
        },
    },
    Rule {
        signatures: &["release version", "not supported"],
        group: "jdk",
        explain: |_| Diagnosis {
            headline: "The JDK on PATH is older than the release the pom targets".into(),
            because: "javac refuses --release for a version it does not implement. It is not \
                      enough for a JDK to be installed; it has to be at least as new as \
                      <maven.compiler.release> (or <java.version>) in pom.xml."
                .into(),
            fixes: vec![
                "jails doctor            # its jdk check prints both numbers side by side".into(),
                "mise exec java@<n> -- jails build".into(),
                "Or lower the release level in pom.xml".into(),
            ],
        },
    },
    Rule {
        signatures: &["NoSuchMethodError"],
        group: "version-skew",
        explain: |log| {
            let method = capture_between(log, "NoSuchMethodError: ", "\n").unwrap_or_default();
            Diagnosis {
                headline: "A library version on the classpath is not the one the code was compiled against"
                    .into(),
                because: format!(
                    "The method exists at compile time and not at runtime, which means two \
                     versions of the same library are in play. {}A known instance in jails-\
                     generated code: Commons CSV renamed Builder.build() to Builder.get() in \
                     1.13, so the pinned version and the generated call have to move together.",
                    if method.is_empty() {
                        String::new()
                    } else {
                        format!("Missing: {}. ", method.trim())
                    }
                ),
                fixes: vec![
                    "jails mvn -- dependency:tree -Dverbose   # find the duplicate".into(),
                    "Pin both artifacts of the pair to the same version in pom.xml".into(),
                ],
            }
        },
    },
    Rule {
        signatures: &["package", "does not exist"],
        group: "missing-dependency",
        explain: |log| {
            let package = capture_between(log, "package ", " does not exist").unwrap_or_default();
            Diagnosis {
                headline: format!(
                    "A dependency is missing from pom.xml{}",
                    if package.is_empty() {
                        String::new()
                    } else {
                        format!(" (package {})", package.trim())
                    }
                ),
                because: "javac cannot see a package the source imports. The import is right and \
                          the dependency is absent -- as opposed to `cannot find symbol`, which \
                          usually means a typo or a class you have not written yet."
                    .into(),
                fixes: vec![
                    "jails add <capability>   # csv, sqlite, json, db, kafka, http, testkit, fake, format".into(),
                    "Or splice the dependency into pom.xml by hand".into(),
                ],
            }
        },
    },
    Rule {
        signatures: &["cannot find symbol"],
        group: "compile",
        explain: |_| {
            Diagnosis {
            headline: "A compile error, not a runtime one".into(),
            because: "javac cannot resolve a name. In a jails project the common causes are a \
                      class that was renamed on one side only, a missing import (generated code \
                      keeps imports normalised, so a hand-added type may have none), or a record \
                      component added without updating its call sites."
                .into(),
            fixes: vec![
                "In Neovim: <leader>je puts every project-wide compile error in the quickfix \
                 list, then ]q walks them -- this is the change-signature workflow Java's LSP \
                 does not provide."
                    .into(),
                "<leader>jq on the error for a one-symbol import fix; <leader>ji to organize imports".into(),
                "jails check     # format check + clean compile + tests".into(),
            ],
        }
        },
    },
    Rule {
        signatures: &["Ryuk", "Timed out"],
        group: "ryuk",
        explain: |_| Diagnosis {
            headline: "Testcontainers' cleanup container (Ryuk) could not start".into(),
            because: "Ryuk is a privileged helper Testcontainers starts to reap containers after \
                      the test run. Rootless podman commonly refuses it. Disabling it is safe \
                      locally; containers are then cleaned up at JVM exit instead."
                .into(),
            fixes: vec!["export TESTCONTAINERS_RYUK_DISABLED=true".into()],
        },
    },
];

pub fn why(input: Option<&Path>, debug: bool, json: bool) -> Result<()> {
    let log = read_input(input, debug)?;
    if log.trim().is_empty() {
        return Err("nothing to explain -- pass a log file, pipe one in, or run `jails why` with no input to start the app and read its output".into());
    }

    let found = explain(&log);
    if json {
        println!("{}", diagnoses_json(&found));
        return Ok(());
    }
    if found.is_empty() {
        println!("jails does not recognise this failure.");
        println!();
        println!(
            "It matches none of the {} failure shapes it knows about. Two things that",
            RULES.len()
        );
        println!("narrow down an unknown one:");
        println!();
        println!("  jails doctor    everything that has to be true before the app can start");
        println!("  jails beans     what is registered, and which dependencies resolve to nothing");
        println!();
        println!("The first `Caused by:` line from the bottom of the trace is the real cause;");
        println!("everything above it is Spring re-wrapping it.");
        return Ok(());
    }

    print_report(&found);
    Ok(())
}

fn diagnoses_json(found: &[Diagnosis]) -> String {
    let diagnoses = found
        .iter()
        .map(|diagnosis| {
            let fixes = diagnosis
                .fixes
                .iter()
                .map(|fix| crate::project::json_string(fix))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"headline\":{},\"because\":{},\"fixes\":[{}]}}",
                crate::project::json_string(&diagnosis.headline),
                crate::project::json_string(&diagnosis.because),
                fixes
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema_version\":3,\"recognized\":{},\"diagnoses\":[{}]}}",
        !found.is_empty(),
        diagnoses
    )
}

fn print_report(found: &[Diagnosis]) {
    for (index, diagnosis) in found.iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("{}", diagnosis.headline);
        println!();
        for line in wrap(&diagnosis.because, 76) {
            println!("  {line}");
        }
        println!();
        for fix in &diagnosis.fixes {
            // `$` marks a line that can be pasted into a shell; prose
            // instructions get a bullet, because a `$` in front of "Annotate
            // the implementation" invites exactly the wrong reflex.
            println!("  {} {fix}", if is_command(fix) { "$" } else { "-" });
        }
    }
    if found.len() > 1 {
        println!();
        println!(
            "{} failures matched. The first one listed is usually the cause and the rest \
             its consequences.",
            found.len()
        );
    }
}

/// Output that means the application is dead, whatever the exit code says.
///
/// This exists because `mvn spring-boot:run` exits 0 on a failed startup:
/// spring-boot-devtools runs `main` on its own `restartedMain` thread, the
/// startup exception is caught there, and Maven -- which only ever saw the
/// plugin return normally -- prints BUILD SUCCESS over the top of a stack
/// trace. Without this, `jails run` reports success for an app that never
/// came up.
const FATAL_MARKERS: [&str; 4] = [
    // Spring's own log line when the context fails to refresh.
    "Application run failed",
    // The failure-analyzer banner, for the failures it has a report for.
    "APPLICATION FAILED TO START",
    "Exception in thread \"main\"",
    "Exception in thread \"restartedMain\"",
];

pub(crate) fn looks_fatal(log: &str) -> bool {
    FATAL_MARKERS.iter().any(|marker| log.contains(marker))
}

/// Explain a captured run that failed. Returns how many failures were
/// recognised, so the caller can say something useful about zero.
pub(crate) fn report(log: &str) -> usize {
    let found = explain(log);
    print_report(&found);
    found.len()
}

/// Rules whose every signature appears in the log, most specific first --
/// a two-signature rule outranks a one-signature rule, so "Port ... already
/// in use" is reported ahead of a generic bean failure it cascaded into.
fn explain(log: &str) -> Vec<Diagnosis> {
    let mut matched: Vec<(usize, &Rule)> = RULES
        .iter()
        .filter(|rule| rule.signatures.iter().all(|s| log.contains(s)))
        .map(|rule| (rule.signatures.len(), rule))
        .collect();
    matched.sort_by(|a, b| b.0.cmp(&a.0));
    let mut seen: Vec<&str> = Vec::new();
    let mut found = Vec::new();
    for (_, rule) in matched {
        if seen.contains(&rule.group) {
            continue;
        }
        seen.push(rule.group);
        found.push((rule.explain)(log));
    }
    found
}

fn read_input(input: Option<&Path>, debug: bool) -> Result<String> {
    if let Some(path) = input {
        return std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()));
    }
    if !std::io::stdin().is_terminal() {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .map_err(|e| format!("failed to read stdin: {e}"))?;
        return Ok(buffer);
    }
    run_and_capture(debug)
}

/// Start the application, echo its output as it arrives, and keep a copy to
/// diagnose. Echoing matters: a run that takes forty seconds to fail must
/// not look like a hang.
fn run_and_capture(debug: bool) -> Result<String> {
    let root = find_project_root()?;
    let pom_text = pom::read(&root)?;
    let mut cmd = Command::new(run::maven_binary(&root));
    match pom::flavor(&pom_text) {
        pom::Flavor::SpringBoot => {
            cmd.arg("spring-boot:run");
        }
        // A plain Maven project has no run goal; compiling and testing is
        // the failure surface it does have.
        pom::Flavor::PlainMaven => {
            cmd.arg("verify");
        }
    }
    cmd.current_dir(&root)
        .stdout(Stdio::piped())
        // Merged rather than captured separately: interleaving order is what
        // makes a log readable, and two pipes read by two threads cannot
        // preserve it.
        .stderr(Stdio::piped());
    if debug {
        crate::debug_cmd(&cmd);
    }

    println!("jails why: starting the app to see how it fails (Ctrl-C once it has)...");
    println!();
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to start Maven: {e}"))?;

    let stderr = child.stderr.take();
    let collector = std::thread::spawn(move || {
        let mut buffer = String::new();
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_string(&mut buffer);
        }
        buffer
    });

    let mut captured = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let mut chunk = [0u8; 4096];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&chunk[..n]);
                    print!("{text}");
                    captured.push_str(&text);
                }
            }
        }
    }
    let _ = child.wait();
    if let Ok(errors) = collector.join() {
        eprint!("{errors}");
        captured.push_str(&errors);
    }
    println!();
    Ok(captured)
}

/// Whether a fix line is literally runnable. The set is closed on purpose:
/// anything not recognised is prose and is marked as prose.
fn is_command(fix: &str) -> bool {
    const RUNNABLE: [&str; 9] = [
        "jails ",
        "export ",
        "lsof ",
        "kill ",
        "systemctl ",
        "mise ",
        "mvn ",
        "docker ",
        "echo ",
    ];
    RUNNABLE.iter().any(|prefix| fix.starts_with(prefix))
}

/// The text between two markers, first occurrence only.
fn capture_between(text: &str, open: &str, close: &str) -> Option<String> {
    let start = text.find(open)? + open.len();
    let rest = &text[start..];
    let end = rest.find(close)?;
    Some(rest[..end].to_string())
}

/// Greedy wrap at word boundaries. Explanations are prose and a terminal is
/// not guaranteed to wrap them anywhere sensible.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.len() + 1 + word.len() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_bean_is_named_in_the_headline() {
        let log = "Parameter 0 of constructor in com.example.rewards.application.RewardHistoryService \
                   required a bean of type 'com.example.rewards.persistence.RewardRepository' that \
                   could not be found.";
        let found = explain(log);
        assert_eq!(found.len(), 1, "{}", found.len());
        assert!(
            found[0].headline.contains("RewardRepository"),
            "{}",
            found[0].headline
        );
        assert!(found[0].fixes.iter().any(|f| f.contains("jails beans")));
    }

    #[test]
    fn the_datasource_failure_points_at_the_test_initializer() {
        let log = "Failed to configure a DataSource: 'url' attribute is not specified\n\
                   Reason: Failed to determine a suitable driver class";
        let found = explain(log);
        assert_eq!(found.len(), 1);
        assert!(
            found[0]
                .because
                .contains("@Import(TestcontainersConfig.class)"),
            "{}",
            found[0].because
        );
    }

    #[test]
    fn the_docker_failure_talks_about_podman_sockets() {
        let log = "Caused by: java.lang.IllegalStateException: Could not find a valid Docker \
                   environment. Please see logs and check configuration";
        let found = explain(log);
        assert_eq!(found.len(), 1);
        assert!(found[0].fixes.iter().any(|f| f.contains("DOCKER_HOST")));
    }

    #[test]
    fn a_port_clash_reports_the_port_number() {
        let log = "Web server failed to start. Port 8081 was already in use.";
        let found = explain(log);
        assert!(found[0].headline.contains("8081"), "{}", found[0].headline);
        assert!(found[0].fixes.iter().any(|f| f.contains("8081")));
    }

    #[test]
    fn a_missing_table_is_named() {
        let log = r#"ERROR: relation "rewards" does not exist"#;
        let found = explain(log);
        assert!(
            found[0].headline.contains("rewards"),
            "{}",
            found[0].headline
        );
    }

    #[test]
    fn more_specific_rules_are_reported_first() {
        // A port clash (two signatures) cascades into a generic bean creation
        // failure; the specific cause has to lead.
        let log = "Port 8080 was already in use. ... cannot find symbol";
        let found = explain(log);
        assert!(found[0].headline.contains("8080"), "{}", found[0].headline);
    }

    #[test]
    fn a_devtools_startup_failure_is_fatal_despite_a_zero_exit() {
        // The exact shape `mvn spring-boot:run` exits 0 on.
        let log = "ERROR 288515 --- [rewards] [  restartedMain] o.s.boot.SpringApplication \
                   : Application run failed\n[INFO] BUILD SUCCESS";
        assert!(looks_fatal(log));
        assert!(!looks_fatal(
            "[INFO] BUILD SUCCESS\nStarted RewardsApplication in 1.2 seconds"
        ));
    }

    /// Every distinct root cause found in a month of this machine's real
    /// session logs, with the occurrence count that justified the rule.
    /// Measured before writing them: 2 of 6 were recognised.
    #[test]
    fn every_root_cause_seen_in_real_logs_is_recognised() {
        let observed: [(&str, &str); 6] = [
            // 8x
            (
                "UnsatisfiedDependencyException",
                "Caused by: org.springframework.beans.factory.UnsatisfiedDependencyException: \
                 Error creating bean with name 'rewardController'",
            ),
            // 8x
            (
                "dataSource / Hikari",
                "Caused by: org.springframework.beans.BeanInstantiationException: Failed to \
                 instantiate [com.zaxxer.hikari.HikariDataSource]: Failed to determine a suitable \
                 driver class",
            ),
            // 7x -- the cached-probe variant, more common than the original
            (
                "Testcontainers retry",
                "Caused by: java.lang.IllegalStateException: Previous attempts to find a Docker \
                 environment failed. Will not retry.",
            ),
            // 4x
            (
                "Testcontainers first failure",
                "Caused by: java.lang.IllegalStateException: Could not find a valid Docker \
                 environment.",
            ),
            // 1x
            (
                "mvnd registry",
                "Caused by: java.nio.file.FileSystemException: \
                 /home/laith/.m2/mvnd/registry/1.0.6/registry.bin: Read-only file system",
            ),
            // 33x -- `jails console`, whose jshell needs a loopback socket
            (
                "jshell execution engine",
                "Launching JShell execution engine threw: FailOverExecutionControlProvider: \
                 FAILED: 0:jdi:hostname(127.0.0.1)",
            ),
        ];
        for (label, log) in observed {
            assert!(
                !explain(log).is_empty(),
                "no rule matches a failure that really happened: {label}"
            );
        }
    }

    #[test]
    fn an_unrecognised_failure_matches_nothing() {
        assert!(explain("something entirely novel went wrong").is_empty());
    }

    #[test]
    fn json_is_versioned_and_keeps_fixes_as_an_array() {
        let found = explain("Web server failed to start. Port 8081 was already in use.");
        let json = diagnoses_json(&found);
        assert!(
            json.starts_with("{\"schema_version\":3,\"recognized\":true"),
            "{json}"
        );
        assert!(json.contains("\"headline\":"), "{json}");
        assert!(json.contains("\"fixes\":["), "{json}");
        assert!(json.contains("8081"), "{json}");
    }

    #[test]
    fn unknown_json_is_an_empty_machine_readable_result() {
        assert_eq!(
            diagnoses_json(&[]),
            "{\"schema_version\":3,\"recognized\":false,\"diagnoses\":[]}"
        );
    }

    #[test]
    fn the_podman_compose_provider_failure_is_recognised() {
        let log = "Error: executing /usr/bin/podman-compose --file compose.yaml --ansi never \
                   config --format=json: exit status 2\n\
                   at org.springframework.boot.docker.compose.core.ProcessRunner.run \
                   ~[spring-boot-docker-compose-4.1.0.jar:4.1.0]";
        let found = explain(log);
        assert_eq!(found.len(), 1, "{}", found.len());
        assert!(
            found[0].headline.contains("podman-compose"),
            "{}",
            found[0].headline
        );
        assert!(
            found[0]
                .fixes
                .iter()
                .any(|f| f.contains("spring.docker.compose.enabled=false")),
            "{:?}",
            found[0].fixes
        );
    }

    #[test]
    fn an_ambiguous_injection_point_is_told_apart_from_a_missing_one() {
        let log = "Parameter 0 of constructor in RewardHistoryService required a single bean, \
                   but 2 were found:\n\t- inMemoryRewardRepository: defined in file [x]\n\
                   \t- jdbcRewardRepository: defined in file [y]";
        let found = explain(log);
        assert_eq!(found.len(), 1, "{}", found.len());
        assert!(
            found[0].headline.contains("Two or more"),
            "{}",
            found[0].headline
        );
        assert!(
            found[0].because.contains("inMemoryRewardRepository"),
            "{}",
            found[0].because
        );
        assert!(found[0].fixes.iter().any(|f| f.contains("@Primary")));
    }

    #[test]
    fn both_spellings_of_a_missing_bean_report_once() {
        // Spring emits both messages for the same failure; reporting two
        // explanations of one cause is noise.
        let log = "No qualifying bean of type 'p.RewardRepository' available\n\
                   required a bean of type 'p.RewardRepository' that could not be found";
        assert_eq!(explain(log).len(), 1);
    }

    #[test]
    fn only_runnable_fix_lines_are_marked_as_commands() {
        assert!(is_command("jails doctor"));
        assert!(is_command("export DOCKER_HOST=unix:///x"));
        assert!(!is_command("Annotate the implementation with @Service"));
    }

    #[test]
    fn wrap_breaks_on_words_only() {
        let wrapped = wrap("one two three four five", 9);
        assert!(wrapped.iter().all(|l| l.len() <= 9), "{wrapped:?}");
        assert_eq!(wrapped.join(" "), "one two three four five");
    }
}
