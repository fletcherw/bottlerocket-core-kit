use nix::unistd::{fork, ForkResult};
use std::env;
use std::process;
use tokio::runtime::Runtime;

// Returning a Result from main makes it print a Debug representation of the error, but with Snafu
// we have nice Display representations of the error, so we wrap "main" (run) and print any error.
// https://github.com/shepmaster/snafu/issues/110
//
// In this binary, we also have to do a bit more processing before we get to the "business logic."
// This program is used to apply settings given to the API, but we don't want to block the API, so
// there's a --daemon argument that makes us fork before doing the work.  This also prevents zombie
// processes, since it's simpler to let init wait for our corpse than to make apiserver wait.  To
// determine whether that's wanted, we have to parse args, and then do the fork if requested.
//
// Also, it's not safe to fork within a tokio runtime, so we can't use tokio::main, and have to
// create the runtime manually before we start the business logic in run().
fn main() {
    // Parse and store the args passed to the program
    let args = thar_be_settings::parse_args(env::args());

    if args.daemon {
        match unsafe { fork() } {
            Ok(ForkResult::Child) => {} // continue
            Ok(ForkResult::Parent { .. }) => process::exit(0),
            Err(e) => {
                eprintln!("Failed to fork child: {e}");
                process::exit(1);
            }
        }
    }

    let rt = Runtime::new().expect("Failed to create tokio runtime");
    if let Err(e) = rt.block_on(async { thar_be_settings::run(args).await }) {
        eprintln!("{e}");
        process::exit(1);
    }
}
