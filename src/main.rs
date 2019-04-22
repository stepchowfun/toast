mod bakefile;
mod cache;
mod runner;
mod schedule;

#[macro_use]
extern crate log;

use clap::{App, Arg};
use env_logger::{fmt::Color, Builder, Env};
use log::Level;
use std::{
  fs,
  io::Write,
  process::exit,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
};

// Defaults
const JOB_FILE_DEFAULT_PATH: &str = "bake.yml";

// Command-line argument and option names
const BAKEFILE_OPTION: &str = "file";
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
    let mut style = buf.style();
    style.set_bold(true);
    match record.level() {
      Level::Error => {
        style.set_color(Color::Red);
      }
      Level::Warn => {
        style.set_color(Color::Yellow);
      }
      Level::Info => {
        style.set_color(Color::Green);
      }
      Level::Debug | Level::Trace => {
        style.set_color(Color::Blue);
      }
    }
    writeln!(
      buf,
      "{} {}",
      style.value(format!("[{}]", record.level())),
      record.args().to_string().replace(
        "\n",
        &format!("\n{}", " ".repeat(record.level().to_string().len() + 3))
      )
    )
  })
  .init();

  // Set up the Ctrl+C handler.
  let running = Arc::new(AtomicBool::new(true));
  let running_ref = running.clone();
  if let Err(e) = ctrlc::set_handler(move || {
    running_ref.store(false, Ordering::SeqCst);
  }) {
    error!("Error installing signal handler. Reason: {}", e);
    exit(1);
  }

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
      Arg::with_name(BAKEFILE_OPTION)
        .short("f")
        .long(BAKEFILE_OPTION)
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
    .value_of(BAKEFILE_OPTION)
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
  info!(
    "Here is the schedule: {}.",
    (schedule
      .iter()
      .map(|task| format!("`{}`", task))
      .collect::<Vec<_>>())
    .join(", ")
  );

  // Execute the schedule.
  let mut from_image = bakefile.image.clone();
  let mut schedule_prefix = vec![];
  for task in &schedule {
    // If the user wants to stop the job, quit now.
    if !running.load(Ordering::SeqCst) {
      info!("Terminating...");
      exit(1);
    }

    // Run the task.
    info!("Running task `{}`...", task);
    schedule_prefix.push(&bakefile.tasks[*task]);
    let cache_key = cache::key(&bakefile.image, &schedule_prefix);
    let to_image = format!("bake:{}", cache_key);
    if let Err(e) = runner::run(&bakefile.tasks[*task], &from_image, &to_image)
    {
      error!("{}", e);
      exit(1);
    }
    from_image = to_image;
  }

  // Celebrate with the user.
  info!("Successfully executed the schedule.");
}
