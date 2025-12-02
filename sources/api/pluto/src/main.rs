/*!
# Introduction

pluto is called by sundog to generate settings required by Kubernetes.
This is done dynamically because we require access to dynamic networking
and cluster setup information.

It uses IMDS to get information such as:

- Instance Type
- Node IP

It uses EKS to get information such as:

- Service IP CIDR

It uses the Bottlerocket API to get information such as:

- Kubernetes Cluster Name
- AWS Region

# Interface

Pluto takes the name of the setting that it is to generate as its first
argument.
It returns the generated setting to stdout as a JSON document.
Any other output is returned to stderr.

Pluto returns a special exit code of 2 to inform `sundog` that a setting should be skipped. For
example, if `max-pods` cannot be generated, we want `sundog` to skip it without failing since a
reasonable default is available.
*/

use std::process;

// Returning a Result from main makes it print a Debug representation of the error, but with Snafu
// we have nice Display representations of the error, so we wrap "main" (run) and print any error.
// https://github.com/shepmaster/snafu/issues/110
#[tokio::main]
async fn main() {
    if let Err(e) = pluto::run().await {
        eprintln!("{e}");
        process::exit(1);
    }
}
