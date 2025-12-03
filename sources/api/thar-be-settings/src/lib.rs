/*!
# Background

thar-be-settings is a simple configuration applier.
Its job is to update configuration files and restart services, as necessary, to make the system reflect any changes to settings.

In the normal ("specific keys") mode, it's intended to be called by the Bottlerocket API server after a settings commit.
It's told the keys that changed, and then queries metadata APIs to determine which services and configuration files are affected by changes to those keys.
Detailed data is then fetched for the relevant services and configuration files.
Configuration file data from the API includes paths to template files for each configuration file, along with the final path to write.
It then renders the templates and rewrites the affected configuration files.
Service data from the API includes any commands needed to restart services affected by configuration file changes, which are run here.

In the standalone ("all keys") mode, it queries the API for all services and configuration files, then renders and rewrites all configuration files and restarts all services.
*/

#[macro_use]
extern crate log;

use snafu::ResultExt;
use std::collections::HashSet;
use std::io::{self, Read};
use std::{env, process};
use schnauzer::BottlerocketTemplateImporter;
use simplelog::{Config as LogConfig, LevelFilter, SimpleLogger};
use std::str::FromStr;

pub mod config;
pub mod error;
pub mod service;

pub use error::Error;

type Result<T> = std::result::Result<T, Error>;

mod command_error {
    use settings_committer::SettingsCommitterError;
    use snafu::Snafu;

    #[derive(Debug, Snafu)]
    #[snafu(visibility(pub(crate)))]
    pub(crate) enum Error {
        #[snafu(display("Logger setup error: {}", source))]
        Logger { source: log::SetLoggerError },

        #[snafu(display("Unable to commit pending transaction: {}", source))]
        Commit { source: SettingsCommitterError },
    }
}

/// RunMode represents how thar-be-settings was requested to be run, either handling all
/// configuration files and services, or handling configuration files and services based on
/// specific keys given by the user.
#[derive(Debug)]
enum RunMode {
    All,
    SpecificKeys,
}

/// Store the args we receive on the command line
pub struct Args {
    pub daemon: bool,
    log_level: LevelFilter,
    mode: RunMode,
    socket_path: String,
}

/// Print a usage message in the event a bad arg is passed
fn usage() -> ! {
    let program_name = env::args().next().unwrap_or_else(|| "program".to_string());
    eprintln!(
        r"Usage: {}
            [ --all ]
            [ --daemon ]
            [ --socket-path PATH ]
            [ --log-level trace|debug|info|warn|error ]

    If --all is given, all configuration files will be written and all
    services will have their restart-commands run.  Otherwise, settings keys
    will be read from stdin; only files related to those keys will be written,
    and only services related to those keys will be restarted.

    If --daemon is given, thar-be-settings will fork and do its work in a new
    process; this is useful to prevent blocking an API call.

    Socket path defaults to {}",
        program_name,
        constants::API_SOCKET,
    );
    process::exit(2);
}

/// Prints a more specific message before exiting through usage().
fn usage_msg<S: AsRef<str>>(msg: S) -> ! {
    eprintln!("{}\n", msg.as_ref());
    usage();
}

/// Parse the args to the program and return an Args struct
pub fn parse_args(args: env::Args) -> Args {
    let mut daemon = false;
    let mut log_level = None;
    let mut mode = RunMode::SpecificKeys;
    let mut socket_path = None;

    let mut iter = args.skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_ref() {
            "--all" => mode = RunMode::All,

            "--daemon" => daemon = true,

            "--log-level" => {
                let log_level_str = iter
                    .next()
                    .unwrap_or_else(|| usage_msg("Did not give argument to --log-level"));
                log_level =
                    Some(LevelFilter::from_str(&log_level_str).unwrap_or_else(|_| {
                        usage_msg(format!("Invalid log level '{log_level_str}'"))
                    }));
            }

            "--socket-path" => {
                socket_path = Some(
                    iter.next()
                        .unwrap_or_else(|| usage_msg("Did not give argument to --socket-path")),
                )
            }

            _ => usage(),
        }
    }

    Args {
        daemon,
        mode,
        log_level: log_level.unwrap_or(LevelFilter::Info),
        socket_path: socket_path.unwrap_or_else(|| constants::API_SOCKET.to_string()),
    }
}

/// Render and write config files to disk.  If `files_limit` is Some, only
/// write those files, otherwise write all known files.
async fn write_config_files(
    args: &Args,
    files_limit: Option<HashSet<String>>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Create a vec of ConfigFile structs from the list of changed services
    info!("Requesting configuration file data for affected services");
    let config_files = config::get_affected_config_files(&args.socket_path, files_limit).await?;
    trace!("Found config files: {config_files:?}");

    let template_importer = BottlerocketTemplateImporter::new((&args.socket_path).into());

    // Ensure all files render properly
    info!("Rendering config files...");
    let strict = match &args.mode {
        RunMode::SpecificKeys => true,
        RunMode::All => false,
    };
    let rendered = config::render_config_files(&template_importer, config_files, strict).await?;

    // If all the config renders properly, write it to disk
    info!("Writing config files to disk...");
    config::write_config_files(&rendered)?;

    // If we're done with early boot and only working with specific services,
    // then trigger a reload if necessary.
    if let RunMode::SpecificKeys = &args.mode {
        config::reload_config_files(&rendered)?;
    }

    Ok(())
}

pub async fn run(args: Args) -> std::result::Result<(), Box<dyn std::error::Error>> {
    // SimpleLogger will send errors to stderr and anything less to stdout.
    SimpleLogger::init(args.log_level, LogConfig::default()).context(command_error::LoggerSnafu)?;

    info!("thar-be-settings started");

    settings_committer::commit(constants::API_SOCKET, constants::LAUNCH_TRANSACTION).await.context(command_error::CommitSnafu)?;

    match args.mode {
        RunMode::SpecificKeys => {
            // Get the settings that changed via stdin
            info!("Parsing stdin for updated settings");
            let changed_settings = get_changed_settings()?;

            // Create a HashSet of affected services
            info!(
                "Requesting affected services for settings: {:?}",
                &changed_settings
            );
            let services =
                service::get_affected_services(&args.socket_path, Some(changed_settings)).await?;
            trace!("Found services: {services:?}");
            if services.0.is_empty() {
                info!("No services are affected, exiting...");
                process::exit(0)
            }

            // Create a HashSet of configuration file names
            let config_file_names = config::get_config_file_names(&services);

            if !config_file_names.is_empty() {
                write_config_files(&args, Some(config_file_names)).await?;
            }

            // Now go bounce the affected services
            info!("Restarting affected services...");
            service::restart_services(services)?;
        }
        RunMode::All => {
            write_config_files(&args, None).await?;

            info!("Restarting all services...");
            let services = service::get_affected_services(&args.socket_path, None).await?;
            trace!("Found services: {services:?}");
            service::restart_services(services)?;
        }
    }

    Ok(())
}

/// Read stdin and parse into JSON
pub fn get_changed_settings() -> Result<HashSet<String>> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context(error::ReadInputSnafu { from: "stdin" })?;
    trace!("Raw input from stdin: {}", &input);

    // Settings should be a vec of strings
    debug!("Parsing stdin as JSON");
    let changed_settings: HashSet<String> =
        serde_json::from_str(&input).context(error::InvalidInputSnafu {
            reason: "Input must be a JSON array of strings",
            input,
        })?;
    trace!("Parsed input: {:?}", &changed_settings);

    Ok(changed_settings)
}
