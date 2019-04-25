mod bakefile;
mod cache;
mod config;
mod entrypoint;
mod format;
mod runner;
mod schedule;

use std::process::exit;

#[macro_use]
extern crate log;
#[macro_use]
extern crate scopeguard;

// Let the fun begin!
fn main() {
  // Jump to the entrypoint and handle any resulting errors.
  if let Err(e) = entrypoint::entry() {
    error!("{}", e);
    exit(1);
  }
}
