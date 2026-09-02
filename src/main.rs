//! The binary: the clap tree, and dispatch to it.
//!
//! **Argument parsing and nothing else.** Every subcommand's work lives in a
//! crate below; what is here is the `clap` derive tree, the module list, and
//! `main`'s translation of a `Failure` into an exit status. That boundary is
//! what lets `jails commands` walk this same tree to describe the CLI, so
//! there is no second list of subcommands, kinds or capabilities anywhere.
//!
//! Two conventions the tree depends on and neither is decorative: anything
//! meant to be typed interactively uses `visible_alias`, because a hidden
//! `alias` is invisible to `clap_complete`; and any argument with a closed
//! value set is a `ValueEnum` rather than a `String` matched by hand, because
//! that is the only shape a static completion list can be generated from.
//!
//! A failure exits non-zero through an *empty* `Err` where the command has
//! already printed its own report, so `main` does not add a redundant
//! `jails: ` line under a formatted one.

mod adopt;
mod app;
mod arguments;
mod canonical_support;
mod cli;
mod contract_command;
mod dispatch;
mod editor_command;
mod facade;
mod model_capability;
mod model_command;
mod model_destroy;
mod model_doctor;
mod model_eject;
mod model_explain;
mod model_field_evolution;
mod model_generate;
mod model_generate_jdl;
mod model_index;
mod model_init;
mod model_migration;
mod model_rename;
mod model_resource;
mod model_setting;
mod model_status;
mod modernize;
mod new;
mod parse_error;
mod plan_command;
mod rename_source;
mod template_macro;
mod tool_command;

// What the CLI accepts lives in `cli`; what it does is the match below.
pub(crate) use cli::{
    Cli, Command, Declare, Invocation, Output, ResourceCommand, ResourceIndexCommand, Undeclare,
};
pub(crate) use facade::*;

use clap::{CommandFactory, Parser};

pub(crate) use template_macro::template_here;

fn main() -> std::process::ExitCode {
    if let Some(result) = plan_command::requested() {
        return dispatch::finish(result);
    }
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return parse_error::render(error),
    };
    let debug = cli.debug;
    let pretend = cli.pretend;

    let invocation = Invocation {
        // The process directory, unless a caller resolves one: see
        // `Invocation::at`.
        root: None,
        pretend,
        debug,
        output: cli.output,
        diff: cli.diff,
        ast: cli.ast,
        plan_out: cli.plan_out,
        plan_in: cli.plan_in,
        command_path: cli::command_path_from_env(),
        force: false,
        // **Only `add` starts a service, and that is the CLI's own shape.**
        // `--no-start` exists on the commands that install or run something;
        // `jails g scaffold` on a project that already declares a database
        // has never brought one up, and making every mutation run the plan's
        // effects would. The `Add` arm reads the flag; everything else leaves
        // what is running alone.
        no_start: true,
    };
    let failure_output = invocation.output;
    let failure_path = invocation.command_path.clone();
    let result = match cli.command {
        Command::About { json } => project::about(json),
        Command::New(args) => new::new(new::request(&args, debug, pretend)),
        Command::NewCli(args) => new::new_cli(&new::cli_request(&args, debug, pretend)),
        Command::App { command } => app::run(command, invocation),
        Command::Model { command } => model_command::run(command, invocation),
        Command::Editor { command } => editor_command::run(command, invocation),
        Command::Contract { command } => contract_command::run(command, invocation),
        Command::Request { request } => tool_command::request(
            tool_command::HttpRequest {
                method: request.method,
                target: request.target,
                profile: request.profile,
                base_url: request.base_url,
                params: request.params,
                query: request.query,
                headers: request.headers,
                header_env: request.header_env,
                json: request.json,
                data: request.data,
                timeout: request.timeout,
                follow: request.follow,
                print: request.print,
            },
            invocation,
        ),
        Command::Runner { runner } => tool_command::runner(
            &runner.file,
            &runner.profiles,
            runner.main.as_deref(),
            runner.web,
            runner.compile,
            runner.yes,
            invocation,
        ),
        Command::Logs { logs } => tool_command::logs(
            &logs.services,
            logs.follow,
            logs.since.as_deref(),
            logs.tail,
            invocation,
        ),
        Command::Architecture { action } => match action {
            cli::ArchitectureAction::Baseline => jails_drive::baseline::freeze(invocation.debug),
        },
        Command::Generate(args) => {
            return dispatch::finish_invocation(
                model_command::ensure_owned(invocation.clone())
                    .and_then(|()| model_generate_jdl::run(args, invocation)),
                failure_output,
                &failure_path,
            );
        }
        Command::Add {
            declare:
                Some(Declare::Dependency {
                    coordinate,
                    version,
                    scope,
                }),
            ..
        } => model_command::ensure_owned(invocation.clone()).and_then(|()| {
            arguments::maven_coordinate(&coordinate).and_then(|coordinate| {
                model_capability::add_dependency(coordinate, version, scope.canonical(), invocation)
            })
        }),
        Command::Add {
            capabilities,
            name,
            no_start,
            package,
            declare: None,
        } => {
            let invocation = invocation.without_starting(no_start);
            return dispatch::finish_invocation(
                model_command::ensure_owned(invocation.clone())
                    .and_then(|()| model_capability::add(capabilities, name, package, invocation)),
                failure_output,
                &failure_path,
            );
        }
        Command::Sync { no_start } => model_command::ensure_owned(invocation.clone())
            .and_then(|()| model_command::sync(no_start, invocation)),
        Command::Remove {
            capabilities,
            name,
            force,
            package,
            undeclare,
        } => {
            let invocation = invocation.forcing(force);
            let result =
                model_command::ensure_owned(invocation.clone()).and_then(|()| match undeclare {
                    None => model_capability::remove(capabilities, name, package, invocation),
                    Some(Undeclare::Dependency { coordinate }) => {
                        arguments::maven_coordinate(&coordinate).and_then(|coordinate| {
                            model_capability::remove_dependency(coordinate, invocation)
                        })
                    }
                    Some(Undeclare::FastTest { force }) => {
                        model_capability::remove_fast_test(invocation.forcing(force))
                    }
                });
            return dispatch::finish_invocation(result, failure_output, &failure_path);
        }
        Command::Set { setting, tests } => model_command::ensure_owned(invocation.clone())
            .and_then(|()| {
                arguments::split_setting(&setting)
                    .and_then(|(key, value)| model_setting::set(key, value, tests, invocation))
            }),
        Command::Unset { key, tests } => model_command::ensure_owned(invocation.clone())
            .and_then(|()| model_setting::unset(key, tests, invocation)),
        Command::Rename {
            command,
            old,
            new,
            force,
        } => {
            // **Two different operations under one verb, and they stay
            // apart.** `rename resource` moves a *declared* resource -- the
            // model, the table and the managed tree together. The bare
            // `rename OLD NEW` is a textual identifier sweep over the reader's
            // own sources, for a type the model does not declare; it needs no
            // model and initialises none, which is why it is dispatched before
            // `ensure_owned` rather than inside it.
            if command.is_none() {
                let (Some(old), Some(new)) = (old, new) else {
                    let result = Err(jails_support::Failure::Told(
                        "`jails rename` takes either a resource -- `rename resource <current> <new> --strategy ...` -- or two simple type names.\n       fix: name what you are renaming".to_string(),
                    ));
                    return dispatch::finish_invocation(result, failure_output, &failure_path);
                };
                let result = rename_source::run(&old, &new, force, invocation);
                return dispatch::finish_invocation(result, failure_output, &failure_path);
            }
            let result =
                model_command::ensure_owned(invocation.clone()).and_then(|()| match command {
                    Some(cli::RenameCommand::Resource {
                        from,
                        to,
                        strategy,
                        table,
                        api,
                        route,
                        force: _,
                    }) => model_rename::run(
                        model_rename::Request {
                            from,
                            to,
                            strategy,
                            table,
                            api,
                            route,
                        },
                        invocation,
                    ),
                    None => unreachable!("the textual rename is dispatched above"),
                });
            return dispatch::finish_invocation(result, failure_output, &failure_path);
        }
        Command::Destroy {
            kind,
            name,
            force,
            package,
            storage,
            confirm_table,
            migrate,
            datasource,
        } => {
            // Canonical removal is model subtraction plus an explicit storage
            // policy, so the migration flags are refused and `--storage drop`
            // is the confirmation. `--force` has a narrower meaning -- the
            // reader saying that edits to the files being removed may go with
            // them.
            let invocation = invocation.forcing(force);
            let result = model_command::ensure_owned(invocation.clone()).and_then(|()| {
                model_destroy::run(
                    model_destroy::Request {
                        kind,
                        name,
                        package: package.is_some(),
                        storage,
                        confirm_table,
                        migration_effect: migrate || datasource.is_some(),
                    },
                    invocation,
                )
            });
            return dispatch::finish_invocation(result, failure_output, &failure_path);
        }
        Command::Resource { command } => match command {
            ResourceCommand::Status { selector } => model_command::ensure_owned(invocation.clone())
                .and_then(|()| model_status::run(&selector, None, invocation)),
            ResourceCommand::Revive { selector, table } => {
                model_command::ensure_owned(invocation.clone())
                    .and_then(|()| model_destroy::revive(selector, table, invocation))
            }
            // Canonical repair is `sync` with the deleted-managed-file guard
            // waived, so it takes no strategy: managed output is reproducible
            // from the model, and the model is the only strategy there is.
            //
            // A selector is refused rather than ignored: compilation is
            // whole-model, so scoping it to one resource is not something this
            // can honour. `--datasource` *is* honoured, and asks the one
            // question the files cannot answer -- whether the image Flyway
            // actually applied is the one the seal would restore.
            ResourceCommand::Repair { selector } => {
                if selector.is_some() {
                    Err(jails_support::Failure::Told(
                        "canonical `resource repair` repairs the whole managed tree and takes no selector: it renders `.jails/generated` from the model.\n       fix: run `jails resource repair` with no selector".to_string(),
                    ))
                } else {
                    model_command::ensure_owned(invocation.clone())
                        .and_then(|()| model_command::repair(invocation))
                }
            }
            ResourceCommand::Index { command } => model_index::run(command, invocation),
            ResourceCommand::Field { command } => model_resource::run(command, invocation),
        },
        Command::Start { services } => compose::start(&services, debug),
        Command::Stop { services } => compose::stop_cmd(&services, debug),
        Command::Adopt => adopt::layout(invocation),
        Command::Modernize => modernize::run(invocation),
        Command::Src { type_name, json } => source::src(&type_name, json),
        Command::Bench {
            vus,
            duration,
            export,
        } => bench::bench(
            bench::Profile {
                vus,
                duration,
                export,
            },
            debug,
        ),
        Command::Doctor { json } => doctor::doctor(json, crate::model_doctor::checks()),
        Command::Why {
            log,
            name,
            last,
            evidence,
            json,
        } => why::command(log.as_deref(), name.as_deref(), last, evidence, debug, json),
        Command::Stats { json } => inspect::stats(json),
        Command::Notes { tag, json } => inspect::notes(tag.as_deref(), json),
        Command::Routes { json } => inspect::routes(json),
        Command::Beans { pattern, json } => inspect::beans(pattern.as_deref(), json),
        Command::Migrate { check, no_start } => match () {
            () if !check => Err(
                "`--check` is the only mode jails has: it applies the migrations to a \
                     scratch database and drops it. Applying them for real is Flyway's job, \
                     which the application does at startup.\n\nfix: run `jails migrate`."
                    .into(),
            ),
            () => migrate::check(no_start, debug),
        },
        Command::Kafka { command, no_start } => kafka::kafka(command, no_start, debug),
        Command::Lint => lint::lint(),
        Command::Db {
            command,
            file,
            web,
            no_start,
            args,
        } => match command {
            Some(cli::DbCommand::Console {
                database,
                profile,
                client,
                single_connection,
            }) => tool_command::db_console(
                database.as_deref(),
                profile.as_deref(),
                client,
                single_connection,
                invocation,
            ),
            None => console::db(file.as_deref(), web, no_start, &args, debug),
        },
        Command::Console { console } => tool_command::console(
            &console.profiles,
            console.main.as_deref(),
            console.web,
            console.compile,
            console.yes,
            &console.args,
            invocation,
        ),
        Command::Test {
            requested,
            scope,
            compile,
            engine,
            watch,
            affected,
            failed,
            tags,
            fail_fast,
            slowest,
            json,
            fast,
            until_fail,
            repeat,
            timeout,
            db,
            explain_selection,
            command,
        } => {
            if let Some(cli::TestCommand::Daemon { action }) = command {
                return match action {
                    cli::TestDaemonAction::Status => testd::testd(testd::Action::Status, debug),
                    cli::TestDaemonAction::Stop => testd::testd(testd::Action::Stop, debug),
                    cli::TestDaemonAction::Restart => testd::testd(testd::Action::Restart, debug),
                }
                .map(|()| std::process::ExitCode::SUCCESS)
                .unwrap_or_else(|error| {
                    eprintln!("jails: {error}");
                    std::process::ExitCode::FAILURE
                });
            }
            // The launcher class has to be on the test classpath before
            // `--fast` can run anything, and that is a dependency in the
            // reader's POM -- so it is an owned entity installed by an
            // ordinary transition, not a side effect of how the tests were
            // run, and `jails remove fast-test` takes it back out. Idempotent,
            // so every later `--fast` writes nothing.
            let options = run::TestOptions {
                scope: scope.into(),
                compile: compile.into(),
                engine: engine.into(),
                watch,
                affected,
                failed,
                tags,
                fail_fast,
                slowest,
                json: json || invocation.output.is_json(),
                fast,
                until_fail,
                repeat,
                timeout,
                database_schema: db == cli::TestDatabaseArg::Schema,
                explain_selection,
            };
            let installed = run::validate_test_options(&options).and_then(|()| {
                match fast
                    || engine == cli::TestEngineArg::Warm
                    || (engine == cli::TestEngineArg::Auto
                        && matches!(
                            compile,
                            cli::TestCompileArg::Ide | cli::TestCompileArg::None
                        ))
                    || (affected && engine != cli::TestEngineArg::Build)
                {
                    true => model_command::ensure_owned(invocation.clone())
                        .and_then(|()| model_capability::ensure_fast_test(invocation)),
                    false => Ok(()),
                }
            });
            installed.and_then(|()| run::test(&requested, options, debug))
        }
        Command::Testd {
            filter,
            affected,
            stop,
            status,
        } => {
            let action = if stop {
                testd::Action::Stop
            } else if status {
                testd::Action::Status
            } else if affected {
                testd::Action::Affected
            } else {
                testd::Action::Run(filter)
            };
            println!(
                "note: `jails testd` is a compatibility alias; use `jails test --engine warm \
                 --compile none` or `jails test daemon ...`"
            );
            testd::testd(action, debug)
        }
        Command::Build => run::build(debug),
        Command::Clean => run::clean(debug),
        // A canonical project's generated tree is compiler output, so the
        // only thing left for a formatter to touch is the reader's own code.
        // See `run::format_project`.
        Command::Fmt => run::format_project(debug),
        Command::Check => run::check(debug),
        Command::Mvn { args } => run::mvn(&args, debug),
        Command::Gradle { args } => run::gradle(&args, debug),
        Command::Run {
            no_build,
            launcher,
            compile,
            services,
            profiles,
            watch,
            args,
        } => run::run(
            run::RunOptions {
                launcher: match launcher {
                    cli::RunLauncherArg::Auto => run::RunLauncher::Auto,
                    cli::RunLauncherArg::Classpath => run::RunLauncher::Classpath,
                    cli::RunLauncherArg::BuildTool => run::RunLauncher::BuildTool,
                    cli::RunLauncherArg::Jar => run::RunLauncher::Jar,
                },
                compile: if no_build {
                    run::RunCompile::None
                } else {
                    match compile {
                        cli::RunCompileArg::Auto => run::RunCompile::Auto,
                        cli::RunCompileArg::Ide => run::RunCompile::Ide,
                        cli::RunCompileArg::Build => run::RunCompile::Build,
                        cli::RunCompileArg::None => run::RunCompile::None,
                    }
                },
                services: match services {
                    cli::RunServicesArg::Existing => run::RunServices::Existing,
                    cli::RunServicesArg::Start => run::RunServices::Start,
                    cli::RunServicesArg::None => run::RunServices::None,
                },
                profiles,
                watch,
            },
            &args,
            debug,
        ),
        Command::Setup {} => doctor::setup(pretend),
        Command::Explain { kind } => explain::explain(kind),
        Command::Commands { json } => commands::commands(Cli::command(), json),
        Command::Completion { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "jails", &mut std::io::stdout());
            Ok(())
        }
    };

    dispatch::finish_invocation(result, failure_output, &failure_path)
}

/// These two assert against jails' *real* CLI, so they live with it.
///
/// They cannot live in `commands.rs`: reaching for `crate::Cli` from there is
/// a cycle, since that module is one layer below the binary. `commands` takes
/// the `clap::Command` as an argument and exposes its two walkers, so what is
/// asserted here is the command that parses the arguments rather than a
/// fixture resembling it.
#[cfg(test)]
mod tests {
    use super::*;
    use jails_report::commands;

    #[test]
    fn visible_aliases_are_carried_because_completion_cannot_see_hidden_ones() {
        let command = Cli::command();
        let subs = commands::subcommands(&command);
        let generate = subs
            .iter()
            .find(|entry| entry.name == "generate")
            .expect("generate is a subcommand");
        assert!(
            generate.aliases.iter().any(|alias| alias == "g"),
            "{:?}",
            generate.aliases
        );
    }

    #[test]
    fn the_global_pretend_flag_and_its_alias_reach_the_option_list() {
        let flags = commands::options(&Cli::command());
        assert!(flags.contains(&"--pretend".to_string()), "{flags:?}");
    }
}
