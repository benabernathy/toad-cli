use clap::{Parser, ValueEnum};
use std::path::PathBuf;

use crate::time_limit::TimeScale;

#[derive(Parser)]
#[command(name = "toad", version, about = "A developer-friendly REST client")]
pub struct Cli {
    /// Path to the collection file (omit when using --listen)
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

    /// Listen for inbound requests on this port and log/record them, instead of running a collection
    #[arg(long, conflicts_with = "file")]
    pub listen: Option<u16>,

    /// Append captured request output to this file (only used with --listen)
    #[arg(long, requires = "listen")]
    pub output_file: Option<PathBuf>,

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
}
