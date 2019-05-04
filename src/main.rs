mod bakefile;
mod cache;
mod config;
mod format;
mod runner;
mod schedule;
mod tar;

use clap::{App, AppSettings, Arg};
use env_logger::{fmt::Color, Builder, Env};
use log::Level;
use std::{
  cell::{Cell, RefCell},
  collections::{HashMap, HashSet},
  convert::AsRef,
  env::current_dir,
  fs,
  io::{stdout, Seek, SeekFrom, Write},
  path::Path,
  path::PathBuf,
  process::exit,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
  },
};
use tempfile::tempfile;
use textwrap::Wrapper;

#[macro_use]
extern crate log;
#[macro_use]
extern crate scopeguard;

// The program version
const VERSION: &str = "0.3.0";

// Defaults
const BAKEFILE_DEFAULT_NAME: &str = "bake.yml";
const CONFIG_FILE_XDG_PATH: &str = "bake/bake.yml";

// Command-line argument and option names
const BAKEFILE_ARG: &str = "file";
const CONFIG_FILE_ARG: &str = "config-file";
const READ_LOCAL_CACHE_ARG: &str = "read-local-cache";
const WRITE_LOCAL_CACHE_ARG: &str = "write-local-cache";
const READ_REMOTE_CACHE_ARG: &str = "read-remote-cache";
const WRITE_REMOTE_CACHE_ARG: &str = "write-remote-cache";
const REPO_ARG: &str = "repo";
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

// Set up the signal handlers.
fn set_up_signal_handlers(
  running: Arc<AtomicBool>,
  active_containers: Arc<Mutex<HashSet<String>>>,
) -> Result<(), String> {
  // If STDOUT is a TTY, the process will receive a SIGINT when
  // the user types CTRL+C at the terminal. The default behavior is to crash
  // when this signal is received. However, we would rather clean up resources
  // before terminating, so we trap the signal here. This code also traps
  // SIGTERM, because we compile the `ctrlc` crate with the `termination`
  // feature [ref:ctrlc_term].
  ctrlc::set_handler(move || {
    // Let the rest of the program know the user wants to quit.
    if running.swap(false, Ordering::SeqCst) {
      // Acknowledge the request to quit. We may have been in the middle of
      // printing a line of output, so here we print a newline before emitting
      // the log message.
      let _ = stdout().write(b"\n");
      info!("Terminating...");

      // Stop any active containers. The `unwrap` will only fail if a panic
      // already occurred.
      for container in &*active_containers.lock().unwrap() {
        if let Err(e) = runner::stop_container(&container) {
          error!("{}", e);
        }
      }

      // We may have been in the middle of printing a line of output. Here we
      // print a newline to prepare for further printing.
      let _ = stdout().write(b"\n");
    }
  })
  .map_err(|e| format!("Error installing signal handler. Details: {}", e))
}

// Convert a string (from a command-line argument) into a Boolean.
fn parse_bool(s: &str) -> Result<bool, String> {
  let normalized = s.trim().to_lowercase();
  match normalized.as_ref() {
    "true" | "yes" => Ok(true),
    "false" | "no" => Ok(false),
    _ => Err(format!("`{}` is not a Boolean.", s)),
  }
}

// This struct represents the command-line arguments.
struct Settings {
  bakefile_path: PathBuf,
  docker_repo: String,
  read_local_cache: bool,
  write_local_cache: bool,
  read_remote_cache: bool,
  write_remote_cache: bool,
  spawn_shell: bool,
  tasks: Option<Vec<String>>,
}

// Parse the command-line arguments;
fn settings() -> Result<Settings, String> {
  let matches = App::new("Bake")
    .version(VERSION)
    .version_short("v")
    .author("Stephan Boyer <stephan@stephanboyer.com>")
    .about("Bake is a containerized build system.")
    .setting(AppSettings::ColoredHelp)
    .setting(AppSettings::NextLineHelp)
    .setting(AppSettings::UnifiedHelpMessage)
    .arg(
      Arg::with_name(BAKEFILE_ARG)
        .short("f")
        .long(BAKEFILE_ARG)
        .value_name("PATH")
        .help("Sets the path to the bakefile")
        .takes_value(true),
    )
    .arg(
      Arg::with_name(CONFIG_FILE_ARG)
        .short("c")
        .long(CONFIG_FILE_ARG)
        .value_name("PATH")
        .help("Sets the path of the config file")
        .takes_value(true),
    )
    .arg(
      Arg::with_name(READ_LOCAL_CACHE_ARG)
        .long(READ_LOCAL_CACHE_ARG)
        .value_name("BOOL")
        .help("Sets whether local cache reading is enabled")
        .takes_value(true),
    )
    .arg(
      Arg::with_name(WRITE_LOCAL_CACHE_ARG)
        .long(WRITE_LOCAL_CACHE_ARG)
        .value_name("BOOL")
        .help("Sets whether local cache writing is enabled")
        .takes_value(true),
    )
    .arg(
      Arg::with_name(READ_REMOTE_CACHE_ARG)
        .long(READ_REMOTE_CACHE_ARG)
        .value_name("BOOL")
        .help("Sets whether remote cache reading is enabled")
        .takes_value(true),
    )
    .arg(
      Arg::with_name(WRITE_REMOTE_CACHE_ARG)
        .long(WRITE_REMOTE_CACHE_ARG)
        .value_name("BOOL")
        .help("Sets whether remote cache writing is enabled")
        .takes_value(true),
    )
    .arg(
      Arg::with_name(REPO_ARG)
        .short("r")
        .long(REPO_ARG)
        .value_name("REPO")
        .help("Sets the Docker repository")
        .takes_value(true),
    )
    .arg(
      Arg::with_name(SHELL_ARG)
        .short("s")
        .long(SHELL_ARG)
        .help("Drops you into a shell after the tasks are finished"),
    )
    .arg(
      Arg::with_name(TASKS_ARG)
        .value_name("TASKS")
        .multiple(true)
        .help("Sets the tasks to run"),
    )
    .get_matches();

  // Determine the bakefile path.
  let bakefile_path = matches.value_of(BAKEFILE_ARG).map_or_else(
    || {
      let mut candidate_dir = current_dir().map_err(|e| {
        format!(
          "Unable to determine current working directory. Details: {}",
          e
        )
      })?;
      loop {
        let candidate_path = candidate_dir.join(BAKEFILE_DEFAULT_NAME);
        if let Ok(metadata) = fs::metadata(&candidate_path) {
          if metadata.file_type().is_file() {
            return Ok(candidate_path);
          }
        }
        if !candidate_dir.pop() {
          return Err(format!(
            "Unable to locate file `{}`.",
            BAKEFILE_DEFAULT_NAME
          ));
        }
      }
    },
    |x| Ok(Path::new(x).to_owned()),
  )?;

  // Parse the config file path.
  let default_config_file_path =
    dirs::config_dir().map(|path| path.join(CONFIG_FILE_XDG_PATH));
  let config_file_path = matches.value_of(CONFIG_FILE_ARG).map_or_else(
    || default_config_file_path,
    |path| Some(PathBuf::from(path)),
  );

  // Parse the config file.
  let config_data = config_file_path
    .as_ref()
    .and_then(|path| {
      debug!("Loading configuration file `{}`...", path.to_string_lossy());
      fs::read_to_string(path).ok()
    })
    .map_or_else(
      || {
        debug!(
          "Configuration file not found. Using the default configuration."
        );
        config::EMPTY_CONFIG.to_owned()
      },
      |data| {
        debug!("Found it.");
        data
      },
    );
  let config = config::parse(&config_data).map_err(|e| {
    format!(
      "Unable to parse file `{}`. Details: {}.",
      config_file_path.as_ref().unwrap().to_string_lossy(), // Manually verified safe
      e
    )
  })?;

  // Parse the local caching switches.
  let read_local_cache = matches
    .value_of(READ_LOCAL_CACHE_ARG)
    .map_or(Ok(config.read_local_cache), |s| parse_bool(s))?;
  let write_local_cache = matches
    .value_of(WRITE_LOCAL_CACHE_ARG)
    .map_or(Ok(config.write_local_cache), |s| parse_bool(s))?;

  // Parse the remote caching switches.
  let read_remote_cache = matches
    .value_of(READ_REMOTE_CACHE_ARG)
    .map_or(Ok(config.read_remote_cache), |s| parse_bool(s))?;
  let write_remote_cache = matches
    .value_of(WRITE_REMOTE_CACHE_ARG)
    .map_or(Ok(config.write_remote_cache), |s| parse_bool(s))?;

  // Parse the Docker repo.
  let docker_repo = matches
    .value_of(REPO_ARG)
    .unwrap_or(&config.docker_repo)
    .to_owned();

  // Parse the shell switch.
  let spawn_shell = matches.is_present(SHELL_ARG);

  // Parse the tasks.
  let tasks = matches.values_of(TASKS_ARG).map(|tasks| {
    tasks
      .map(std::borrow::ToOwned::to_owned)
      .collect::<Vec<_>>()
  });

  Ok(Settings {
    bakefile_path,
    read_local_cache,
    write_local_cache,
    read_remote_cache,
    write_remote_cache,
    docker_repo,
    spawn_shell,
    tasks,
  })
}

// Parse a bakefile.
fn parse_bakefile(bakefile_path: &Path) -> Result<bakefile::Bakefile, String> {
  // Read the file from disk.
  let bakefile_data = fs::read_to_string(bakefile_path).map_err(|e| {
    format!(
      "Unable to read file `{}`. Details: {}",
      bakefile_path.to_string_lossy(),
      e
    )
  })?;

  // Parse it.
  bakefile::parse(&bakefile_data).map_err(|e| {
    format!(
      "Unable to parse file `{}`. Details: {}",
      bakefile_path.to_string_lossy(),
      e
    )
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
        Ok(vec![default.as_ref()])
      } else {
        // There is no default. Run all the tasks.
        Ok(bakefile.tasks.keys().map(AsRef::as_ref).collect::<Vec<_>>())
      }
    },
    |tasks| {
      // The user provided some tasks. Check that they exist, and run them.
      for task in tasks {
        if !bakefile.tasks.contains_key(task) {
          // [tag:tasks_valid]
          return Err(format!(
            "No task named `{}` in `{}`.",
            task,
            settings.bakefile_path.to_string_lossy()
          ));
        }
      }

      Ok(tasks.iter().map(AsRef::as_ref).collect())
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
    // [tag:environment_valid]
    return Err(format!(
      "The following tasks use variables which are missing from the environment: {}.",
      format::series(
        violations
          .iter()
          .map(|(task, vars)| format!(
            "`{}` ({})",
            task,
            format::series(
              vars
                .iter()
                .map(|var| format!("`{}`", var))
                .collect::<Vec<_>>().as_ref()
            )
          ))
          .collect::<Vec<_>>().as_ref()
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
  active_containers: &Arc<Mutex<HashSet<String>>>,
) -> Result<(), String> {
  // Pull the base image. Docker will do this automatically when we run the
  // first task, but we do it explicitly here so the user knows what's
  // happening and when it's done.
  let base_image_already_existed = runner::image_exists(&bakefile.image);
  if !base_image_already_existed {
    info!("Pulling image `{}`...", bakefile.image);
    runner::pull_image(&bakefile.image)?;
  }

  // Run each task in sequence.
  let mut cache_key = cache::hash_str(&bakefile.image);
  let mut first_task = true;
  let from_image = RefCell::new(bakefile.image.clone());
  let from_image_cacheable = Cell::new(true);
  for task in schedule {
    let task_data = &bakefile.tasks[*task]; // [ref:tasks_valid]

    // At the end of this iteration, delete the image from the previous step if
    // it isn't cacheable.
    let image_to_delete = if (settings.write_local_cache
      && from_image_cacheable.get())
      || (first_task && base_image_already_existed)
    {
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
    first_task = false;

    // If the user wants to stop the job, quit now.
    if !running.load(Ordering::SeqCst) {
      return Err("Interrupted.".to_owned());
    }

    // Tar up the files to be copied into the container.
    let tar_file = tempfile().map_err(|e| {
      format!("Unable to create temporary file. Details: {}", e)
    })?;
    let mut bakefile_dir = PathBuf::from(&settings.bakefile_path);
    bakefile_dir.pop();
    let (mut tar_file, files_hash) = tar::create(
      tar_file,
      &task_data.paths,
      &bakefile_dir.to_string_lossy().to_string(),
      &task_data.location,
    )?;
    tar_file
      .seek(SeekFrom::Start(0))
      .map_err(|e| format!("Unable to seek temporary file. Details: {}", e))?;

    // Compute the cache key.
    cache_key = cache::key(&cache_key, &task_data, &files_hash, &env);
    let to_image =
      RefCell::new(format!("{}:{}", settings.docker_repo, cache_key));

    // Remember this image for the next task.
    let this_task_cacheable = task_data.cache && from_image_cacheable.get();
    defer! {{
      from_image.replace(to_image.borrow().clone());
      from_image_cacheable.set(this_task_cacheable);
    }}

    // Skip the task if it's cached.
    if this_task_cacheable {
      // Check the local cache.
      if settings.read_local_cache && runner::image_exists(&to_image.borrow())
      {
        info!("Task `{}` found in local cache.", task);
        continue;
      }

      // Check the remote cache if applicable.
      if settings.read_remote_cache {
        info!("Attempting to fetch task `{}` from remote cache...", task);
        if runner::pull_image(&to_image.borrow()).is_ok() {
          // Skip to the next task.
          info!("Task `{}` fetched from remote cache.", task);
          continue;
        }
        info!("Task `{}` not found in remote cache.", task);
      }
    }

    // If the user wants to stop the job, quit now.
    if !running.load(Ordering::SeqCst) {
      return Err("Interrupted.".to_owned());
    }

    // Run the task.
    info!("Running task `{}`...", task);
    runner::run(
      task_data,
      &from_image.borrow(),
      &to_image.borrow(),
      &env,
      tar_file,
      &running,
      &active_containers,
    )?;

    // If the user wants to stop the job, quit now.
    if !running.load(Ordering::SeqCst) {
      return Err("Interrupted.".to_owned());
    }

    // Push the image to a remote cache if applicable.
    if settings.write_remote_cache && this_task_cacheable {
      info!("Writing to remote cache...");
      match runner::push_image(&to_image.borrow()) {
        Ok(()) => info!("Task `{}` pushed to remote cache.", task),
        Err(e) => warn!("{}", e),
      };
    }
  }

  // Delete the final image if it isn't cacheable.
  defer! {{
    if !settings.write_local_cache || !from_image_cacheable.get() {
      if let Err(e) = runner::delete_image(&from_image.borrow()) {
        error!("{}", e);
      }
    }
  }}

  // Tell the user the good news!
  info!("{} finished.", format::number(schedule.len(), "task"));

  // Drop the user into a shell if requested.
  if settings.spawn_shell {
    info!("Here's a shell in the context of the tasks that were executed:");
    runner::spawn_shell(&from_image.borrow())?;
  }

  // Everything succeeded.
  Ok(())
}

// Program entrypoint
fn entry() -> Result<(), String> {
  // Set up the logger.
  set_up_logging();

  // Set up global mutable state (yum!).
  let running = Arc::new(AtomicBool::new(true));
  let active_containers = Arc::new(Mutex::new(HashSet::<String>::new()));

  // Set up the signal handlers.
  set_up_signal_handlers(running.clone(), active_containers.clone())?;

  // Parse the command-line arguments;
  let settings = settings()?;

  // Parse the bakefile.
  let bakefile = parse_bakefile(&settings.bakefile_path)?;

  // Determine which tasks the user wants to run.
  let root_tasks = get_roots(&settings, &bakefile)?;

  // Compute a schedule of tasks to run.
  let schedule = schedule::compute(&bakefile, &root_tasks);
  info!(
    "The following tasks will be executed in the order given: {}.",
    format::series(
      schedule
        .iter()
        .map(|task| format!("`{}`", task))
        .collect::<Vec<_>>()
        .as_ref()
    )
  );

  // Fetch all the environment variables used by the tasks in the schedule.
  let env = fetch_env(&schedule, &bakefile.tasks)?;

  // Execute the schedule.
  run_tasks(
    &schedule,
    &settings,
    &bakefile,
    &env,
    &running,
    &active_containers,
  )?;

  // Everything succeeded.
  Ok(())
}

// Let the fun begin!
fn main() {
  // Jump to the entrypoint and handle any resulting errors.
  if let Err(e) = entry() {
    error!("{}", e);
    exit(1);
  }
}
