mod job;
mod schedule;

use clap::{App, Arg};
use std::{fs, process::exit};

// Defaults
const JOB_FILE_DEFAULT_PATH: &str = "bake.yml";

// Command-line argument and option names
const JOB_FILE_OPTION: &str = "file";
const JOB_TASKS_ARGUMENT: &str = "tasks";

// Let the fun begin!
fn main() {
  // Set up the command-line interface.
  let matches = App::new("Bake")
    .version("0.1.0")
    .author("Stephan Boyer <stephan@stephanboyer.com>")
    .about("Bake is a containerized build system.")
    .arg(
      Arg::with_name(JOB_TASKS_ARGUMENT)
        .value_name("TASKS")
        .multiple(true)
        .help("Sets the tasks to run"),
    )
    .arg(
      Arg::with_name(JOB_FILE_OPTION)
        .short("f")
        .long(JOB_FILE_OPTION)
        .value_name("PATH")
        .help(&format!(
          "Sets the path to the job file (default: {})",
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

  // Parse the tasks.
  let root_tasks: Vec<&str> =
    matches.values_of(JOB_TASKS_ARGUMENT).map_or_else(
      || job.tasks.keys().map(|key| &key[..]).collect(),
      |tasks| {
        tasks
          .map(|task| {
            if !job.tasks.contains_key(task) {
              // [tag:tasks_valid]
              eprintln!("No task named `{}` in `{}`.", task, job_file_path);
              exit(1);
            };
            task
          })
          .collect()
      },
    );

  // Compute a schedule of tasks to run.
  let tasks_to_run = schedule::compute(&job, &root_tasks);

  // Execute the schedule.
  for task in tasks_to_run {
    // Just print the task name for now.
    println!("Running task `{}`...", task);
  }
}
