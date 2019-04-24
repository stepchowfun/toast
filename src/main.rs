mod bakefile;
mod cache;
mod format;
mod runner;
mod schedule;

#[macro_use]
extern crate log;
#[macro_use]
extern crate scopeguard;

use clap::{App, Arg};
use env_logger::{fmt::Color, Builder, Env};
use log::Level;
use std::{
  collections::HashMap,
  fs,
  io::{stdout, Write},
  process::exit,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
};
use textwrap::Wrapper;

// Defaults
const JOB_FILE_DEFAULT_PATH: &str = "bake.yml";

// Command-line argument and option names
const BAKEFILE_ARG: &str = "file";
const SHELL_ARG: &str = "shell";
const TASKS_ARG: &str = "tasks";

// Set up the logger.
fn set_up_logging() {
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
    let indent_size = record.level().to_string().len() + 3;
    let indent = &" ".repeat(indent_size);
    writeln!(
      buf,
      "{} {}",
      style.value(format!("[{}]", record.level())),
      &Wrapper::with_termwidth()
        .initial_indent(indent)
        .subsequent_indent(indent)
        .fill(&record.args().to_string())[indent_size..]
    )
  })
  .init();
}

// Set up the signal handlers. Returns a reference to a Boolean indicating
// whether the user requested graceful termination.
fn set_up_signal_handlers() -> Arc<AtomicBool> {
  // Set up the SIGINT handler that ignores the signal. If the user presses
  // CTRL+C, all processes attached to the foreground process group receive
  // the signal, which includes processes in the container since we use the
  // `--tty` option with `docker create` [ref:tty]. So the user can kill the
  // container directly, and by ignoring SIGINT here we get a chance to clean
  // up afterward.
  let running = Arc::new(AtomicBool::new(true));
  let running_ref = running.clone();
  if let Err(e) = ctrlc::set_handler(move || {
    // Remember that the user wants to quit.
    running_ref.store(false, Ordering::SeqCst);

    // If the user interrupted the container, the container may have been in
    // the middle of printing a line of output. Here we print a newline to
    // prepare for further printing.
    let _ = stdout().write(b"\n");
  }) {
    error!("Error installing signal handler. Reason: {}", e);
    exit(1);
  }

  running
}

// This struct represents the command-line arguments.
struct Settings {
  bakefile_path: String,
  shell: bool,
  tasks: Option<Vec<String>>,
}

// Parse the command-line arguments;
fn settings() -> Settings {
  let matches = App::new("Bake")
    .version("0.1.0")
    .version_short("v")
    .author("Stephan Boyer <stephan@stephanboyer.com>")
    .about("Bake is a containerized build system.")
    .arg(
      Arg::with_name(BAKEFILE_ARG)
        .short("f")
        .long(BAKEFILE_ARG)
        .value_name("PATH")
        .help(&format!(
          "Sets the path to the bakefile (default: {})",
          JOB_FILE_DEFAULT_PATH,
        ))
        .takes_value(true),
    )
    .arg(
      Arg::with_name(SHELL_ARG)
        .short("s")
        .long(SHELL_ARG)
        .help("Drops you into a shell after the tasks are complete"),
    )
    .arg(
      Arg::with_name(TASKS_ARG)
        .value_name("TASKS")
        .multiple(true)
        .help("Sets the tasks to run"),
    )
    .get_matches();

  // Parse the bakefile path.
  let bakefile_path = matches
    .value_of(BAKEFILE_ARG)
    .unwrap_or(JOB_FILE_DEFAULT_PATH)
    .to_owned();

  // Parse the --shell flag.
  let shell = matches.is_present(SHELL_ARG);

  // Parse the tasks.
  let tasks = matches.values_of(TASKS_ARG).map(|tasks| {
    tasks
      .map(std::borrow::ToOwned::to_owned)
      .collect::<Vec<_>>()
  });

  Settings {
    bakefile_path,
    shell,
    tasks,
  }
}

// Parse a bakefile.
fn parse_bakefile(bakefile_path: &str) -> bakefile::Bakefile {
  let bakefile_data = fs::read_to_string(bakefile_path).unwrap_or_else(|e| {
    error!("Unable to read file `{}`. Reason: {}", bakefile_path, e);
    exit(1);
  });

  bakefile::parse(&bakefile_data).unwrap_or_else(|e| {
    error!("Unable to parse file `{}`. Reason: {}", bakefile_path, e);
    exit(1);
  })
}

// Determine which tasks the user wants to run.
fn get_roots<'a>(
  settings: &'a Settings,
  bakefile: &'a bakefile::Bakefile,
) -> Vec<&'a str> {
  settings.tasks.as_ref().map_or_else(
    || {
      // The user didn't provide any tasks. Run all of them.
      bakefile
        .tasks
        .keys()
        .map(|key| &key[..])
        .collect::<Vec<_>>()
    },
    |tasks| {
      // The user provided some tasks. Check that they exist, and run them.
      tasks
        .iter()
        .map(|task| {
          if !bakefile.tasks.contains_key(task) {
            // [tag:tasks_valid]
            error!(
              "No task named `{}` in `{}`.",
              task, settings.bakefile_path
            );
            exit(1);
          };
          &task[..]
        })
        .collect()
    },
  )
}

// Fetch all the environment variables used by the tasks in the schedule.
fn fetch_env(
  schedule: &[&str],
  tasks: &HashMap<String, bakefile::Task>,
) -> HashMap<String, String> {
  let mut env = HashMap::new();
  let mut violations = HashMap::new();

  for task in schedule {
    match bakefile::environment(&tasks[*task]) {
      // [ref:tasks_valid]
      Ok(env_for_task) => {
        env.extend(env_for_task);
      }
      Err(vars) => {
        violations.insert((*task).to_owned(), vars);
      }
    }
  }

  if !violations.is_empty() {
    // [tag:env_valid]
    error!(
      "The following tasks are missing variables from the environment: {}.",
      format::series(
        &violations
          .iter()
          .map(|(task, vars)| format!(
            "`{}` ({})",
            task,
            format::series(
              &vars
                .iter()
                .map(|var| format!("`{}`", var))
                .collect::<Vec<_>>()[..]
            )
          ))
          .collect::<Vec<_>>()[..]
      )
    );
    exit(1);
  }

  env
}

// Let the fun begin!
fn main() {
  // Set up the logger.
  set_up_logging();

  // Set up the signal handlers.
  let running = set_up_signal_handlers();

  // Parse the command-line arguments;
  let settings = settings();

  // Parse the bakefile.
  let bakefile = parse_bakefile(&settings.bakefile_path);

  // Determine which tasks the user wants to run.
  let root_tasks = get_roots(&settings, &bakefile);

  // Compute a schedule of tasks to run.
  let schedule = schedule::compute(&bakefile, &root_tasks);
  info!(
    "The following tasks will be executed in the order given: {}.",
    format::series(
      &schedule
        .iter()
        .map(|task| format!("`{}`", task))
        .collect::<Vec<_>>()[..]
    )
  );

  // Fetch all the environment variables used by the tasks in the schedule.
  let env = fetch_env(&schedule, &bakefile.tasks);

  // Execute the schedule.
  let mut from_image = bakefile.image.clone();
  let mut from_image_cacheable = true;
  let mut schedule_prefix = vec![];
  let mut can_use_cache = true;
  let mut succeeded = true;
  for task in &schedule {
    // If the user wants to stop the job, quit now.
    if !running.load(Ordering::SeqCst) {
      error!("Interrupted.");
      succeeded = false;
      break;
    }

    // Compute the cache key.
    schedule_prefix.push(&bakefile.tasks[*task]); // [ref:tasks_valid]
    let cache_key = cache::key(&bakefile.image, &schedule_prefix, &env);
    let to_image = format!("bake:{}", cache_key);

    // Skip the task if it's cached.
    if bakefile.tasks[*task].cache {
      if can_use_cache && runner::image_exists(&to_image) {
        // Remember this image for the next task.
        from_image = to_image;
        from_image_cacheable = true;

        // Skip to the next task.
        info!("Task `{}` found in cache.", task);
        continue;
      }
    } else {
      can_use_cache = false;
    }

    // Run the task.
    // The indexing is safe due to [ref:tasks_valid].
    info!("Running task `{}`...", task);
    if let Err(e) =
      runner::run(&bakefile.tasks[*task], &from_image, &to_image, &env)
    {
      if running.load(Ordering::SeqCst) {
        error!("{}", e);
      } else {
        error!("Interrupted.");
      }

      succeeded = false;
      break;
    }

    // Delete the previous image if it isn't cacheable.
    if !from_image_cacheable {
      if let Err(e) = runner::delete_image(&from_image) {
        error!("{}", e);
        succeeded = false;
        break;
      }
    }

    // Remember this image for the next task.
    from_image = to_image;
    from_image_cacheable = can_use_cache;
  }

  if succeeded {
    // Tell the user the good news!
    info!(
      "Successfully executed {}.",
      format::number(schedule.len(), "task")
    );

    // Drop the user into a shell if requested.
    if settings.shell {
      info!("Here's a shell in the context of the tasks that were executed:");
      if let Err(e) = runner::run_shell(&from_image) {
        error!("{}", e);
      }
    }

    // Delete the final image if it isn't cacheable.
    if !from_image_cacheable {
      if let Err(e) = runner::delete_image(&from_image) {
        error!("{}", e);
      }
    }
  } else {
    // Something went wrong.
    exit(1);
  }
}
