mod job;

use clap::{App, Arg};
use std::{fs, process::exit};

// Defaults
const JOB_FILE_DEFAULT_PATH: &str = "bake.yml";

// Command-line option names
const JOB_FILE_OPTION: &str = "file";

fn main() {
  // Set up the command-line interface.
  let matches = App::new("Bake")
    .version("0.1.0")
    .author("Stephan Boyer <stephan@stephanboyer.com>")
    .about("Bake is a containerized build system.")
    .arg(
      Arg::with_name(JOB_FILE_OPTION)
        .short("f")
        .long(JOB_FILE_OPTION)
        .value_name("PATH")
        .help(&format!(
          "Sets the path of the job file (default: {})",
          JOB_FILE_DEFAULT_PATH,
        ))
        .takes_value(true),
    )
    .get_matches();

  // Parse the job file path.
  let job_file_path = matches
    .value_of(JOB_FILE_OPTION)
    .unwrap_or(JOB_FILE_DEFAULT_PATH);

  // Parse the job file.
  let job_data = fs::read_to_string(job_file_path).unwrap_or_else(|e| {
    eprintln!("Unable to read file `{}`. Reason: {}", job_file_path, e);
    exit(1);
  });
  let job = job::parse(&job_data).unwrap_or_else(|e| {
    eprintln!("Unable to parse file `{}`. Reason: {}", job_file_path, e);
    exit(1);
  });

  // Build a map from task name to task ID.
  let _job_index = job::index(&job).unwrap_or_else(|e| {
    eprintln!("Unable to parse file `{}`. Reason: {}", job_file_path, e);
    exit(1);
  });
}
