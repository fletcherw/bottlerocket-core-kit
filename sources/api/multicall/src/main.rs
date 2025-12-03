use snafu::{OptionExt, ResultExt};
use std::{env, process};


mod error {
    use std::path::PathBuf;

    use snafu::Snafu;

    #[derive(Debug, Snafu)]
    #[snafu(visibility(pub(super)))]
    pub(super) enum MulticallError {
        #[snafu(display("binary was executed without a name"))]
        MissingName,

        #[snafu(display("current executable is not valid utf-8"))]
        InvalidName,

        #[snafu(display("multicall binary doesn't support binary: {} at path: {}", binary, path.display()))]
        UnsupportedBinary{
            binary: String,
            path: PathBuf,
         },

        #[snafu(display(
            "running sundog failed: {}",
            source
        ))]
        Sundog { source: sundog::SundogError },

        #[snafu(display(
            "running pluto failed: {}",
            source
        ))]
        Pluto { source: Box<dyn std::error::Error> },

        #[snafu(display(
            "running thar-be-settings failed: {}",
            source
        ))]
        TharBeSettings { source: Box<dyn std::error::Error> },
    }
}

use error::MulticallError;

async fn run() -> std::result::Result<(), MulticallError> {
    let binary_path: std::path::PathBuf = env::args().next().context(error::MissingNameSnafu)?.into();
    let binary_name = binary_path.file_name().context(error::MissingNameSnafu)?.to_str().context(error::InvalidNameSnafu)?;
    match binary_name {
        "pluto" => pluto::run().await.context(error::PlutoSnafu),
        "sundog" => sundog::run().await.context(error::SundogSnafu),
        "thar-be-settings" => thar_be_settings::run(thar_be_settings::parse_args(env::args())).await.context(error::TharBeSettingsSnafu),
        _ => {
            Err(error::MulticallError::UnsupportedBinary{ binary: binary_name.to_owned(), path: binary_path })
        }
    }
}

// Returning a Result from main makes it print a Debug representation of the error, but with Snafu
// we have nice Display representations of the error, so we wrap "main" (run) and print any error.
// https://github.com/shepmaster/snafu/issues/110
#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{e}");
        process::exit(1);
    }
}
