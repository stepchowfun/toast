mod cache;
mod config;
mod docker;
mod format;
mod runner;
mod schedule;
mod spinner;
mod tar;
mod toastfile;

use crate::format::CodeStr;
use atty::Stream;
use clap::{App, AppSettings, Arg};
use env_logger::{fmt::Color, Builder};
use log::{Level, LevelFilter};
use std::{
    collections::{HashMap, HashSet},
    convert::AsRef,
    env,
    env::current_dir,
    fs,
    io::{stdout, Write},
    path::Path,
    path::PathBuf,
    process::exit,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate log;

#[macro_use]
extern crate scopeguard;

// The program version
const VERSION: &str = env!("CARGO_PKG_VERSION");

// Defaults
const TOASTFILE_DEFAULT_NAME: &str = "toast.yml";
const CONFIG_FILE_XDG_PATH: &str = "toast/toast.yml";
const DEFAULT_LOG_LEVEL: LevelFilter = LevelFilter::Info;

// Command-line argument and option names
const TOASTFILE_ARG: &str = "file";
const CONFIG_FILE_ARG: &str = "config-file";
const READ_LOCAL_CACHE_ARG: &str = "read-local-cache";
const WRITE_LOCAL_CACHE_ARG: &str = "write-local-cache";
const READ_REMOTE_CACHE_ARG: &str = "read-remote-cache";
const WRITE_REMOTE_CACHE_ARG: &str = "write-remote-cache";
const REPO_ARG: &str = "repo";
const SHELL_ARG: &str = "shell";
const TASKS_ARG: &str = "tasks";

// The error message to log when Toast is interrupted.
const INTERRUPT_MESSAGE: &str = "Interrupted.";

// Set up the logger.
fn set_up_logging() {
    Builder::new()
        .filter_module(
            module_path!(),
            LevelFilter::from_str(
                &env::var("LOG_LEVEL")
                    .unwrap_or_else(|_| DEFAULT_LOG_LEVEL.to_string()),
            )
            .unwrap_or_else(|_| DEFAULT_LOG_LEVEL),
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
                record.args().to_string()
            )
        })
        .init();
}

// Set up the signal handlers.
fn set_up_signal_handlers(
    interrupted: Arc<AtomicBool>,
    active_containers: Arc<Mutex<HashSet<String>>>,
) -> Result<(), String> {
    // If Toast is in the foreground process group for some TTY, the process will
    // receive a SIGINT when the user types CTRL+C at the terminal. The default
    // behavior is to crash when this signal is received. However, we would
    // rather clean up resources before terminating, so we trap the signal here.
    // This code also traps SIGTERM, because we compile the `ctrlc` crate with
    // the `termination` feature [ref:ctrlc_term].
    ctrlc::set_handler(move || {
        // Let the rest of the program know the user wants to quit.
        if interrupted.swap(true, Ordering::SeqCst) {
            // Stop any active containers. The `unwrap` will only fail if a panic
            // already occurred.
            for container in &*active_containers.lock().unwrap() {
                if let Err(e) =
                    docker::stop_container(&container, &interrupted)
                {
                    error!("{}", e);
                }
            }

            // We may have been in the middle of printing a line of output. Here we
            // print a newline to prepare for further printing.
            let _ = stdout().write(b"\n");
        }
    })
    .map_err(|e| format!("Error installing signal handler. Details: {}.", e))
}

// Convert a string (from a command-line argument) into a Boolean.
fn parse_bool(s: &str) -> Result<bool, String> {
    let normalized = s.trim().to_lowercase();
    match normalized.as_ref() {
        "true" | "yes" => Ok(true),
        "false" | "no" => Ok(false),
        _ => Err(format!("{} is not a Boolean.", s.code_str())),
    }
}

// This struct represents the command-line arguments.
pub struct Settings {
    toastfile_path: PathBuf,
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
    let matches = App::new("Toast")
        .version(VERSION)
        .version_short("v")
        .author("Stephan Boyer <stephan@stephanboyer.com>")
        .about("Toast is a containerized build system.")
        .setting(AppSettings::ColoredHelp)
        .setting(AppSettings::NextLineHelp)
        .setting(AppSettings::UnifiedHelpMessage)
        .arg(
            Arg::with_name(TOASTFILE_ARG)
                .short("f")
                .long(TOASTFILE_ARG)
                .value_name("PATH")
                .help("Sets the path to the toastfile")
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

    // Find the toastfile.
    let toastfile_path = matches.value_of(TOASTFILE_ARG).map_or_else(
        || {
            let mut candidate_dir = current_dir().map_err(|e| {
                format!(
                    "Unable to determine working directory. Details: {}.",
                    e
                )
            })?;
            loop {
                let candidate_path =
                    candidate_dir.join(TOASTFILE_DEFAULT_NAME);
                if let Ok(metadata) = fs::metadata(&candidate_path) {
                    if metadata.file_type().is_file() {
                        return Ok(candidate_path);
                    }
                }
                if !candidate_dir.pop() {
                    return Err(format!(
                        "Unable to locate file {}.",
                        TOASTFILE_DEFAULT_NAME.code_str()
                    ));
                }
            }
        },
        |x| Ok(Path::new(x).to_owned()),
    )?;

    // Read the config file path.
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
            debug!(
                "Attempting to load configuration file {}\u{2026}",
                path.to_string_lossy().code_str()
            );
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
            "Unable to parse file {}. Details: {}",
            config_file_path
                .as_ref()
                .unwrap()
                .to_string_lossy()
                .code_str(), // Manually verified safe
            e
        )
    })?;

    // Read the local caching switches.
    let read_local_cache = matches
        .value_of(READ_LOCAL_CACHE_ARG)
        .map_or(Ok(config.read_local_cache), |s| parse_bool(s))?;
    let write_local_cache = matches
        .value_of(WRITE_LOCAL_CACHE_ARG)
        .map_or(Ok(config.write_local_cache), |s| parse_bool(s))?;

    // Read the remote caching switches.
    let read_remote_cache = matches
        .value_of(READ_REMOTE_CACHE_ARG)
        .map_or(Ok(config.read_remote_cache), |s| parse_bool(s))?;
    let write_remote_cache = matches
        .value_of(WRITE_REMOTE_CACHE_ARG)
        .map_or(Ok(config.write_remote_cache), |s| parse_bool(s))?;

    // Read the Docker repo.
    let docker_repo = matches
        .value_of(REPO_ARG)
        .unwrap_or(&config.docker_repo)
        .to_owned();

    // Read the shell switch.
    let spawn_shell = matches.is_present(SHELL_ARG);

    // Read the list of tasks.
    let tasks = matches.values_of(TASKS_ARG).map(|tasks| {
        tasks
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<_>>()
    });

    Ok(Settings {
        toastfile_path,
        read_local_cache,
        write_local_cache,
        read_remote_cache,
        write_remote_cache,
        docker_repo,
        spawn_shell,
        tasks,
    })
}

// Parse a toastfile.
fn parse_toastfile(
    toastfile_path: &Path,
) -> Result<toastfile::Toastfile, String> {
    // Read the file from disk.
    let toastfile_data = fs::read_to_string(toastfile_path).map_err(|e| {
        format!(
            "Unable to read file {}. Details: {}.",
            toastfile_path.to_string_lossy().code_str(),
            e
        )
    })?;

    // Parse it.
    toastfile::parse(&toastfile_data).map_err(|e| {
        format!(
            "Unable to parse file {}. Details: {}",
            toastfile_path.to_string_lossy().code_str(),
            e
        )
    })
}

// Determine which tasks the user wants to run.
fn get_roots<'a>(
    settings: &'a Settings,
    toastfile: &'a toastfile::Toastfile,
) -> Result<Vec<&'a str>, String> {
    settings.tasks.as_ref().map_or_else(
        || {
            // The user didn't provide any tasks. Check if there is a default task.
            if let Some(default) = &toastfile.default {
                // There is a default. Use it.
                Ok(vec![default.as_ref()])
            } else {
                // There is no default. Run all the tasks.
                Ok(toastfile
                    .tasks
                    .keys()
                    .map(AsRef::as_ref)
                    .collect::<Vec<_>>())
            }
        },
        |tasks| {
            // The user provided some tasks. Check that they exist, and run them.
            for task in tasks {
                if !toastfile.tasks.contains_key(task) {
                    // [tag:tasks_valid]
                    return Err(format!(
                        "No task named {} in {}.",
                        task.code_str(),
                        settings.toastfile_path.to_string_lossy().code_str()
                    ));
                }
            }

            Ok(tasks.iter().map(AsRef::as_ref).collect())
        },
    )
}

// Fetch all the environment variables used by the tasks in the schedule.
fn fetch_environment(
    schedule: &[&str],
    tasks: &HashMap<String, toastfile::Task>,
) -> Result<HashMap<String, String>, String> {
    let mut env = HashMap::new();
    let mut violations = HashMap::new();

    for task in schedule {
        match toastfile::environment(&tasks[*task]) {
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
            "{} ({})",
            task.code_str(),
            format::series(
              vars
                .iter()
                .map(|var| format!("{}", var.code_str()))
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
#[allow(clippy::too_many_arguments)]
fn run_tasks(
    schedule: &[&str],
    settings: &Settings,
    toastfile: &toastfile::Toastfile,
    environment: &HashMap<String, String>,
    interrupted: &Arc<AtomicBool>,
    active_containers: &Arc<Mutex<HashSet<String>>>,
) -> Result<runner::Context, (String, runner::Context)> {
    // This variable will be `true` as long as we're executing tasks that have
    // `cache: true`. As soon as we encounter a task with `cache: false`, this
    // variable will be permanently set to `false`.
    let mut caching_enabled = true;

    // This is the cache key for the current task. We initialize it with the base
    // image name.
    let mut cache_key = toastfile.image.clone();

    // We start with the base image.
    let mut context = runner::Context {
        image: toastfile.image.clone(),
        persist: true,
        interrupted: interrupted.clone(),
    };

    // Run each task in the schedule.
    for task in schedule {
        // Fetch the data for the current task.
        let task_data = &toastfile.tasks[*task]; // [ref:tasks_valid]

        // If the current task is not cacheable, don't read or write to any form of
        // cache from now on.
        caching_enabled = caching_enabled && task_data.cache;

        // If the user wants to stop the schedule, quit now.
        if interrupted.load(Ordering::SeqCst) {
            return Err((INTERRUPT_MESSAGE.to_owned(), context));
        }

        // Run the task.
        info!("Running task {}\u{2026}", task.code_str());
        let (new_cache_key, new_context) = runner::run(
            settings,
            &environment,
            &interrupted,
            &active_containers,
            task_data,
            &cache_key,
            caching_enabled,
            context,
        )?;

        // Remember the cache key and context for the next task.
        cache_key = new_cache_key;
        context = new_context;
    }

    // Everything succeeded.
    Ok(context)
}

// Program entrypoint
fn entry() -> Result<(), String> {
    // Determine whether to print colored output.
    colored::control::set_override(atty::is(Stream::Stdout));

    // Set up the logger.
    set_up_logging();

    // Set up global mutable state (yum!).
    let interrupted = Arc::new(AtomicBool::new(false));
    let active_containers = Arc::new(Mutex::new(HashSet::<String>::new()));

    // Set up the signal handlers.
    set_up_signal_handlers(interrupted.clone(), active_containers.clone())?;

    // Parse the command-line arguments;
    let settings = settings()?;

    // Parse the toastfile.
    let toastfile = parse_toastfile(&settings.toastfile_path)?;

    // Determine which tasks the user wants to run.
    let root_tasks = get_roots(&settings, &toastfile)?;

    // Compute a schedule of tasks to run.
    let schedule = schedule::compute(&toastfile, &root_tasks);
    if !schedule.is_empty() {
        info!(
            "Ready to run {}: {}.",
            format::number(schedule.len(), "task"),
            format::series(
                schedule
                    .iter()
                    .map(|task| format!("{}", task.code_str()))
                    .collect::<Vec<_>>()
                    .as_ref()
            )
        );
    }

    // Fetch all the environment variables used by the tasks in the schedule.
    let environment = fetch_environment(&schedule, &toastfile.tasks)?;

    // Execute the schedule.
    let (succeeded, context) = match run_tasks(
        &schedule,
        &settings,
        &toastfile,
        &environment,
        &interrupted,
        &active_containers,
    ) {
        Ok(context) => {
            // Log the success and proceed in case the user wants to be dropped into
            // a shell.
            info!("Done.");
            (true, context)
        }
        Err((e, context)) => {
            // If the schedule failed because the user interrupted a task, quit now.
            if interrupted.load(Ordering::SeqCst) {
                return Err(INTERRUPT_MESSAGE.to_owned());
            }

            // Log the error and proceed in case the user wants to be dropped into a
            // shell.
            error!("{}", e);
            (false, context)
        }
    };

    // Drop the user into a shell if requested.
    if settings.spawn_shell {
        // Inform the user of what's about to happen.
        info!("Preparing a shell\u{2026}");

        // Spawn the shell.
        docker::spawn_shell(&context.image, &interrupted)?;
    }

    // Throw an error if any of the tasks failed.
    if succeeded {
        Ok(())
    } else {
        Err("One of the tasks failed.".to_owned())
    }
}

// Let the fun begin!
fn main() {
    // Jump to the entrypoint and handle any resulting errors.
    if let Err(e) = entry() {
        error!("{}", e);
        exit(1);
    }
}
