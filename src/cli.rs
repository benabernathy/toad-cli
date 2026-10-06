use clap::{Parser, ValueEnum};
use std::path::PathBuf;

use crate::retry::RetrySetting;
use crate::time_limit::TimeScale;
use crate::variables::parse_cli_var;

#[derive(Parser)]
#[command(name = "toad", version, about = "A developer-friendly REST client")]
pub struct Cli {
    /// Path to the collection file
    #[arg(required_unless_present = "schema")]
    pub file: Option<PathBuf>,

    /// Name of the request to run (omits runs all)
    pub requests: Option<String>,

    // Optional output format/verbosity
    #[arg(short, long, value_enum)]
    pub output: Option<OutputFormat>,

    // Only list requests in file
    #[arg(short, long)]
    pub list_requests: bool,

    // Optional profile name to use for vars
    #[arg(short, long)]
    pub profile: Option<String>,

    /// Set a variable for this run, replacing its value from [vars], a profile, or a capture.
    /// The variable must be declared in one of those. Repeat to set more than one.
    #[arg(long = "var", value_name = "NAME=VALUE", value_parser = parse_cli_var)]
    pub vars: Vec<(String, String)>,

    /// Stop before each request and wait for a key: s to step, c to continue to the next
    /// breakpoint, r to run to the end, q to quit
    #[arg(short, long)]
    pub step: bool,

    /// Stop before this request and wait for a key, as with --step. Repeat it or give a
    /// comma-separated list to stop at more than one request.
    #[arg(
        short = 'b',
        long = "break",
        value_name = "REQUEST",
        value_delimiter = ','
    )]
    pub breakpoints: Vec<String>,

    /// Print the JSON Schema for collection files and exit. Editors use it for autocomplete
    /// and to flag misspelled settings.
    #[arg(long, conflicts_with = "file")]
    pub schema: bool,

    /// Path to a custom CA bundle (PEM, JKS, or PKCS12) to trust, overriding any
    /// `use_custom_ca` set in the collection file's [config]
    #[arg(long)]
    pub use_custom_ca: Option<PathBuf>,

    /// Password for the custom CA keystore (JKS/PKCS12 only; not needed for PEM).
    /// Falls back to the TOAD_CA_PASSWORD environment variable if unset.
    #[arg(long)]
    pub use_custom_ca_password: Option<String>,

    /// Multiply every expect_max_ms limit by this factor, or 'off' to skip time limits.
    /// Falls back to the TOAD_TIME_SCALE environment variable if unset.
    #[arg(long, value_name = "FACTOR|off")]
    pub time_scale: Option<TimeScale>,

    /// Retry failed requests this many times, replacing any [config] retry (a request's own
    /// retry still wins), or 'off' to never retry. Falls back to the TOAD_RETRY environment
    /// variable if unset.
    #[arg(long, value_name = "N|off")]
    pub retry: Option<RetrySetting>,
}

#[derive(ValueEnum, Clone, Default, PartialEq)]
pub enum OutputFormat {
    #[default]
    Normal,
    Quiet,
    Silent,
    Verbose,
    ResponseOnly,
    RequestOnly,
    /// One JSON object per line for each event, for scripts and CI reports
    Json,
}
