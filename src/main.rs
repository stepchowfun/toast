mod bakefile;
mod cache;
mod runner;
mod schedule;

#[macro_use]
extern crate log;

use clap::{App, Arg};
use env_logger::{Builder, Env};
use std::{fs, io::Write, process::exit};

// Defaults
const JOB_FILE_DEFAULT_PATH: &str = "bake.yml";

// Command-line argument and option names
const JOB_FILE_OPTION: &str = "file";
const TASKS_ARGUMENT: &str = "tasks";

// Let the fun begin!
fn main() {
  // Set up the logger.
  Builder::from_env(
    Env::default()
      .filter_or("LOG_LEVEL", "info")
      .write_style("LOG_STYLE"),
  )
  .format(|buf, record| {
    writeln!(buf, "[{}] {}", record.level(), record.args())
  })
  .init();

  // Set up the command-line interface.
  let matches = App::new("Bake")
    .version("0.1.0")
    .author("Stephan Boyer <stephan@stephanboyer.com>")
    .about("Bake is a containerized build system.")
    .arg(
      Arg::with_name(TASKS_ARGUMENT)
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
          "Sets the path to the bakefile (default: {})",
          JOB_FILE_DEFAULT_PATH,
        ))
        .takes_value(true),
    )
    .get_matches();

  // Parse the bakefile path.
  let bakefile_file_path = matches
    .value_of(JOB_FILE_OPTION)
    .unwrap_or(JOB_FILE_DEFAULT_PATH);

  // Parse the bakefile.
  let bakefile_data =
    fs::read_to_string(bakefile_file_path).unwrap_or_else(|e| {
      error!(
        "Unable to read file `{}`. Reason: {}",
        bakefile_file_path, e
      );
      exit(1);
    });
  let bakefile = bakefile::parse(&bakefile_data).unwrap_or_else(|e| {
    error!(
      "Unable to parse file `{}`. Reason: {}",
      bakefile_file_path, e
    );
    exit(1);
  });

  // Parse the tasks.
  let root_tasks: Vec<&str> = matches.values_of(TASKS_ARGUMENT).map_or_else(
    || bakefile.tasks.keys().map(|key| &key[..]).collect(),
    |tasks| {
      tasks
        .map(|task| {
          if !bakefile.tasks.contains_key(task) {
            // [tag:tasks_valid]
            error!("No task named `{}` in `{}`.", task, bakefile_file_path);
            exit(1);
          };
          task
        })
        .collect()
    },
  );

  // Compute a schedule of tasks to run.
  let schedule = schedule::compute(&bakefile, &root_tasks);

  // Execute the schedule.
  let mut from_image = bakefile.image.clone();
  let mut schedule_prefix = vec![];
  for task in schedule {
    info!("Running task `{}`...", task);
    schedule_prefix.push(&bakefile.tasks[task]);
    let cache_key = cache::key(&bakefile.image, &schedule_prefix);
    let to_image = format!("bake:{}", cache_key);
    if let Err(e) = runner::run(&bakefile.tasks[task], &from_image, &to_image)
    {
      error!("{}", e);
      exit(1);
    }
    from_image = to_image;
  }
}
