//! `remove` and `sync`: taking a capability back out, and putting one back.
//!
//! The inverse of `add`, and the half with the sharper failure mode.
//!
//! **A marked block is where people tune the capability**, because it has the
//! capability's name on it. `unowned_properties` diffs the block against what
//! jails would have written, so `remove` can name the lines it did not write
//! before deleting them -- a real project had about twenty hand-written Kafka
//! properties inside jails' own markers.
//!
//! **`remove` must also take the capability out of `[project] capabilities`.**
//! Left listed, the next `sync` restores exactly what was just removed, which
//! is the failure that makes a manifest worse than none.
//!
//! `sync` is the other direction: make the project match the list. It is why
//! the list has to be true, and why `add` records every capability it applies
//! -- including on the "already set up" path, since a capability installed
//! before the manifest existed is still part of the project.

use super::*;

/// Inverse of [`add`]: unsplice the same pom entries, delete the same files,
/// take compose services out, and stop their containers.
pub fn remove(
    capability: Capability,
    name: Option<&str>,
    dry_run: bool,
    force: bool,
    package: Option<&str>,
    debug: bool,
) -> Result<()> {
    let project = Project::discover()?;
    let root = project.root().to_path_buf();
    let pom_text = project.pom().to_string();
    let flavor = project.flavor();
    let plan = build_plan(capability, &project, name, package)?;

    let mut updated_pom = pom_text.clone();
    let mut removed_deps: Vec<&Dependency> = Vec::new();
    for dep in plan.deps.iter().chain(plan.legacy_deps.iter()) {
        if let Some(next) = pom::remove_dependency(&updated_pom, dep.group_id, dep.artifact_id)? {
            updated_pom = next;
            removed_deps.push(dep);
        }
    }

    let mut removed_plugins: Vec<&str> = Vec::new();
    for (artifact_id, _) in &plan.plugins {
        if let Some(next) = pom::remove_plugin(&updated_pom, artifact_id)? {
            updated_pom = next;
            removed_plugins.push(artifact_id);
        }
    }

    let existing_files: Vec<&PathBuf> = plan
        .files
        .iter()
        .map(|f| &f.path)
        .filter(|p| p.exists())
        .collect();

    let mut compose_text = compose::read(&root)?;
    let mut compose_removed: Vec<&ComposeService> = Vec::new();
    for svc in &plan.compose {
        if let Some(next) = compose::remove_service(&compose_text, svc) {
            compose_text = next;
            compose_removed.push(svc);
        }
    }

    let mut docker_compose_dep = false;
    if flavor == Flavor::SpringBoot
        && !compose::has_services(&compose_text)
        && let Some(next) = pom::remove_dependency(
            &updated_pom,
            crate::pom::SPRING_DOCKER_COMPOSE.group_id,
            crate::pom::SPRING_DOCKER_COMPOSE.artifact_id,
        )?
    {
        updated_pom = next;
        docker_compose_dep = true;
    }
    if flavor == Flavor::SpringBoot
        && plan.spring_test_import.is_some()
        && let Some(next) = pom::remove_dependency(
            &updated_pom,
            SPRING_TESTCONTAINERS.group_id,
            SPRING_TESTCONTAINERS.artifact_id,
        )?
    {
        updated_pom = next;
        removed_deps.push(&SPRING_TESTCONTAINERS);
    }

    let pom_changed = !removed_deps.is_empty() || !removed_plugins.is_empty() || docker_compose_dep;
    let factories_present = plan.spring_test_import.as_ref().is_some_and(|cfg| {
        fs::read_to_string(spring_factories_path(&root)).is_ok_and(|s| s.contains(&cfg.fqcn()))
    });
    let properties_present = plan.spring_test_import.is_some()
        && fs::read_to_string(application_properties_path(&root))
            .is_ok_and(|s| s.contains(EXCEPTION_TRANSLATION_PROPERTY));
    let tests_to_unwire: Vec<PathBuf> = plan
        .spring_test_import
        .as_ref()
        .map(|cfg| {
            find_spring_boot_tests(&root.join("src/test/java"))
                .into_iter()
                .filter(|p| {
                    fs::read_to_string(p).is_ok_and(|s| {
                        s.contains(&jails_java::annotate::import_annotation(cfg.class))
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if !pom_changed
        && existing_files.is_empty()
        && compose_removed.is_empty()
        && tests_to_unwire.is_empty()
        && !factories_present
        && !properties_present
    {
        println!("{} is not set up -- nothing to do", capability.label());
        return Ok(());
    }

    if dry_run {
        for dep in &removed_deps {
            println!(
                "  would remove dependency  {}:{}",
                dep.group_id, dep.artifact_id
            );
        }
        if docker_compose_dep {
            println!(
                "  would remove dependency  {}:{}",
                crate::pom::SPRING_DOCKER_COMPOSE.group_id,
                crate::pom::SPRING_DOCKER_COMPOSE.artifact_id
            );
        }
        for artifact_id in &removed_plugins {
            println!("  would remove plugin  {artifact_id}");
        }
        for path in &existing_files {
            println!("  would delete  {}", rel(&root, path));
        }
        report_edited_files(&root, &plan);
        for svc in &compose_removed {
            println!("  would remove compose service  {}", svc.name);
        }
        for path in &tests_to_unwire {
            println!("  would unsplice @Import from {}", rel(&root, path));
        }
        report_unowned_properties(&root, capability.label(), &plan.properties);
        if factories_present {
            println!(
                "  would unsplice {}",
                rel(&root, &spring_factories_path(&root))
            );
        }
        if properties_present {
            println!(
                "  would unsplice {}",
                rel(&root, &application_properties_path(&root))
            );
        }
        return Ok(());
    }

    if !force {
        println!("about to remove {}:", capability.label());
        for dep in &removed_deps {
            println!("  dep {}:{}", dep.group_id, dep.artifact_id);
        }
        if docker_compose_dep {
            println!(
                "  dep {}:{}",
                crate::pom::SPRING_DOCKER_COMPOSE.group_id,
                crate::pom::SPRING_DOCKER_COMPOSE.artifact_id
            );
        }
        for artifact_id in &removed_plugins {
            println!("  plugin {artifact_id}");
        }
        for path in &existing_files {
            println!("  {}", rel(&root, path));
        }
        report_edited_files(&root, &plan);
        for svc in &compose_removed {
            println!("  compose {}", svc.name);
        }
        for path in &tests_to_unwire {
            println!("  import in {}", rel(&root, path));
        }
        report_unowned_properties(&root, capability.label(), &plan.properties);
        if factories_present {
            println!("  {}", rel(&root, &spring_factories_path(&root)));
        }
        if properties_present {
            println!("  {}", rel(&root, &application_properties_path(&root)));
        }
        print!("proceed? [y/N] ");
        std::io::stdout().flush().ok();
        let mut answer = String::new();
        std::io::stdin()
            .read_line(&mut answer)
            .map_err(|e| format!("failed to read confirmation: {e}"))?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("aborted");
            return Ok(());
        }
    } else {
        // `--force` skips the prompt, which is the whole silent path: without
        // this, a scripted `remove --force` deletes a hand-finished class and
        // says only "removed csv".
        report_edited_files(&root, &plan);
    }

    if pom_changed {
        crate::apply::put_named(root.join("pom.xml"), &updated_pom, "pom.xml")?;
        for dep in &removed_deps {
            println!("  remove  {}:{}", dep.group_id, dep.artifact_id);
        }
        if docker_compose_dep {
            println!(
                "  remove  {}:{}",
                crate::pom::SPRING_DOCKER_COMPOSE.group_id,
                crate::pom::SPRING_DOCKER_COMPOSE.artifact_id
            );
        }
        for artifact_id in &removed_plugins {
            println!("  remove  plugin {artifact_id}");
        }
    }

    for path in existing_files {
        jails_support::apply::remove(path)
            .map_err(|e| format!("failed to remove {}: {e}", path.display()))?;
        println!("  delete  {}", rel(&root, path));
        delete_maven_output(&root, path);
    }

    if !compose_removed.is_empty() {
        let names: Vec<&str> = compose_removed.iter().map(|s| s.name).collect();
        compose::stop(&root, &names, debug);
        compose::write(&root, &compose_text)?;
        if compose_text.is_empty() {
            println!("  delete  {}", rel(&root, &compose::path(&root)));
        } else {
            println!("  compose {}", rel(&root, &compose::path(&root)));
        }
        for svc in &compose_removed {
            println!("  stop    {}", svc.name);
        }
    }

    if let Some(cfg) = &plan.spring_test_import {
        uninstall_postgres_test_initializer(&root, cfg)?;
        delete_maven_output(&root, &spring_factories_path(&root));
        uninstall_db_properties(&root)?;
        delete_maven_output(&root, &application_properties_path(&root));
        let _ = strip_legacy_postgres_imports(&root, cfg)?;
    }
    if !plan.properties.is_empty() {
        remove_capability_properties(&root, capability.label())?;
        delete_maven_output(&root, &application_properties_path(&root));
    }

    // The exact inverse of the record in `add`: left listed, the next `sync`
    // would put back what was just removed.
    crate::config::forget_capability(&root, capability.label())?;
    println!("removed {}", capability.label());
    Ok(())
}

/// Make the project match what `jails.toml` says it is made of.
///
/// The manifest is the point: `add` records every capability it applies, so
/// the file is a true description of the project rather than one somebody has
/// to remember to update. `sync` reads it back and applies whatever is
/// missing.
///
/// What that buys, in the order it matters:
///
/// - A fresh clone becomes the project it claims to be in one command,
///   instead of whoever set it up recalling which `jails add` calls they ran.
/// - A project regenerates against a newer jails. The rewards audit ends with
///   exactly this problem -- a project still carrying hand-written files that
///   jails now produces, with no way to take the improvements but to redo the
///   commands.
/// - `--pretend` answers "what is this project missing?" without writing.
///
/// Every capability is idempotent and reports what is already there, so a
/// `sync` over a project that is already correct changes nothing and says so.
pub fn sync(dry_run: bool, debug: bool, no_start: bool) -> Result<()> {
    use clap::ValueEnum;

    let project = Project::discover()?;
    let labels = project.capabilities();

    if labels.is_empty() {
        println!(
            "{} declares no capabilities, so there is nothing to sync.\n\n\
             `jails add <capability>` records what it applies, so the file\n\
             describes the project from then on. To adopt a project that was\n\
             built before the manifest existed, re-run the `add` calls it had:\n\
             each one reports what is already there and changes nothing else.",
            crate::config::FILE
        );
        return Ok(());
    }

    // Parsing is validated at load, so an unknown label cannot reach here --
    // but resolving every one before applying any keeps `sync` consistent
    // with `add A B`, which preflights for the same reason.
    let mut capabilities = Vec::with_capacity(labels.len());
    for label in labels {
        let capability = Capability::value_variants()
            .iter()
            .find(|c| c.label() == label)
            .copied()
            .ok_or_else(|| format!("{}: unknown capability `{label}`", crate::config::FILE))?;
        capabilities.push(capability);
    }
    preflight(&capabilities, None, None)?;

    println!(
        "{} declares {}: {}\n",
        crate::config::FILE,
        match capabilities.len() {
            1 => "1 capability".to_string(),
            n => format!("{n} capabilities"),
        },
        labels.join(", ")
    );
    for capability in capabilities {
        add(capability, None, dry_run, None, debug, no_start)?;
    }
    Ok(())
}
