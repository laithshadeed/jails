//! One `match` from a kind to the files it would write, and nothing else.
//!
//! This is `abstract.md` rung 4's query half: it computes and returns, it never
//! writes. `generate` is the modifier on top of it, and `destroy` asks it the
//! same question in reverse — which is what let `KIND_FILES`, 672 lines
//! transcribing these paths by hand, be deleted.
//!
//! It is a `match` on purpose, not a table. `plan.md` §6.2's option F would
//! make each kind a descriptor, and that is the right eventual shape for the
//! kinds that are pure data; the ones that read a record off disk, refuse a
//! precondition or vary structurally are logic, and logic in a descriptor is a
//! conditional no test can reach directly. Until that line is drawn kind by
//! kind, one readable `match` beats a half-general table with escapes in it.

use super::*;

/// The files one `generate` call would write, computed without writing any.
///
/// `abstract.md` rungs 4-5, Separate Query from Modifier. `destroy` used to
/// read `KIND_FILES`: 672 lines transcribing by hand the paths this match
/// right next door already computes. Two transcriptions of one fact drift, and
/// `tests/agreement.rs` existed to police the drift -- which §9 calls a receipt
/// for a decision not made. The decision is made here: there is one list, and
/// `destroy` asks for it.
///
/// **Contents are computed too, and thrown away by `destroy`.** That is the
/// cost, and it is the right trade: a query that returned only paths would be
/// a *second* traversal of the same match, which is the duplication this
/// removes wearing a different hat.
pub(crate) fn artifacts_for(
    project: &Project,
    recipe: &Recipe<'_>,
    package: Option<&str>,
) -> Result<Vec<Artifact>> {
    let root = project.root().to_path_buf();
    let base = project.base().to_string();
    let config = project.layers();
    // `--package` replaces the conventional home for every artifact in this
    // call; without it each kind goes where its convention says.
    let place = |default: &str| project.package_named(default, package);
    let Recipe {
        name,
        fields,
        indexes,
        strategy_on,
        strategy_yields,
        ..
    } = *recipe;

    let artifacts = match recipe.kind {
        ArtifactKind::Scaffold => {
            // A scaffold includes a Spring MVC controller and a JdbcClient
            // adapter. Emitting that shape into a plain Maven project produces
            // Java that cannot compile; refuse while generation is still a
            // pure query, before the prepare/commit path can write anything.
            require_spring_project(project, "scaffold")?;
            scaffold_artifacts(
                &crate::model::Slice::new(project, package),
                name,
                fields,
                indexes,
            )?
        }
        ArtifactKind::Controller => {
            let web = place(layout::WEB);
            // One value, read by the class and by its test. See `web::Endpoint`
            // for why they must not derive it separately.
            // Both referenced types are looked for in the domain layer, the
            // same assumption `transition` and `durable-job` make about
            // `--on`: `g record` is what writes a response type, and that is
            // where it puts it. A type kept elsewhere produces a compile error
            // naming this exact import, which is the failure jails can
            // describe -- guessing by walking the tree would find the wrong
            // `Status` in a project that has three.
            let domain = config.layer(layout::DOMAIN);
            let domain = crate::generate::subpackage(&base, domain);
            let extra: String = [strategy_yields, strategy_on]
                .into_iter()
                .flatten()
                .map(|ty| crate::generate::import_of(&web, &domain, ty))
                .collect();
            let endpoint = crate::generate::web::Endpoint {
                method: recipe.http_method(),
                returns: strategy_yields,
                accepts: strategy_on,
                extra,
            };
            vec![
                Artifact {
                    kind: "controller",
                    path: main_dir(&root, &web).join(format!("{name}Controller.java")),
                    contents: stub_controller(&web, name, &endpoint),
                },
                Artifact {
                    kind: "controller test",
                    path: test_dir(&root, &web).join(format!("{name}ControllerTest.java")),
                    contents: controller_stub_test(
                        &web,
                        name,
                        project.mockmvc_autoconfigure_import(),
                        &endpoint,
                        project.boot_major(),
                    ),
                },
            ]
        }
        ArtifactKind::Service => {
            let service = place(layout::SERVICE);
            vec![
                Artifact {
                    kind: "service",
                    path: main_dir(&root, &service).join(format!("{name}Service.java")),
                    contents: stub_service(&service, name),
                },
                Artifact {
                    kind: "service test",
                    path: test_dir(&root, &service).join(format!("{name}ServiceTest.java")),
                    contents: service_stub_test(&service, name),
                },
            ]
        }
        // The layer-less kind: a plain class and its test, in the base package
        // rather than a subpackage, because "a class" says nothing about which
        // layer owns it. Everything else here has a conventional home; this is
        // the one for ordinary Java -- an algorithm, a ring buffer, a parser.
        ArtifactKind::Class => {
            let pkg = place("");
            vec![
                Artifact {
                    kind: "class",
                    path: main_dir(&root, &pkg).join(format!("{name}.java")),
                    contents: stub_class(&pkg, name),
                },
                Artifact {
                    kind: "class test",
                    path: test_dir(&root, &pkg).join(format!("{name}Test.java")),
                    contents: class_test(&pkg, name),
                },
            ]
        }
        ArtifactKind::Interface => {
            let pkg = place("");
            vec![Artifact {
                kind: "interface",
                path: main_dir(&root, &pkg).join(format!("{name}.java")),
                contents: interface_java(&pkg, name),
            }]
        }
        // Spring-only kinds. The templates live in spring.rs, next to the
        // capabilities that share their Spring Boot 4 assumptions.
        ArtifactKind::Client => {
            require_spring_project(project, "client")?;
            crate::spring::client_files(&crate::model::Slice::new(project, package), name)
        }
        ArtifactKind::Fetcher => {
            require_spring_project(project, "fetcher")?;
            if !fields.is_empty() || strategy_on.is_some() || strategy_yields.is_some() {
                return Err(jails_support::Failure::Told(
                    "fetcher takes only a name; limits and policy are external configuration"
                        .to_string(),
                ));
            }
            crate::spring::fetcher_files(&crate::model::Slice::new(project, package), name)
        }
        ArtifactKind::Job => {
            require_spring_project(project, "job")?;
            crate::spring::job_files(&crate::model::Slice::new(project, package), name)
        }
        ArtifactKind::HttpWorkflow => {
            require_spring_project(project, "http-workflow")?;
            if !fields.is_empty() || strategy_yields.is_some() {
                return Err(
                    jails_support::Failure::Told("http-workflow takes a name and `--on <Fetcher>`; bounds are request/configuration data"
                        .to_string()),
                );
            }
            let fetcher = strategy_on.ok_or_else(|| {
                format!(
                    "http-workflow {name} needs the safe fetcher it composes.\n       fix: pass `--on <Fetcher>`, for example `--on Page`."
                )
            })?;
            crate::spring::http_workflow_files(
                &crate::model::Slice::new(project, package),
                name,
                &strip_redundant_suffix(ArtifactKind::Fetcher, &capitalize(fetcher)),
            )?
        }
        ArtifactKind::Association => {
            require_spring_project(project, "association")?;
            if fields.is_empty() {
                return Err(format!(
                    "association {name} needs at least one `childField=parentField` mapping"
                )
                .into());
            }
            let child = strategy_on.ok_or_else(|| {
                format!(
                    "association {name} needs its child resource.\n       fix: pass `--on <Child>`."
                )
            })?;
            let parent = strategy_yields.ok_or_else(|| {
                format!(
                    "association {name} needs its parent resource.\n       fix: pass `--yields <Parent>`."
                )
            })?;
            crate::spring::association_files(
                &crate::model::Slice::new(project, package),
                name,
                &capitalize(child),
                &capitalize(parent),
                fields,
            )?
        }
        ArtifactKind::Idempotency => {
            require_spring_project(project, "idempotency")?;
            if !fields.is_empty() || strategy_on.is_some() || strategy_yields.is_some() {
                return Err(jails_support::Failure::Told(
                    "idempotency takes only a name; the scope, key and request bytes are \
                     runtime values the caller supplies, not generation-time ones"
                        .to_string(),
                ));
            }
            crate::spring::idempotency_files(&crate::model::Slice::new(project, package), name)?
        }
        ArtifactKind::Auth => {
            require_spring_project(project, "auth")?;
            if !fields.is_empty() || strategy_on.is_some() || strategy_yields.is_some() {
                return Err(jails_support::Failure::Told(
                    "auth takes only a name; the subject and scopes are runtime values the \
                     caller supplies, not generation-time ones"
                        .to_string(),
                ));
            }
            crate::spring::auth_files(&crate::model::Slice::new(project, package), name)?
        }
        ArtifactKind::Webhook => {
            require_spring_project(project, "webhook")?;
            if !fields.is_empty() || strategy_on.is_some() || strategy_yields.is_some() {
                return Err(jails_support::Failure::Told(
                    "webhook takes only a name; the payload is whatever the sender posts, \
                     and binding it before the signature is checked is the bug this kind \
                     exists to avoid"
                        .to_string(),
                ));
            }
            crate::spring::webhook_files(&crate::model::Slice::new(project, package), name)?
        }
        ArtifactKind::Search => {
            require_spring_project(project, "search")?;
            crate::spring::search_files(&crate::model::Slice::new(project, package), name, fields)?
        }
        ArtifactKind::HttpSink => {
            require_spring_project(project, "http-sink")?;
            if !fields.is_empty() {
                return Err(jails_support::Failure::Told(
                    "http-sink payloads come from the typed outbox event; do not repeat fields"
                        .to_string(),
                ));
            }
            let usecase = strategy_on.ok_or_else(|| {
                format!(
                    "http-sink {name} needs its transactional outbox use case.\n       fix: pass `--on <UseCase>`."
                )
            })?;
            let event = strategy_yields.ok_or_else(|| {
                format!(
                    "http-sink {name} needs the typed event it delivers.\n       fix: pass `--yields <Event>`."
                )
            })?;
            crate::spring::http_sink_files(
                &crate::model::Slice::new(project, package),
                name,
                &capitalize(usecase),
                &capitalize(event),
            )?
        }
        ArtifactKind::DurableJob => {
            require_spring_project(project, "durable-job")?;
            let usecase = strategy_on.ok_or_else(|| {
                format!(
                    "durable-job {name} needs the create use case it invokes.\n       fix: pass `--on <UseCase>`, for example `--on ProcessTask`."
                )
            })?;
            let target = strategy_yields.ok_or_else(|| {
                format!(
                    "durable-job {name} needs the resource that proves completion.\n       fix: pass `--yields <Resource>`, for example `--yields Task`."
                )
            })?;
            let slice = crate::model::Slice::new(project, package);
            let parsed = parse_fields(fields)?;
            crate::spring::durable_job_files(
                &slice,
                name,
                &capitalize(usecase),
                &capitalize(target),
                &parsed,
            )?
        }
        ArtifactKind::Usecase => {
            require_spring_project(project, "usecase")?;
            let target = strategy_on.ok_or_else(|| {
                format!(
                    "usecase {name} needs the resource it creates.\n       fix: pass `--on <Resource>`, for example `jails g usecase {name} title:string --on Task`."
                )
            })?;
            // `--package` places the operation itself. The target resource
            // already exists in the project's configured scaffold layers;
            // moving the operation must not make Jails look for a second copy
            // of that resource in the override package. `Slice` owns that rule
            // now, so no call site restates it.
            let slice = crate::model::Slice::new(project, package);
            let parsed = parse_fields(fields)?;
            let mut files =
                crate::spring::usecase_files(&slice, name, &capitalize(target), &parsed)?;
            if let Some(event) = strategy_yields {
                files.extend(crate::spring::outbox_files(
                    &slice,
                    name,
                    &capitalize(target),
                    &capitalize(event),
                    &parsed,
                )?);
            }
            files
        }
        ArtifactKind::Query => {
            require_spring_project(project, "query")?;
            crate::spring::require_jakarta_spring(project, "query", "JdbcClient")?;
            let target = strategy_on.ok_or_else(|| {
                format!(
                    "query {name} needs the resource it reads.\n       fix: pass `--on <Resource>`, for example `jails g query {name} status:TaskStatus --on Task`."
                )
            })?;
            if strategy_yields.is_some() {
                return Err(jails_support::Failure::Told(
                    "`--yields` is not valid for a query; queries return the target resource"
                        .to_string(),
                ));
            }
            let slice = crate::model::Slice::new(project, package);
            let parsed = parse_fields(fields)?;
            crate::spring::query_files(&slice, name, &capitalize(target), &parsed)?
        }
        ArtifactKind::Transition => {
            require_spring_project(project, "transition")?;
            crate::spring::require_jakarta_spring(project, "transition", "JdbcClient")?;
            let target = strategy_on.ok_or_else(|| {
                format!(
                    "transition {name} needs the resource it updates.\n       fix: pass `--on <Resource>`, for example `jails g transition {name} id:uuid tenantId:uuid@scope status:TaskStatus version:long --on Task`."
                )
            })?;
            if strategy_yields.is_some() {
                return Err(
                    jails_support::Failure::Told("`--yields` is not valid for a transition; transitions return the updated target resource"
                        .to_string()),
                );
            }
            let slice = crate::model::Slice::new(project, package);
            let parsed = parse_fields(fields)?;
            crate::spring::transition_files(&slice, name, &capitalize(target), &parsed)?
        }
        ArtifactKind::Event => {
            require_spring_project(project, "event")?;
            let parsed = parse_fields(fields)?;
            crate::spring::event_files(&crate::model::Slice::new(project, package), name, &parsed)?
        }
        ArtifactKind::Dto => {
            let domain = place(layout::DOMAIN);
            let (components, _) = fields_from_spec_or_record(project, &domain, name, fields)?;
            crate::spring::dto_files(
                &crate::model::Slice::new(project, package),
                name,
                &components,
            )
        }
        ArtifactKind::Record => {
            let parsed = parse_fields(fields)?;
            let domain = place(layout::DOMAIN);
            vec![
                Artifact {
                    kind: "record",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: record_java(&domain, name, &parsed),
                },
                Artifact {
                    kind: "record test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: record_test(project, &domain, name, &parsed),
                },
            ]
        }
        // The three one-shots. `plan_recipe` refuses them before this is
        // reached, asking `recipe::is_persistent` rather than listing them --
        // so these arms exist for exhaustiveness and are the *only* thing
        // trusting that guard. They stay `unreachable!` rather than becoming a
        // `_` because a new kind must still be classified here.
        ArtifactKind::Field => {
            unreachable!("a one-shot: refused by `plan_recipe`, which asks `recipe::is_persistent`")
        }
        ArtifactKind::Factory => {
            if !fields.is_empty() {
                return Err(format!(
                    "factory {name} reads the existing record and takes no field spec.\n       \
                     fix: run `jails g factory {name}`."
                )
                .into());
            }
            let domain = subpackage(&base, config.layer(layout::DOMAIN));
            let testkit = place(layout::TESTKIT);
            let components = project.record_in(&domain, name).ok_or_else(|| {
                format!(
                    "no {name} record found under {domain}.\n       \
                     fix: generate the record/scaffold first, then run `jails g factory {name}`."
                )
            })?;
            vec![Artifact {
                kind: "test factory",
                path: test_dir(&root, &testkit).join(format!("{name}Factory.java")),
                contents: factory_java(project, &testkit, &domain, name, &components),
            }]
        }
        ArtifactKind::Value => {
            let parsed = parse_fields(fields)?;
            if parsed.is_empty() {
                return Err(jails_support::Failure::Told("a value type needs at least one field, e.g. `generate value Money amount:long`".to_string()));
            }
            let domain = place(layout::DOMAIN);
            vec![
                Artifact {
                    kind: "value",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: value_java(&domain, name, &parsed),
                },
                Artifact {
                    kind: "value test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: value_test(project, &domain, name, &parsed),
                },
            ]
        }
        ArtifactKind::Enum => {
            let constants = parse_constants(fields)?;
            let domain = place(layout::DOMAIN);
            vec![
                Artifact {
                    kind: "enum",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: enum_java(&domain, name, &constants),
                },
                Artifact {
                    kind: "enum test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: enum_test(&domain, name, &constants),
                },
            ]
        }
        ArtifactKind::Repo => {
            let app = place(layout::APP);
            let adapters = place(layout::ADAPTERS);
            let domain = place(layout::DOMAIN);
            let mut artifacts = Vec::new();
            // One source rule shared with scaffold/dto: explicit fields,
            // otherwise the record on disk, otherwise a refusal that names
            // the fix. A TODO-shaped adapter silently loses data.
            let (record_fields, _) = fields_from_spec_or_record(project, &domain, name, fields)?;
            let columns = crate::sql::columns(&record_fields, project, &domain, &lower_first(name));

            // A repository of a type that does not exist is meaningless, and
            // the port would not compile. Rather than fail, lay down the
            // smallest record that could be one -- it is a starting point the
            // reader will obviously edit, the same way `scaffold` works.
            if !project
                .projected_main_sources()
                .contains_key(&main_dir(&root, &domain).join(format!("{name}.java")))
            {
                let id = if record_fields.is_empty() {
                    parse_fields(&["id:string!".to_string()])?
                } else {
                    record_fields.clone()
                };
                artifacts.push(Artifact {
                    kind: "record (placeholder for the repository)",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: record_java(&domain, name, &id),
                });
                artifacts.push(Artifact {
                    kind: "record test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: record_test(project, &domain, name, &id),
                });
            }

            artifacts.push(Artifact {
                kind: "repository port",
                path: main_dir(&root, &app).join(format!("{name}Repository.java")),
                contents: repository_port(
                    &app,
                    name,
                    &import_of(&app, &domain, name),
                    &repository::key_type(&columns),
                ),
            });
            artifacts.push(Artifact {
                kind: "JDBC adapter",
                path: main_dir(&root, &adapters).join(format!("Jdbc{name}Repository.java")),
                contents: jdbc_repository_for(
                    project,
                    &adapters,
                    name,
                    &format!(
                        "{}{}",
                        import_of(&adapters, &domain, name),
                        import_of(&adapters, &app, &format!("{name}Repository"))
                    ),
                    &columns,
                    &domain,
                ),
            });
            artifacts.push(Artifact {
                kind: "JDBC adapter integration test",
                path: test_dir(&root, &adapters).join(format!("Jdbc{name}RepositoryIT.java")),
                contents: jdbc_repository_test_for(
                    project,
                    &repository::Subject {
                        pkg: &adapters,
                        domain: &domain,
                        repository: &app,
                        name,
                        fields: &record_fields,
                        columns: &columns,
                    },
                ),
            });
            artifacts
        }
        ArtifactKind::Handler => {
            let api = place(layout::API);
            let domain = place(layout::DOMAIN);
            let mut artifacts = Vec::new();

            // Every handler renders failures through the same envelope, so the
            // first one lays it down and the rest reuse it.
            if !main_dir(&root, &domain).join("ApiError.java").exists() {
                let fields = parse_fields(&[
                    "code:string!".to_string(),
                    "message:string!".to_string(),
                    "details:map<string,string>".to_string(),
                ])?;
                artifacts.push(Artifact {
                    kind: "error envelope",
                    path: main_dir(&root, &domain).join("ApiError.java"),
                    contents: value_java(&domain, "ApiError", &fields),
                });
                artifacts.push(Artifact {
                    kind: "error envelope test",
                    path: test_dir(&root, &domain).join("ApiErrorTest.java"),
                    contents: value_test(project, &domain, "ApiError", &fields),
                });
            }

            artifacts.push(Artifact {
                kind: "handler",
                path: main_dir(&root, &api).join(format!("{name}Handler.java")),
                contents: handler_java(&api, name, &import_of(&api, &domain, "ApiError")),
            });
            artifacts.push(Artifact {
                kind: "handler test",
                path: test_dir(&root, &api).join(format!("{name}HandlerTest.java")),
                contents: handler_test(&api, name),
            });
            artifacts
        }
        ArtifactKind::Sealed => {
            let variants = parse_variants(fields)?;
            let domain = place(layout::DOMAIN);
            vec![
                Artifact {
                    kind: "sealed type",
                    path: main_dir(&root, &domain).join(format!("{name}.java")),
                    contents: sealed_java(&domain, name, &variants),
                },
                Artifact {
                    kind: "sealed type test",
                    path: test_dir(&root, &domain).join(format!("{name}Test.java")),
                    contents: sealed_test(&domain, name, &variants),
                },
            ]
        }
        ArtifactKind::Strategy => {
            let variants = parse_variants(fields)?;
            let slice = crate::model::Slice::new(project, package);
            let domain = place(layout::DOMAIN);
            let on = strategy_on.ok_or_else(|| {
                format!(
                    "`generate strategy` needs the type the strategy examines, e.g. \
                     `jails g strategy {name} Coffee Large --on Transaction --yields Reward`.\n\n\
                     Without it jails would have to invent the one method every \
                     implementation overrides, and every implementation would then have \
                     to be rewritten."
                )
            })?;
            let spring = matches!(
                crate::pom::read(&root).map(|p| crate::pom::flavor(&p)),
                Ok(crate::pom::Flavor::SpringBoot)
            );
            // The generated signature names types jails did not write. If one
            // is not in the project yet, say so here rather than letting the
            // next `mvn` be what tells you -- a compile error for a line you
            // did not write is the plumbing this tool exists to remove.
            for missing in missing_types(&root, [Some(on), strategy_yields]) {
                println!(
                    "note: {missing} is not in this project yet -- \
                     `jails g record {missing} <field:type ...>` writes one"
                );
            }
            // Where `--on` and `--yields` already live. They are somebody
            // else's types, so their home is the conventional one whatever
            // `--package` says about this call's own classes.
            let owner = slice.owned(Layer::Domain);
            let signature = |user: &str| {
                let mut imports = crate::generate::import_of(user, &owner, on);
                if let Some(yields) = strategy_yields {
                    imports += &crate::generate::import_of(user, &owner, yields);
                }
                imports
            };
            // A `@Component` in `domain` violates the ArchUnit rule
            // `g scaffold` writes, and the annotation is load-bearing: without
            // it the bean is silently absent from the injected `List<Port>`.
            // Two first-party generators cannot disagree about where the
            // domain boundary is, so the beans live a layer up and the port --
            // which needs no framework at all -- stays where it belongs. On a
            // plain-Maven project there is no annotation and no rule, but the
            // placement stays the same, because one layout is easier to
            // explain than one that depends on the build file.
            let beans = place(layout::SERVICE);
            let mut artifacts = vec![Artifact {
                kind: "strategy",
                path: main_dir(&root, &domain).join(format!("{name}.java")),
                contents: strategy_interface_java(
                    &domain,
                    name,
                    &variants,
                    on,
                    strategy_yields,
                    &signature(&domain),
                ),
            }];
            let mut extra = crate::generate::import_of(&beans, &domain, name);
            extra += &signature(&beans);
            for variant in &variants {
                let class = strategy_class(variant, name);
                artifacts.push(Artifact {
                    kind: "strategy implementation",
                    path: main_dir(&root, &beans).join(format!("{class}.java")),
                    contents: strategy_impl_java(
                        &beans,
                        name,
                        &class,
                        on,
                        strategy_yields,
                        spring,
                        &extra,
                    ),
                });
                artifacts.push(Artifact {
                    kind: "strategy implementation test",
                    path: test_dir(&root, &beans).join(format!("{class}Test.java")),
                    contents: strategy_impl_test(&beans, name, &class, on, strategy_yields),
                });
            }
            artifacts
        }
        ArtifactKind::Command => {
            let cli = project.package(Layer::Cli, package);
            vec![
                Artifact {
                    kind: "command",
                    path: project
                        .main(Layer::Cli, package)
                        .join(format!("{name}Command.java")),
                    contents: command_java(&cli, name),
                },
                Artifact {
                    kind: "command test",
                    path: project
                        .test(Layer::Cli, package)
                        .join(format!("{name}CommandTest.java")),
                    contents: command_test(&cli, name),
                },
            ]
        }
        ArtifactKind::Cli => {
            let cli = project.package(Layer::Cli, package);
            vec![
                Artifact {
                    kind: "cli",
                    path: project
                        .main(Layer::Cli, package)
                        .join(format!("{name}Cli.java")),
                    contents: cli_java(&cli, &format!("{name}Cli"), &name.to_lowercase()),
                },
                Artifact {
                    kind: "cli test",
                    path: project
                        .test(Layer::Cli, package)
                        .join(format!("{name}CliTest.java")),
                    contents: cli_test(&cli, &format!("{name}Cli")),
                },
            ]
        }
        ArtifactKind::Cases => unreachable!("a one-shot: its NAME is a path, not a class"),
        ArtifactKind::Migration => unreachable!("a one-shot: its NAME is a SQL description"),
        ArtifactKind::Test => {
            let pkg = place("");
            vec![Artifact {
                kind: "test",
                path: test_dir(&root, &pkg).join(format!("{name}Test.java")),
                contents: stub_test(&pkg, name),
            }]
        }
        ArtifactKind::IntegrationTest => {
            let pkg = place("");
            vec![Artifact {
                kind: "integration test",
                path: test_dir(&root, &pkg).join(format!("{name}IT.java")),
                contents: integration_test_java(&pkg, name),
            }]
        }
    };

    Ok(artifacts)
}
