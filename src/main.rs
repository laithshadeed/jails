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

mod app;
mod arguments;
mod canonical_support;
mod cli;
mod contract_command;
mod dispatch;
mod editor_command;
mod facade;
mod history_command;
mod model_capability;
mod model_command;
mod model_destroy;
mod model_eject;
mod model_field_evolution;
mod model_generate;
mod model_generate_jdl;
mod model_import;
mod model_index;
mod model_migration;
mod model_rename;
mod model_resource;
mod model_setting;
mod model_upgrade;
mod new;
mod parse_error;
mod plan_command;
mod schema_command;
mod sql_command;
mod template_macro;
mod tool_command;

// What the CLI accepts lives in `cli`; what it does is the match below.
pub(crate) use cli::{
    Cli, Command, Declare, Invocation, Output, ResourceCommand, ResourceIndexCommand, SqlCommand,
    Undeclare,
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
        pretend,
        debug,
        output: cli.output,
        diff: cli.diff,
        ast: cli.ast,
        plan_out: cli.plan_out,
        plan_in: cli.plan_in,
        command_path: cli::command_path_from_env(),
    };
    let failure_output = invocation.output;
    let failure_path = invocation.command_path.clone();
    let result = match cli.command {
        Command::About { json } => project::about(json),
        Command::New(args) => new::new(new::request(&args, debug, pretend)),
        Command::NewCli(args) => new::new_cli(&new::cli_request(&args, debug, pretend)),
        Command::App { command } => app::run(command, invocation),
        Command::Model { command } => model_command::run(command, invocation),
        Command::Sql { command } => sql_command::run(command, invocation),
        Command::Introspect { command } => schema_command::introspect(command, invocation),
        Command::Pull {
            datasource,
            schema,
            table,
            into_slice,
            services,
        } => schema_command::pull(
            &datasource,
            &schema,
            table.as_deref(),
            into_slice.as_deref(),
            services,
            invocation,
        ),
        Command::Schema { command } => schema_command::schema(command, invocation),
        Command::Editor { command } => editor_command::run(command, invocation),
        Command::Contract { command } => contract_command::run(command, invocation),
        Command::History(history) => history_command::history(history.limit, invocation.output),
        Command::Show(show) => {
            history_command::show(&show.transaction, invocation.diff, show.why, invocation.output)
        }
        Command::Undo(_) if model_command::owns() => model_command::refuse_legacy_mutation(
            "undo",
            "restore from version control or apply a reviewed forward model patch",
        ),
        Command::Undo(undo) => dispatch::mutate(invocation, false, |run| {
            jails_engine::route::undo_files(run, &undo.transaction, undo.merge)
        }),
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
            cli::ArchitectureAction::Baseline => {
                jails_drive::baseline::freeze(invocation.debug)
            }
        },
        Command::Generate(args) => {
            if model_generate::owns(&args) {
                return dispatch::finish_invocation(
                    model_generate::run(args, invocation),
                    failure_output,
                    &failure_path,
                );
            }
            // Built once, outside the closure: a route may be called twice --
            // a plan for a confirmation, then the commit -- and the intent is
            // the same request both times.
            let default_literal = args.default_literal.clone();
            let backfill_file = args.backfill_file.clone();
            let intent = jails_engine::route::Intent::from(args);
            dispatch::mutate(invocation, false, |run| {
                jails_engine::route::recipe_with_field_data(
                    run,
                    &intent,
                    default_literal.as_deref(),
                    backfill_file.as_deref(),
                )
            })
        }
        Command::Add {
            declare:
                Some(Declare::Dependency {
                    coordinate,
                    version,
                    scope,
            }),
            ..
        } => {
            arguments::maven_coordinate(&coordinate).and_then(|coordinate| {
                if model_capability::owns() {
                    return model_capability::add_dependency(
                        coordinate,
                        version,
                        scope.canonical(),
                        invocation,
                    );
                }
                dispatch::mutate(invocation, false, |run| {
                    jails_engine::route::add_dependency(
                        run,
                        coordinate.clone(),
                        version.clone(),
                        scope.resolved(),
                    )
                })
            })
        }
        Command::Add {
            capabilities,
            name,
            no_start,
            package,
            declare: None,
        } => {
            if model_capability::owns() {
                return dispatch::finish_invocation(
                    model_capability::add(
                        capabilities,
                        name,
                        package,
                        invocation,
                    ),
                    failure_output,
                    &failure_path,
                );
            }
            dispatch::mutate(invocation, no_start, |run| {
                // Every capability is checked before any is applied. Each one is
                // its own transition, so without this `jails add db security` on a
                // plain Maven project would install the database and *then* refuse
                // -- leaving the reader with half of what they asked for and no
                // word about which half.
                add::preflight_in(
                    run.project(),
                    &capabilities,
                    name.as_deref(),
                    package.as_deref(),
                )?;
                let asked =
                    dispatch::declarations(&capabilities, name.as_deref(), package.as_deref())?;
                dispatch::one_transition_each(run, &asked, jails_engine::route::install)
            })
        }
        Command::Sync { no_start } if model_command::owns() => {
            model_command::sync(no_start, invocation)
        }
        Command::Sync { no_start } => dispatch::mutate(invocation, no_start, |run| {
            // Most projects never write a manifest, so an empty list is not an
            // error and "nothing to do" would not explain itself. Said before
            // the transition rather than inside it: what follows is a real
            // reconciliation of an empty list, and this is advice about the
            // file that would give it something to do.
            if run.project().declarations().is_empty() {
                println!(
                    "note: no capabilities are declared in jails.toml, so there is nothing \
                     to reconcile.\n      `jails add <capability>` records one; `sync` then \
                     makes the project match the list."
                );
            }
            jails_engine::route::sync(run)
        }),
        Command::Remove {
            capabilities,
            name,
            force,
            package,
            undeclare,
        } => {
            if model_capability::owns() {
                let result = match undeclare {
                    None => model_capability::remove(
                        capabilities,
                        name,
                        package,
                        invocation,
                    ),
                    Some(Undeclare::Dependency { coordinate }) => {
                        arguments::maven_coordinate(&coordinate).and_then(|coordinate| {
                            model_capability::remove_dependency(coordinate, invocation)
                        })
                    }
                    Some(Undeclare::FastTest) => {
                        model_capability::remove_fast_test(invocation)
                    }
                };
                return dispatch::finish_invocation(result, failure_output, &failure_path);
            }
            match undeclare {
            // `mutate`, not `mutate_confirmed`: the prompt on `remove
            // <capability>` is there because deleting generated files is a
            // decision about bytes the reader may have edited. Retiring a
            // declared resource unsplices exactly what jails spliced and
            // touches nothing else, so there is nothing to authorise.
            Some(Undeclare::Dependency { coordinate }) => arguments::maven_coordinate(&coordinate)
                .map(jails_protocol::entity::DeclaredId::Dependency)
                .and_then(|id| {
                    dispatch::mutate(invocation, false, |run| {
                        jails_engine::route::undeclare(run, id.clone())
                    })
                }),
            Some(Undeclare::FastTest) => {
                dispatch::mutate(invocation, false, jails_engine::route::remove_fast_test)
            }
                None => dispatch::mutate_confirmed(invocation, false, force, |run| {
                let asked =
                    dispatch::declarations(&capabilities, name.as_deref(), package.as_deref())?;
                dispatch::one_transition_each(run, &asked, jails_engine::route::remove)
            }),
            }
        }
        Command::Set { setting, tests } => {
            arguments::split_setting(&setting).and_then(|(key, value)| {
                if model_setting::owns() {
                    return model_setting::set(key, value, tests, invocation);
                }
                dispatch::mutate(invocation, false, |run| {
                    jails_engine::route::set_property(run, key.clone(), value.clone(), tests)
                })
            })
        }
        Command::Unset { key, tests } => {
            if model_setting::owns() {
                model_setting::unset(key, tests, invocation)
            } else {
                arguments::declared_property(&key, tests).and_then(|id| {
                    dispatch::mutate(invocation, false, |run| {
                        jails_engine::route::undeclare(run, id.clone())
                    })
                })
            }
        }
        Command::Rename {
            command,
            old,
            new,
            force,
        } => match command {
            Some(cli::RenameCommand::Resource {
                from,
                to,
                strategy,
                table,
                api,
                route,
                force,
            }) => {
                if model_rename::owns() {
                    return dispatch::finish_invocation(
                        model_rename::run(
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
                        failure_output,
                        &failure_path,
                    );
                }
                dispatch::mutate(invocation, false, |run| {
                jails_engine::route::rename_resource(
                    run,
                    jails_engine::route::RenameResourceInvocation {
                        selector: &from,
                        new: &to,
                        strategy: strategy.into(),
                        target_table: table.as_deref(),
                        api: api.into(),
                        target_route: route.as_deref(),
                        force,
                    },
                )
                })
            }
            Some(cli::RenameCommand::Storage { .. }) if model_command::owns() => {
                model_command::refuse_legacy_mutation(
                    "rename storage",
                    "use a supported single-cutover field/entity policy; multi-release storage campaigns are not canonical yet",
                )
            }
            Some(cli::RenameCommand::Storage {
                resource,
                complete,
                old_version_retired,
                force,
            }) => dispatch::mutate(invocation, false, |run| {
                jails_engine::route::rename_storage(
                    run,
                    &resource,
                    &complete,
                    old_version_retired,
                    force,
                )
            }),
            None if model_command::owns() => model_command::refuse_legacy_mutation(
                "rename OLD NEW",
                "use `jails rename resource <current> <new> --strategy preserve-table|single-cutover`",
            ),
            None => match (old, new) {
                (Some(old), Some(new)) => dispatch::mutate(invocation, false, |run| {
                    jails_engine::route::rename(run, &old, &new, force)
                }),
                _ => Err("legacy rename requires OLD and NEW.\n       fix: use `jails rename resource <slice>.<current-name> <new-name> --strategy preserve-table|single-cutover|rolling`".into()),
            },
        },
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
            if model_destroy::owns() {
                return dispatch::finish_invocation(
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
                    ),
                    failure_output,
                    &failure_path,
                );
            }
            dispatch::mutate_confirmed(invocation, false, force, |run| {
                let storage = arguments::storage_retirement(storage, confirm_table.clone())?;
                let migration_effect = migrate.then_some(datasource.as_deref()).flatten();
                jails_engine::route::destroy(
                    run,
                    kind,
                    &name,
                    package.as_deref(),
                    force,
                    storage,
                    migration_effect,
                )
            })
        }
        Command::Resource { command } => match command {
            ResourceCommand::Status {
                selector,
                datasource,
            } => schema_command::resource_status(
                &selector,
                datasource.as_deref(),
                invocation,
            ),
            ResourceCommand::Revive { selector, table } => {
                if model_destroy::owns() {
                    model_destroy::revive(selector, table, invocation)
                } else {
                    dispatch::mutate(invocation, false, |run| {
                        jails_engine::route::revive(run, &selector, &table)
                    })
                }
            }
            ResourceCommand::Repair { .. } if model_command::owns() => {
                model_command::refuse_legacy_mutation(
                    "resource repair",
                    "run `jails sync`; overlapping or deleted managed files require an explicit reader decision",
                )
            }
            ResourceCommand::Repair {
                selector,
                strategy: _,
                datasource,
            } => dispatch::mutate(invocation, false, |run| {
                jails_engine::route::repair(run, &selector, datasource.as_deref())
            }),
            ResourceCommand::Index { command } => model_index::run(command, invocation),
            ResourceCommand::Field { command } => model_resource::run(command, invocation),
        },
        Command::Start { services } => compose::start(&services, debug),
        Command::Stop { services } => compose::stop_cmd(&services, debug),
        Command::Adopt if model_command::owns() => model_command::refuse_legacy_mutation(
            "adopt",
            "canonical projects already own `.jails/generated`; adopt only before creating the model",
        ),
        Command::Adopt => dispatch::mutate(invocation, false, jails_engine::route::adopt_layout),
        Command::Modernize if model_command::owns() => model_command::refuse_legacy_mutation(
            "modernize",
            "modernize the build before canonical adoption, then recapture the project",
        ),
        Command::Modernize => dispatch::mutate(invocation, false, jails_engine::route::modernize),
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
        Command::Doctor { json } => doctor::doctor(json),
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
        Command::Migrate {
            command,
            check,
            no_start,
        } => match command {
            Some(cli::MigrateCommand::Lint { manifest }) => {
                schema_command::migrate_lint(manifest.as_deref(), invocation)
            }
            None if !check => Err(
                "`--check` is the only mode jails has: it applies the migrations to a \
                     scratch database and drops it. Applying them for real is Flyway's job, \
                     which the application does at startup.\n\nfix: run `jails migrate`."
                    .into(),
            ),
            None => migrate::check(no_start, debug),
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
            // run. V1 spliced it from inside `run::test`, recorded nothing,
            // and left no way to take it back out; `jails remove fast-test`
            // is that way. Idempotent, so every later `--fast` writes nothing.
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
                    true if model_capability::owns() => {
                        model_capability::ensure_fast_test(invocation)
                    }
                    true => dispatch::precondition(
                        invocation,
                        jails_engine::route::install_fast_test,
                    ),
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
        Command::Fmt if model_command::owns() => model_command::refuse_legacy_mutation(
            "fmt",
            "run the project formatter directly; canonical formatter ownership is not modeled yet",
        ),
        Command::Fmt => dispatch::mutate(invocation, false, jails_engine::route::format),
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
/// They used to sit in `commands.rs` and reach for `crate::Cli`, which was one
/// layer above that module — invisible while everything was one crate, and a
/// cycle the moment the tooling became one. `commands` takes the
/// `clap::Command` as an argument now and exposes its two walkers, so the
/// property being tested is unchanged: this is the command that parses the
/// arguments, not a fixture resembling it.
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
