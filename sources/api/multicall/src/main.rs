use snafu::{OptionExt, ResultExt};
use std::{env, process};


mod error {
    use snafu::Snafu;

    #[derive(Debug, Snafu)]
    #[snafu(visibility(pub(super)))]
    pub(super) enum MulticallError {
        #[snafu(display("couldn't get name of executable: {}", source))]
        CurrentExe{ source: std::io::Error },

        #[snafu(display("current executable is not a file"))]
        MissingName,

        #[snafu(display("current executable is not valid utf-8"))]
        InvalidName,

        #[snafu(display("multicall binary doesn't support binary: {}", binary))]
        UnsupportedBinary{ binary: String },

        #[snafu(display(
            "running sundog failed: {}",
            source
        ))]
        Sundog { source: sundog::SundogError },
    }
}

use error::MulticallError;

async fn run() -> std::result::Result<(), MulticallError> {
    let binary_path = env::current_exe().context(error::CurrentExeSnafu)?;
    let binary_name = binary_path.file_name().context(error::MissingNameSnafu)?.to_str().context(error::InvalidNameSnafu)?;
    match binary_name {
        "pluto" => unimplemented!(),
        "sundog" => sundog::run().await.context(error::SundogSnafu),
        _ => {
            Err(error::MulticallError::UnsupportedBinary{ binary: binary_name.to_owned() })
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
