//! Transparent adapters for curl, database clients, JShell, and Compose logs.

use crate::cli::{DatabaseClientArg, Output, WebModeArg};
use crate::{console, inspect, project};
use jails_support::Result;
use jails_support::process::{CommandSpec, Diagnostics, OutputMode};
use std::collections::{BTreeMap, BTreeSet};
use std::io::IsTerminal;
use std::path::Path;

pub(crate) struct HttpRequest {
    pub method: String,
    pub target: String,
    pub profile: Option<String>,
    pub base_url: Option<String>,
    pub params: Vec<String>,
    pub query: Vec<String>,
    pub headers: Vec<String>,
    pub header_env: Vec<String>,
    pub json: Option<String>,
    pub data: Option<String>,
    pub timeout: Option<String>,
    pub follow: bool,
    pub print: bool,
}

pub(crate) fn request(request: HttpRequest, invocation: crate::Invocation) -> Result<()> {
    if invocation.output.is_json() && !request.print {
        return Err(
            "transparent curl execution cannot use JSON output.\n       fix: omit `--output json`, or combine it with `--print` for structured preflight only."
                .into(),
        );
    }
    let project = project::Project::discover()?;
    let method = request.method.to_ascii_uppercase();
    if !matches!(
        method.as_str(),
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
    ) {
        return Err(format!(
            "unsupported HTTP method `{}`.\n       fix: use GET, POST, PUT, PATCH, DELETE, HEAD, or OPTIONS.",
            request.method
        )
        .into());
    }
    let routes = inspect::collect_routes(project.root());
    let route_path = if request.target.starts_with('/') {
        if request.target.contains("://") {
            return Err(
                "request targets are origin-relative and cannot contain an authority.\n       fix: pass the origin with `--base-url` and a target beginning `/`."
                    .into(),
            );
        }
        request.target.clone()
    } else {
        let matches = routes
            .iter()
            .filter(|route| {
                route.verb == method
                    && (route.handler == request.target
                        || format!("route:{}:{}:{}", route.verb, route.path, route.handler)
                            == request.target)
            })
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [route] => route.path.clone(),
            [] => {
                return Err(format!(
                    "no {method} route matches `{}`.\n       fix: run `jails routes` or pass an origin-relative `/path`.",
                    request.target
                )
                .into());
            }
            _ => {
                return Err(format!(
                    "route target `{}` is ambiguous.\n       fix: pass its stable `route:<METHOD>:<path>:<handler>` identity.",
                    request.target
                )
                .into());
            }
        }
    };
    let params = pairs(&request.params, "path parameter")?;
    let path = substitute_path(&route_path, &params)?;
    let base = match (&request.base_url, &request.profile) {
        (Some(base), None) => validate_origin(base)?,
        (Some(_), Some(_)) => {
            return Err(
                "`--base-url` conflicts with `--profile`.\n       fix: select exactly one origin source."
                    .into(),
            );
        }
        (None, Some(profile)) => {
            return Err(format!(
                "HTTP profile `{profile}` is not declared in the loaded application manifest.\n       fix: declare `[tools.http.profiles.{profile}]` or pass `--base-url`."
            )
            .into());
        }
        (None, None) => {
            return Err(
                "request has no base origin.\n       fix: pass `--base-url http://127.0.0.1:8080` or select a declared profile."
                    .into(),
            );
        }
    };
    let url = format!("{base}{path}");
    let mut public_args = vec![
        "--silent".to_string(),
        "--show-error".to_string(),
        "--fail-with-body".to_string(),
        "--request".to_string(),
        method,
        "--url".to_string(),
        url,
    ];
    for (name, value) in pairs(&request.query, "query pair")? {
        public_args.extend(["--data-urlencode".into(), format!("{name}={value}")]);
    }
    if let Some(timeout) = &request.timeout {
        validate_duration(timeout)?;
        public_args.extend(["--max-time".into(), timeout.clone()]);
    }
    if request.follow {
        public_args.push("--location".into());
    }
    if let Some(body) = request.json.as_ref().or(request.data.as_ref()) {
        validate_body(body, project.root())?;
        public_args.extend([
            if request.json.is_some() {
                "--json"
            } else {
                "--data"
            }
            .into(),
            body.clone(),
        ]);
    }

    let mut secret_headers = Vec::new();
    secret_headers.extend(request.headers.iter().cloned());
    for (name, environment) in pairs(&request.header_env, "header environment")? {
        let value = std::env::var(&environment).map_err(|_| {
            format!(
                "header environment `{environment}` is unavailable.\n       fix: export it before running the request."
            )
        })?;
        secret_headers.push(format!("{name}: {value}"));
    }
    let scratch = tempfile::Builder::new()
        .prefix("jails-curl-")
        .tempdir()
        .map_err(|error| format!("could not reserve private curl configuration: {error}"))?;
    let mut actual_args = public_args.clone();
    if !secret_headers.is_empty() {
        let config = scratch.path().join("headers.conf");
        let mut body = String::new();
        for header in &secret_headers {
            if header.contains(['\r', '\n']) {
                return Err(
                    "HTTP headers may not contain newlines.\n       fix: pass one `name=value` header per option."
                        .into(),
                );
            }
            body.push_str(&format!("header = \"{}\"\n", curl_config(header)));
        }
        jails_support::apply::put_in_scratch(&config, body.as_bytes())?;
        actual_args.extend(["--config".into(), config.to_string_lossy().into_owned()]);
        public_args.extend(["--config".into(), "<redacted:headers>".into()]);
    }
    let rendered = render_argv("curl", &public_args);
    if request.print || invocation.pretend {
        if invocation.output.is_json() {
            println!(
                "{{\"schema\":\"jails.tool-invocation.v1\",\"tool\":\"curl\",\"argv\":{}}}",
                jails_support::json::string(&rendered)
            );
        } else {
            println!("{rendered}");
        }
        return Ok(());
    }
    if invocation.debug {
        eprintln!("+ {rendered}");
    }
    let done = jails_support::process::run(
        &CommandSpec::new("curl")
            .args(&actual_args)
            .current_dir(project.root())
            .output(OutputMode::Inherit),
        Diagnostics::Normal,
    )?;
    if done.status.success() {
        Ok(())
    } else {
        Err(jails_support::Failure::Reported)
    }
}

pub(crate) fn db_console(
    database: Option<&str>,
    profile: Option<&str>,
    client: DatabaseClientArg,
    single_connection: bool,
    invocation: crate::Invocation,
) -> Result<()> {
    transparent_output(invocation.output, "database console")?;
    if database.is_some_and(|name| name != "postgres") || profile.is_some() {
        return Err(
            "the selected database/profile is not a declared datasource.\n       fix: omit it to use the committed `postgres` datasource."
                .into(),
        );
    }
    console::postgres_console(
        match client {
            DatabaseClientArg::Pgcli => "pgcli",
            DatabaseClientArg::Psql => "psql",
        },
        single_connection,
        invocation.debug,
    )
}

pub(crate) fn runner(
    file: &Path,
    profiles: &[String],
    main: Option<&str>,
    web: WebModeArg,
    compile: bool,
    yes: bool,
    invocation: crate::Invocation,
) -> Result<()> {
    transparent_output(invocation.output, "runner")?;
    console::runner(
        file,
        profiles,
        main,
        web_mode(web),
        compile,
        yes,
        invocation.debug,
    )
}

pub(crate) fn console(
    profiles: &[String],
    main: Option<&str>,
    web: WebModeArg,
    compile: bool,
    yes: bool,
    args: &[String],
    invocation: crate::Invocation,
) -> Result<()> {
    transparent_output(invocation.output, "console")?;
    console::spring_console(
        profiles,
        main,
        web_mode(web),
        compile,
        yes,
        args,
        invocation.debug,
    )
}

fn web_mode(mode: WebModeArg) -> console::WebMode {
    match mode {
        WebModeArg::None => console::WebMode::None,
        WebModeArg::Random => console::WebMode::Random,
        WebModeArg::Configured => console::WebMode::Configured,
    }
}

pub(crate) fn logs(
    services: &[String],
    follow: bool,
    since: Option<&str>,
    tail: usize,
    invocation: crate::Invocation,
) -> Result<()> {
    transparent_output(invocation.output, "logs")?;
    if follow && !std::io::stdout().is_terminal() {
        return Err("`logs --follow` requires a controlling terminal.\n       fix: omit `--follow` for bounded output.".into());
    }
    if tail == 0 {
        return Err("log tail must be positive.\n       fix: pass `--tail 1` or greater.".into());
    }
    if let Some(since) = since {
        validate_duration(since)?;
    }
    let project = project::Project::discover()?;
    let yaml = jails_project::compose::read(project.root())?;
    let declared = declared_services(&yaml);
    let selected = if services.is_empty() {
        declared.iter().cloned().collect::<Vec<_>>()
    } else {
        services.to_vec()
    };
    if let Some(unknown) = selected.iter().find(|service| !declared.contains(*service)) {
        return Err(format!(
            "Compose service `{unknown}` is not declared.\n       fix: choose one of: {}.",
            declared.iter().cloned().collect::<Vec<_>>().join(", ")
        )
        .into());
    }
    let (program, prefix) = jails_support::process::compose_program().ok_or_else(|| {
        "no Compose implementation is available.\n       fix: install Docker Compose or podman-compose."
            .to_string()
    })?;
    let mut args = prefix
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    args.extend(["logs".into(), "--tail".into(), tail.to_string()]);
    if follow {
        args.push("--follow".into());
    }
    if let Some(since) = since {
        args.extend(["--since".into(), since.into()]);
    }
    args.extend(selected);
    let done = jails_support::process::run(
        &CommandSpec::new(program)
            .args(&args)
            .current_dir(project.root())
            .output(OutputMode::Inherit),
        Diagnostics::from_flag(invocation.debug),
    )?;
    if done.status.success() {
        Ok(())
    } else {
        Err(jails_support::Failure::Reported)
    }
}

fn pairs(values: &[String], label: &str) -> Result<BTreeMap<String, String>> {
    let mut parsed = BTreeMap::new();
    for value in values {
        let (name, value) = value
            .split_once('=')
            .ok_or_else(|| format!("invalid {label} `{value}`.\n       fix: use `name=value`."))?;
        if name.is_empty() || parsed.insert(name.into(), value.into()).is_some() {
            return Err(format!(
                "duplicate or empty {label} `{name}`.\n       fix: name each value exactly once."
            )
            .into());
        }
    }
    Ok(parsed)
}

fn substitute_path(path: &str, params: &BTreeMap<String, String>) -> Result<String> {
    let mut output = path.to_string();
    let mut used = BTreeSet::new();
    while let Some(start) = output.find('{') {
        let end = output[start + 1..].find('}').map(|at| at + start + 1).ok_or_else(|| "route contains an unmatched path parameter.\n       fix: repair the controller mapping.".to_string())?;
        let name = &output[start + 1..end];
        let value = params.get(name).ok_or_else(|| format!("route requires path parameter `{name}`.\n       fix: pass `--param {name}=<value>`."))?;
        used.insert(name.to_string());
        output.replace_range(start..=end, &percent_encode(value));
    }
    if let Some(extra) = params.keys().find(|name| !used.contains(*name)) {
        return Err(format!("extra path parameter `{extra}`.\n       fix: remove it or select a route that declares it.").into());
    }
    Ok(output)
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn validate_origin(origin: &str) -> Result<String> {
    if !(origin.starts_with("http://") || origin.starts_with("https://"))
        || origin[origin.find("://").unwrap_or(0) + 3..].contains('/')
    {
        return Err("base URL must be an HTTP(S) origin with no path.\n       fix: use a value such as `http://127.0.0.1:8080`.".into());
    }
    Ok(origin.trim_end_matches('/').to_string())
}

fn validate_body(body: &str, project_root: &Path) -> Result<()> {
    let path = body.strip_prefix('@').ok_or_else(|| "request bodies must be `@<project-relative-path>` or `@-`.\n       fix: put inline data in a file or pipe it on stdin.".to_string())?;
    if path == "-" {
        return Ok(());
    }
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        || !project_root.join(path).is_file()
    {
        return Err("request body path must name a project-relative regular file.\n       fix: pass `@requests/body.json` or `@-`.".into());
    }
    Ok(())
}

fn validate_duration(value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'.' | b'm' | b's' | b'h'))
    {
        return Err(format!(
            "invalid duration `{value}`.\n       fix: use a value such as `10s` or `2m`."
        )
        .into());
    }
    Ok(())
}

fn declared_services(yaml: &str) -> BTreeSet<String> {
    yaml.lines()
        .filter_map(|line| {
            let indent = line.len() - line.trim_start().len();
            let name = line.trim().strip_suffix(':')?;
            (indent == 2
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
            .then(|| name.to_string())
        })
        .collect()
}

fn curl_config(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn render_argv(program: &str, args: &[String]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().map(|arg| {
            if arg
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-._/:=@<>".contains(&byte))
            {
                arg.clone()
            } else {
                format!("'{}'", arg.replace('\'', "'\\''"))
            }
        }))
        .collect::<Vec<_>>()
        .join(" ")
}

fn transparent_output(output: Output, command: &str) -> Result<()> {
    if output.is_json() {
        return Err(format!("{command} is a transparent terminal session and cannot use JSON output.\n       fix: omit `--output json`.").into());
    }
    Ok(())
}
