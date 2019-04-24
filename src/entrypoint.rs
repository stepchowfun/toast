use crate::{bakefile, cache, format, runner, schedule};
use clap::{App, Arg};
use env_logger::{fmt::Color, Builder, Env};
use log::Level;
use rayon::{prelude::*, ThreadPoolBuilder};
use std::{
  cell::{Cell, RefCell},
  collections::HashMap,
  fs,
  io::{stdout, Write},
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
};
use textwrap::Wrapper;

// Defaults
const BAKEFILE_DEFAULT: &str = "bake.yml";
const REPO_DEFAULT: &str = "bake";

// Command-line argument and option names
const BAKEFILE_ARG: &str = "file";
const REMOTE_CACHE_ARG: &str = "remote-cache";
const REPO_ARG: &str = "repo";
const SHELL_ARG: &str = "shell";
const TASKS_ARG: &str = "tasks";

// The maximum number of `docker pull` invocations we can run at a time.
const MAX_CONCURRENT_IMAGE_PULLS: usize = 5;

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
fn set_up_signal_handlers() -> Result<Arc<AtomicBool>, String> {
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
    return Err(format!("Error installing signal handler. Reason: {}", e));
  }

  Ok(running)
}

// This struct represents the command-line arguments.
struct Settings {
  bakefile_path: String,
  docker_repo: String,
  remote_cache: bool,
  spawn_shell: bool,
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
          BAKEFILE_DEFAULT,
        ))
        .takes_value(true),
    )
    .arg(
      Arg::with_name(REMOTE_CACHE_ARG)
        .short("c")
        .long(REMOTE_CACHE_ARG)
        .help("Enables remote caching"),
    )
    .arg(
      Arg::with_name(REPO_ARG)
        .short("r")
        .long(REPO_ARG)
        .value_name("DOCKER REPO")
        .help(&format!(
          "Sets the Docker repository (default: {})",
          REPO_DEFAULT,
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
    .unwrap_or(BAKEFILE_DEFAULT)
    .to_owned();

  // Parse the remote caching switch.
  let remote_cache = matches.is_present(REMOTE_CACHE_ARG);

  // Parse the Docker repo.
  let docker_repo = matches
    .value_of(REPO_ARG)
    .unwrap_or(REPO_DEFAULT)
    .to_owned();

  // Parse the shell switch.
  let spawn_shell = matches.is_present(SHELL_ARG);

  // Parse the tasks.
  let tasks = matches.values_of(TASKS_ARG).map(|tasks| {
    tasks
      .map(std::borrow::ToOwned::to_owned)
      .collect::<Vec<_>>()
  });

  Settings {
    bakefile_path,
    remote_cache,
    docker_repo,
    spawn_shell,
    tasks,
  }
}

// Parse a bakefile.
fn parse_bakefile(bakefile_path: &str) -> Result<bakefile::Bakefile, String> {
  let bakefile_data = fs::read_to_string(bakefile_path).map_err(|e| {
    format!("Unable to read file `{}`. Reason: {}", bakefile_path, e)
  })?;

  bakefile::parse(&bakefile_data).map_err(|e| {
    format!("Unable to parse file `{}`. Reason: {}", bakefile_path, e)
  })
}

// Determine which tasks the user wants to run.
fn get_roots<'a>(
  settings: &'a Settings,
  bakefile: &'a bakefile::Bakefile,
) -> Result<Vec<&'a str>, String> {
  settings.tasks.as_ref().map_or_else(
    || {
      // The user didn't provide any tasks. Check if there is a default task.
      if let Some(default) = &bakefile.default {
        // There is a default; use it.
        Ok(vec![&default[..]])
      } else {
        // There is no default. Run all the tasks.
        Ok(
          bakefile
            .tasks
            .keys()
            .map(|key| &key[..])
            .collect::<Vec<_>>(),
        )
      }
    },
    |tasks| {
      // The user provided some tasks. Check that they exist, and run them.
      for task in tasks {
        if !bakefile.tasks.contains_key(task) {
          // [tag:tasks_valid]
          return Err(format!(
            "No task named `{}` in `{}`.",
            task, settings.bakefile_path
          ));
        }
      }

      Ok(tasks.iter().map(|task| &task[..]).collect())
    },
  )
}

// Fetch all the environment variables used by the tasks in the schedule.
fn fetch_env(
  schedule: &[&str],
  tasks: &HashMap<String, bakefile::Task>,
) -> Result<HashMap<String, String>, String> {
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
    return Err(format!(
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
    ));
  }

  Ok(env)
}

// Run some tasks.
fn run_tasks<'a>(
  schedule: &[&'a str],
  settings: &Settings,
  bakefile: &bakefile::Bakefile,
  env: &HashMap<String, String>,
  running: &Arc<AtomicBool>,
) -> Result<(), String> {
  // Eagerly try to pull all images that will be needed in parallel.
  let mut images_to_pull = vec![];

  if !runner::image_exists(&bakefile.image) {
    images_to_pull.push(bakefile.image.clone());
  }

  if settings.remote_cache {
    let mut schedule_prefix = vec![];

    for task in schedule {
      if !bakefile.tasks[*task].cache {
        break;
      }

      schedule_prefix.push(&bakefile.tasks[*task]); // [ref:tasks_valid]

      let image = format!(
        "{}:{}",
        settings.docker_repo,
        cache::key(&bakefile.image, &schedule_prefix, &env)
      );

      if !runner::image_exists(&image) {
        images_to_pull.push(image);
      }
    }
  }

  info!(
    "Warming the cache by attempting to pull {}...",
    format::number(images_to_pull.len(), "image")
  );
  ThreadPoolBuilder::new()
    .num_threads(MAX_CONCURRENT_IMAGE_PULLS)
    .build()
    .unwrap() // If this fails, we're doomed.
    .install(|| {
      images_to_pull.par_iter().for_each(|image| {
        let _ = runner::pull_image(image);
      });
    });

  // Run each task in sequence.
  let mut schedule_prefix = vec![];
  let from_image = RefCell::new(bakefile.image.clone());
  let from_image_cacheable = Cell::new(true);
  let can_use_cache = Cell::new(true);
  for task in schedule {
    // At the end of this iteration, delete the image from the previous step if
    // it isn't cacheable.
    let image_to_delete = if from_image_cacheable.get() {
      None
    } else {
      Some(from_image.borrow().to_owned())
    };
    defer! {{
      if let Some(image) = image_to_delete {
        if let Err(e) = runner::delete_image(&image) {
          error!("{}", e);
        }
      }
    }}

    // If the user wants to stop the job, quit now.
    if !running.load(Ordering::SeqCst) {
      return Err("Interrupted.".to_owned());
    }

    // Compute the cache key.
    schedule_prefix.push(&bakefile.tasks[*task]); // [ref:tasks_valid]
    let cache_key = cache::key(&bakefile.image, &schedule_prefix, &env);
    let to_image =
      RefCell::new(format!("{}:{}", settings.docker_repo, cache_key));

    // Remember this image for the next task.
    defer! {{
      from_image.replace(to_image.borrow().clone());
      from_image_cacheable.set(can_use_cache.get());
    }}

    // Skip the task if it's cached.
    if bakefile.tasks[*task].cache {
      if can_use_cache.get() {
        // Check the local cache.
        if runner::image_exists(&to_image.borrow()) {
          info!("Task `{}` found in local cache.", task);
          continue;
        }

        // Check the remote cache if applicable.
        if settings.remote_cache
          && runner::pull_image(&to_image.borrow()).is_ok()
        {
          // Skip to the next task.
          info!("Task `{}` found in remote cache.", task);
          continue;
        }
      }
    } else {
      can_use_cache.set(false);
    }

    // Run the task.
    // The indexing is safe due to [ref:tasks_valid].
    info!("Running task `{}`...", task);
    runner::run(
      &bakefile.tasks[*task],
      &from_image.borrow(),
      &to_image.borrow(),
      &env,
      running,
    )?;

    // Push the image to a remote cache if applicable.
    if settings.remote_cache && can_use_cache.get() {
      info!("Writing to cache...");
      match runner::push_image(&to_image.borrow()) {
        Ok(()) => info!("Task `{}` maybe pushed to remote cache.", task),
        Err(e) => warn!("{}", e),
      };
    }
  }

  // Delete the final image if it isn't cacheable.
  defer! {{
    if !from_image_cacheable.get() {
      if let Err(e) = runner::delete_image(&from_image.borrow()) {
        error!("{}", e);
      }
    }
  }}

  // Tell the user the good news!
  info!(
    "Successfully executed {}.",
    format::number(schedule.len(), "task")
  );

  // Drop the user into a shell if requested.
  if settings.spawn_shell {
    info!("Here's a shell in the context of the tasks that were executed:");
    runner::spawn_shell(&from_image.borrow())?;
  }

  // Everything succeeded.
  Ok(())
}

// Program entrypoint
pub fn entry() -> Result<(), String> {
  // Set up the logger.
  set_up_logging();

  // Set up the signal handlers.
  let running = set_up_signal_handlers()?;

  // Parse the command-line arguments;
  let settings = settings();

  // Parse the bakefile.
  let bakefile = parse_bakefile(&settings.bakefile_path)?;

  // Determine which tasks the user wants to run.
  let root_tasks = get_roots(&settings, &bakefile)?;

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
  let env = fetch_env(&schedule, &bakefile.tasks)?;

  // Execute the schedule.
  run_tasks(&schedule, &settings, &bakefile, &env, &running)?;

  // Everything succeeded.
  Ok(())
}
