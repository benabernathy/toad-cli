use anyhow::{Context, Result};
use clap::Parser;
use std::fs;

mod cli;
use cli::{Cli, OutputFormat};

mod interpolate;

mod collection;
use collection::{RequestFile, load_requests, resolve_custom_ca};

mod executor;
use executor::execute_request;

mod output;
use crate::{
    collection::load_ext_body,
    output::{
        NormalOutput, OutputMode, QuietOutput, RequestOnlyOutput, ResponseOnlyOutput, SilentOutput,
        VerboseOutput,
    },
};

mod serve;

mod ca;

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(port) = cli.listen {
        return serve::run(port, cli.output_file);
    }

    let file = cli
        .file
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("a collection file is required unless --listen is used"))?;

    let env_output_mode = match std::env::var("TOAD_OUTPUT").as_deref() {
        Ok("quiet") => OutputFormat::Quiet,
        Ok("silent") => OutputFormat::Silent,
        Ok("verbose") => OutputFormat::Verbose,
        Ok("response-Only") => OutputFormat::ResponseOnly,
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

    let mut rf: RequestFile =
        toml::from_str(&content).with_context(|| format!("could not parse {}", file.display()))?;

    load_ext_body(&mut rf, file)?;
    resolve_custom_ca(&mut rf.config, file);

    if let Some(ca_path) = &cli.use_custom_ca {
        rf.config.use_custom_ca = Some(ca_path.to_string_lossy().to_string());
    }

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

    // Maybe merge vars from a profile
    if let Some(profile_name) = &cli.profile {
        if let Some(profile_vars) = rf.profiles.get(profile_name) {
            rf.vars.extend(profile_vars.clone());
        } else {
            eprintln!("unknown profile '{}'", profile_name);
            std::process::exit(1);
        }
    }

    for (name, req) in &requests {
        let result = execute_request(
            name,
            req,
            &rf.vars,
            rf.config.clone(),
            ca_password.as_deref(),
            output.as_ref(),
        );
        if let Err(e) = result {
            output.request_error(name, format!("{:?}", e).as_str());
            std::process::exit(1);
        }
    }

    Ok(())
}
