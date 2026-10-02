use anyhow::{Context, Result};
use clap::Parser;
use std::fs;

mod cli;
use cli::{Cli, OutputFormat};

mod interpolate;

mod collection;
use collection::{
    RequestFile, load_requests, parse_captures, resolve_custom_ca, time_limit_warnings,
    validate_expect_max_ms, validate_ignore_config,
};

mod executor;
use executor::{RunContext, execute_request};

mod output;
use crate::{
    collection::load_ext_body,
    output::{
        NormalOutput, OutputMode, QuietOutput, RequestOnlyOutput, ResponseOnlyOutput, SilentOutput,
        VerboseOutput,
    },
};

mod ca;

mod auth;

mod capture;

mod time_limit;
use time_limit::TimeLimits;

mod variables;

mod schema;

mod retry;
use retry::{RetryPolicy, RetrySetting};

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.schema {
        print!("{}", schema::SCHEMA);
        return Ok(());
    }

    let file = cli
        .file
        .as_ref()
        .expect("clap requires a collection file unless --schema is used");

    let env_output_mode = match std::env::var("TOAD_OUTPUT").as_deref() {
        Ok("quiet") => OutputFormat::Quiet,
        Ok("silent") => OutputFormat::Silent,
        Ok("verbose") => OutputFormat::Verbose,
        Ok("response-only") => OutputFormat::ResponseOnly,
        Ok("request-only") => OutputFormat::RequestOnly,
        Ok(unknown) => {
            eprintln!("unknown TOAD_OUTPUT value: '{}', using normal", unknown);
            OutputFormat::Normal
        }
        Err(_) => OutputFormat::Normal,
    };

    let output_format = match cli.output {
        Some(fmt) => fmt,
        None => env_output_mode,
    };

    let output: Box<dyn OutputMode> = match output_format {
        OutputFormat::Silent => Box::new(SilentOutput {}),
        OutputFormat::Quiet => Box::new(QuietOutput {}),
        OutputFormat::Verbose => Box::new(VerboseOutput {}),
        OutputFormat::ResponseOnly => Box::new(ResponseOnlyOutput {}),
        OutputFormat::Normal => Box::new(NormalOutput {}),
        OutputFormat::RequestOnly => Box::new(RequestOnlyOutput {}),
    };

    let content =
        fs::read_to_string(file).with_context(|| format!("could not read {}", file.display()))?;

    let mut rf: RequestFile = RequestFile::parse(&content)
        .with_context(|| format!("could not parse {}", file.display()))?;

    load_ext_body(&mut rf, file)?;
    parse_captures(&mut rf)?;
    validate_ignore_config(&rf)?;
    variables::validate_names(&rf)?;
    validate_expect_max_ms(&rf)?;
    resolve_custom_ca(&mut rf.config, file);

    let ca_password = cli
        .use_custom_ca_password
        .clone()
        .or_else(|| std::env::var("TOAD_CA_PASSWORD").ok());

    let requests = load_requests(&rf, cli.requests.as_deref())?;

    if cli.list_requests {
        for (name, _) in &requests {
            println!("\t{}", name);
        }
        return Ok(());
    }

    let time_limits = TimeLimits::resolve(
        cli.time_scale,
        std::env::var("TOAD_TIME_SCALE").ok().as_deref(),
    );
    for warning in time_limit_warnings(&requests, &rf.config, &time_limits) {
        eprintln!("{}", warning);
    }

    let retry_setting =
        RetrySetting::resolve(cli.retry, std::env::var("TOAD_RETRY").ok().as_deref());

    // Maybe merge vars from a profile
    if let Some(profile_name) = &cli.profile {
        if let Some(profile_vars) = rf.profiles.get(profile_name) {
            rf.vars.extend(profile_vars.clone());
        } else {
            eprintln!("unknown profile '{}'", profile_name);
            std::process::exit(1);
        }
    }

    let ctx = RunContext {
        ca_password: ca_password.as_deref(),
        time_limits,
        output: output.as_ref(),
    };

    // Report every undefined variable and unset environment variable before sending anything
    let resolved = variables::resolve_vars(rf.vars.clone(), &interpolate::system_env);
    let problems =
        variables::check_requests(&requests, &rf.config, &resolved, &interpolate::system_env);
    if !problems.is_empty() {
        for (name, problem) in &problems {
            output.request_error(name, problem);
        }
        std::process::exit(1);
    }

    // Captured values are added to this as requests run
    let mut vars = resolved.vars;

    for (name, req) in &requests {
        // --use-custom-ca applies even when the request ignores the config's use_custom_ca
        let mut config = rf.config.without(&req.ignore_config);
        if let Some(ca_path) = &cli.use_custom_ca {
            config.use_custom_ca = Some(ca_path.to_string_lossy().to_string());
        }
        let retry = RetryPolicy::for_request(req, &config, retry_setting);

        let result = execute_request(name, req, &vars, config, &retry, &ctx);
        match result {
            Ok(captured) => vars.extend(captured),
            Err(e) => {
                output.request_error(name, format!("{:?}", e).as_str());
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
