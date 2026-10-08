use anyhow::{Result, anyhow};
use clap::Parser;
use std::time::Instant;

mod cli;
use cli::{Cli, OutputFormat};

mod interpolate;

mod collection_file;

mod collection;
use collection::{Collection, time_limit_warnings};

mod executor;
use executor::{RunContext, execute_request};

mod output;
use output::{
    JsonOutput, NormalOutput, OutputMode, QuietOutput, RequestOnlyOutput, ResponseOnlyOutput,
    SilentOutput, Summary, VerboseOutput,
};

mod ca;

mod auth;

mod capture;

mod assertion;

mod time_limit;
use time_limit::TimeLimits;

mod variables;

mod schema;

mod step;
use step::Stepper;

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
        Ok("json") => OutputFormat::Json,
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
        OutputFormat::Json => Box::new(JsonOutput {}),
    };

    let mut collection = Collection::load(file)?;
    variables::check_cli_vars(&collection, &cli.vars)?;
    if let Some(ca_path) = &cli.use_custom_ca {
        collection.set_custom_ca(ca_path);
    }

    let ca_password = cli
        .use_custom_ca_password
        .clone()
        .or_else(|| std::env::var("TOAD_CA_PASSWORD").ok());

    step::check_breakpoints(&collection, &cli.breakpoints)?;

    let requests = collection.requests_to_run(cli.requests.as_deref())?;

    if cli.list_requests {
        for req in &requests {
            println!("\t{}", req.name);
        }
        return Ok(());
    }

    let mut stepper = Stepper::new(cli.step, &cli.breakpoints);
    if stepper.is_some() {
        step::check_terminal(cli.step)?;
    }

    let time_limits = TimeLimits::resolve(
        cli.time_scale,
        std::env::var("TOAD_TIME_SCALE").ok().as_deref(),
    );
    for warning in time_limit_warnings(&requests, &time_limits) {
        eprintln!("{}", warning);
    }

    let retry_setting =
        RetrySetting::resolve(cli.retry, std::env::var("TOAD_RETRY").ok().as_deref());

    let Some(file_vars) = collection.vars_with_profile(cli.profile.as_deref()) else {
        eprintln!(
            "unknown profile '{}'",
            cli.profile.as_deref().unwrap_or_default()
        );
        std::process::exit(1);
    };

    let ctx = RunContext {
        ca_password: ca_password.as_deref(),
        time_limits,
        output: output.as_ref(),
        cli_vars: &cli.vars,
    };

    let started = Instant::now();
    let names: Vec<&str> = requests.iter().map(|req| req.name.as_str()).collect();
    output.run_start(&names);

    // Report every undefined variable and unset environment variable before sending anything
    let mut resolved = variables::resolve_vars(file_vars, &interpolate::system_env);
    // --var values replace the file's values and are used as given
    for (name, value) in &cli.vars {
        resolved.missing_env.remove(name);
        resolved.vars.insert(name.clone(), value.clone());
    }
    let problems = variables::check_requests(&requests, &resolved, &interpolate::system_env);
    if !problems.is_empty() {
        for (name, problem) in &problems {
            output.request_error(name, &anyhow!("{problem}"));
        }
        output.run_finished(&Summary {
            passed: 0,
            failed: 0,
            not_run: requests.len(),
            elapsed: started.elapsed(),
        });
        std::process::exit(1);
    }

    // Captured values are added to this as requests run
    let mut vars = resolved.vars;

    for (i, req) in requests.iter().enumerate() {
        let name = &req.name;
        if let Some(stepper) = &mut stepper
            && stepper.should_stop(name)
            && !stepper.press(step::ask(stepper, name)?)
        {
            output.run_finished(&Summary {
                passed: i,
                failed: 0,
                not_run: requests.len() - i,
                elapsed: started.elapsed(),
            });
            eprintln!(
                "stopped before '{name}' ({i} of {} requests run)",
                requests.len()
            );
            std::process::exit(130);
        }

        let retry = RetryPolicy::for_request(req, retry_setting);
        let result = execute_request(req, &vars, &retry, &ctx);
        match result {
            // A --var value also replaces a captured value
            Ok(captured) => vars.extend(
                captured
                    .into_iter()
                    .filter(|(name, _)| !cli.vars.iter().any(|(n, _)| n == name)),
            ),
            Err(e) => {
                output.request_error(name, &e);
                output.run_finished(&Summary {
                    passed: i,
                    failed: 1,
                    not_run: requests.len() - i - 1,
                    elapsed: started.elapsed(),
                });
                std::process::exit(1);
            }
        }
    }

    output.run_finished(&Summary {
        passed: requests.len(),
        failed: 0,
        not_run: 0,
        elapsed: started.elapsed(),
    });

    Ok(())
}
