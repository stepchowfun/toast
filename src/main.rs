mod cache;
mod config;
mod docker;
mod failure;
mod format;
mod runner;
mod schedule;
mod spinner;
mod tar;
mod toastfile;

use crate::{failure::Failure, format::CodeStr};
use clap::{ArgAction, Parser};
use env_logger::{Builder, fmt::style::Effects};
use log::{Level, LevelFilter};
use std::{
    collections::{HashMap, HashSet},
    convert::AsRef,
    default::Default,
    env,
    env::current_dir,
    fs,
    io::{IsTerminal, Write, stdout},
    mem::drop,
    path::{Path, PathBuf},
    process::exit,
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use toastfile::{DEFAULT_USER, default_task_mount_readonly, location, user};
use typed_path::UnixPath;

#[macro_use]
extern crate log;

#[macro_use]
extern crate scopeguard;

// Defaults
const TOASTFILE_DEFAULT_NAME: &str = "toast.yml";
const CONFIG_FILE_XDG_PATH: &str = "toast/toast.yml";
const DEFAULT_LOG_LEVEL: LevelFilter = LevelFilter::Info;

// Set up the logger.
fn set_up_logging() {
    Builder::new()
        .filter_module(
            module_path!(),
            LevelFilter::from_str(
                &env::var("LOG_LEVEL").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.to_string()),
            )
            .unwrap_or(DEFAULT_LOG_LEVEL),
        )
        .format(|buf, record| {
            let level_for_style = match record.level() {
                Level::Trace => Level::Debug,
                level => level,
            };
            let style = buf
                .default_level_style(level_for_style)
                .effects(Effects::BOLD);

            writeln!(
                buf,
                "{style}[{}]{style:#} {}",
                record.level(),
                record.args(),
            )
        })
        .init();
}

// Set up the signal handlers.
fn set_up_signal_handlers(
    docker_cli: String,
    interrupted: Arc<AtomicBool>,
    active_containers: Arc<Mutex<HashSet<String>>>,
) -> Result<(), Failure> {
    // If Toast is in the foreground process group for some TTY, the process will receive a SIGINT
    // when the user types CTRL+C at the terminal. The default behavior is to crash when this signal
    // is received. However, we would rather clean up resources before terminating, so we trap the
    // signal here. This code also traps SIGHUP and SIGTERM, since we compile the `ctrlc` crate with
    // the `termination` feature [ref:ctrlc_term].
    ctrlc::set_handler(move || {
        // Let the rest of the program know the user wants to quit.
        if interrupted.swap(true, Ordering::SeqCst) {
            // Stop any active containers. The `unwrap` will only fail if a panic already occurred.
            for container in &*active_containers.lock().unwrap() {
                if let Err(e) = docker::stop_container(&docker_cli, container, &interrupted) {
                    error!("{e}");
                }
            }

            // We may have been in the middle of printing a line of output. Here we print a newline
            // to prepare for further printing.
            drop(stdout().write(b"\n"));
        }
    })
    .map_err(failure::system("Error installing signal handler."))
}

// Convert a string (from a command-line argument) into a Boolean.
fn parse_bool(s: &str) -> Result<bool, Failure> {
    let normalized = s.trim().to_lowercase();
    match normalized.as_ref() {
        "true" | "yes" => Ok(true),
        "false" | "no" => Ok(false),
        _ => Err(Failure::User(
            format!("{} is not a Boolean.", s.code_str()),
            None,
        )),
    }
}

// This struct represents the raw command-line arguments.
#[derive(Parser)]
#[allow(clippy::struct_excessive_bools)]
#[command(
    about = concat!(
        env!("CARGO_PKG_DESCRIPTION"),
        "\n\n",
        "More information can be found at: ",
        env!("CARGO_PKG_HOMEPAGE")
    ),
    version,
    disable_version_flag = true
)]
struct Cli {
    #[arg(short, long, help = "Print version", action = ArgAction::Version)]
    _version: Option<bool>,

    #[arg(
        short = 'f',
        long = "file",
        value_name = "PATH",
        help = "Set the path to the toastfile"
    )]
    toastfile: Option<PathBuf>,

    #[arg(
        short,
        long,
        value_name = "PATH",
        help = "Set the path to the config file"
    )]
    config_file: Option<PathBuf>,

    #[arg(short, long, value_name = "PATH", help = "Set the output directory")]
    output_dir: Option<PathBuf>,

    #[arg(
        long,
        value_name = "BOOL",
        help = "Set whether local cache reading is enabled"
    )]
    read_local_cache: Option<String>,

    #[arg(
        long,
        value_name = "BOOL",
        help = "Set whether local cache writing is enabled"
    )]
    write_local_cache: Option<String>,

    #[arg(
        long,
        value_name = "BOOL",
        help = "Set whether remote cache reading is enabled"
    )]
    read_remote_cache: Option<String>,

    #[arg(
        long,
        value_name = "BOOL",
        help = "Set whether remote cache writing is enabled"
    )]
    write_remote_cache: Option<String>,

    #[arg(
        short = 'r',
        long,
        value_name = "REPO",
        help = "Set the Docker repository for remote caching"
    )]
    docker_repo: Option<String>,

    #[arg(
        long = "docker-cli",
        value_name = "CLI",
        help = "Set the Docker CLI binary"
    )]
    docker_binary: Option<String>,

    #[arg(short, long, help = "List the tasks that have a description")]
    list: bool,

    #[arg(
        short,
        long = "shell",
        help = "Drop into a containerized shell after the tasks are finished"
    )]
    spawn_shell: bool,

    #[arg(
        long = "force",
        value_name = "TASK",
        help = "Run a task unconditionally, even if it’s cached"
    )]
    forced_tasks: Vec<String>,

    #[arg(long, help = "Pull the base image and run all tasks unconditionally")]
    force_all: bool,

    #[arg(help = "Set the tasks to run")]
    tasks: Vec<String>,
}

// This struct represents the parsed command-line arguments.
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    toastfile_path: PathBuf,
    docker_cli: String,
    docker_repo: String,
    read_local_cache: bool,
    write_local_cache: bool,
    read_remote_cache: bool,
    write_remote_cache: bool,
    list: bool,
    spawn_shell: bool,
    tasks: Option<Vec<String>>,
    forced_tasks: Vec<String>,
    force_all: bool,
    output_dir: PathBuf,
}

// Parse the command-line arguments.
#[allow(clippy::too_many_lines)]
fn settings() -> Result<Settings, Failure> {
    let cli = Cli::parse();

    // Find the toastfile.
    let toastfile_path = cli.toastfile.map_or_else(
        || {
            let mut candidate_dir =
                current_dir().map_err(failure::system("Unable to determine working directory."))?;
            loop {
                let candidate_path = candidate_dir.join(TOASTFILE_DEFAULT_NAME);
                if let Ok(metadata) = fs::metadata(&candidate_path)
                    && metadata.file_type().is_file()
                {
                    return Ok(candidate_path);
                }
                if !candidate_dir.pop() {
                    return Err(Failure::User(
                        format!(
                            "Unable to locate file {}.",
                            TOASTFILE_DEFAULT_NAME.code_str(),
                        ),
                        None,
                    ));
                }
            }
        },
        Ok,
    )?;

    // Read the config file path.
    let default_config_file_path = dirs::config_dir().map(|path| path.join(CONFIG_FILE_XDG_PATH));
    let config_file_path = cli.config_file.or(default_config_file_path);

    // Read the config file path.
    let output_dir = cli.output_dir.unwrap_or_else(|| {
        let mut candidate_dir = toastfile_path.clone();
        candidate_dir.pop();
        candidate_dir
    });

    // Parse the config file.
    let config_data = config_file_path
        .as_ref()
        .and_then(|path| {
            debug!(
                "Attempting to load configuration file {}\u{2026}",
                path.to_string_lossy().code_str(),
            );
            fs::read_to_string(path).ok()
        })
        .map_or_else(
            || {
                debug!("Configuration file not found. Using the default configuration.");
                config::EMPTY_CONFIG.to_owned()
            },
            |data| {
                debug!("Found it.");
                data
            },
        );
    let config = config::parse(&config_data).map_err(failure::user(format!(
        "Unable to parse file {}.",
        config_file_path
            .as_ref()
            .unwrap() // Manually verified safe
            .to_string_lossy()
            .code_str(),
    )))?;

    // Read the local caching switches.
    let read_local_cache = cli
        .read_local_cache
        .as_deref()
        .map_or(Ok(config.read_local_cache), parse_bool)?;
    let write_local_cache = cli
        .write_local_cache
        .as_deref()
        .map_or(Ok(config.write_local_cache), parse_bool)?;

    // Read the remote caching switches.
    let read_remote_cache = cli
        .read_remote_cache
        .as_deref()
        .map_or(Ok(config.read_remote_cache), parse_bool)?;
    let write_remote_cache = cli
        .write_remote_cache
        .as_deref()
        .map_or(Ok(config.write_remote_cache), parse_bool)?;

    // Read the Docker repo.
    let docker_repo = cli.docker_repo.unwrap_or(config.docker_repo);

    // Read the Docker CLI.
    let docker_cli = cli.docker_binary.unwrap_or(config.docker_cli);

    // Read the list of tasks.
    let tasks = (!cli.tasks.is_empty()).then_some(cli.tasks);

    Ok(Settings {
        toastfile_path,
        docker_cli,
        docker_repo,
        read_local_cache,
        write_local_cache,
        read_remote_cache,
        write_remote_cache,
        list: cli.list,
        spawn_shell: cli.spawn_shell,
        tasks,
        forced_tasks: cli.forced_tasks,
        force_all: cli.force_all,
        output_dir,
    })
}

// Parse a toastfile.
fn parse_toastfile(toastfile_path: &Path) -> Result<toastfile::Toastfile, Failure> {
    // Read the file from disk.
    let toastfile_data = fs::read_to_string(toastfile_path).map_err(failure::user(format!(
        "Unable to read file {}.",
        toastfile_path.to_string_lossy().code_str(),
    )))?;

    // Parse it.
    toastfile::parse(&toastfile_data).map_err(failure::user(format!(
        "Unable to parse file {}.",
        toastfile_path.to_string_lossy().code_str(),
    )))
}

// Determine which tasks the user wants to run.
fn get_roots<'a>(
    settings: &'a Settings,
    toastfile: &'a toastfile::Toastfile,
) -> Result<Vec<&'a str>, Failure> {
    // Start with the tasks provided via positional arguments.
    let mut roots: Vec<&'a str> = settings
        .tasks
        .as_ref()
        .map_or_else(Vec::new, |tasks| tasks.iter().map(AsRef::as_ref).collect());

    // Add the tasks that were provided via the `--force` flag.
    roots.extend(
        settings
            .forced_tasks
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<&'a str>>(),
    );

    // For convenience, there is some special behavior for the empty case.
    if roots.is_empty() {
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
    } else {
        // The user provided some tasks. Check that they exist.
        for task in &roots {
            if !toastfile.tasks.contains_key(*task) {
                // [tag:tasks_valid]
                return Err(Failure::User(
                    format!(
                        "No task named {} in {}.",
                        task.code_str(),
                        settings.toastfile_path.to_string_lossy().code_str(),
                    ),
                    None,
                ));
            }
        }

        // Run the tasks that the user provided.
        Ok(roots)
    }
}

// Fetch all the environment variables used by the tasks in the schedule.
fn fetch_environment(
    schedule: &[&str],
    tasks: &HashMap<String, toastfile::Task>,
) -> Result<HashMap<String, String>, Failure> {
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
        return Err(Failure::User(
            format!(
                "The following tasks use variables which are missing from the environment: {}.",
                format::series(
                    violations
                        .iter()
                        .map(|(task_name, vars)| {
                            format!(
                                "{} ({})",
                                task_name.code_str(),
                                format::series(
                                    vars.iter()
                                        .map(|var| format!("{}", var.code_str()))
                                        .collect::<Vec<_>>()
                                        .as_ref(),
                                ),
                            )
                        })
                        .collect::<Vec<_>>()
                        .as_ref(),
                ),
            ),
            None,
        ));
    }

    Ok(env)
}

// Run some tasks and return the final context and the last attempted task. The returned context
// should not be `None` if `need_context` is `true`.
#[allow(clippy::too_many_arguments)]
fn run_tasks(
    schedule: &[&str],
    settings: &Settings,
    toastfile: &toastfile::Toastfile,
    environment: &HashMap<String, String>,
    need_context: bool,
    interrupted: &Arc<AtomicBool>,
    active_containers: &Arc<Mutex<HashSet<String>>>,
) -> (Result<(), Failure>, Option<runner::Context>, Option<String>) {
    // This variable will be `true` as long as we're executing tasks that have `cache: true`. As
    // soon as we encounter a task with `cache: false`, this variable will be permanently set to
    // `false`. If the user provided the `--force-image-pull` flag, this variable will always be
    // `false`.
    let mut caching_enabled = !settings.force_all;

    // We start with the base image.
    let mut context = Some(runner::Context {
        image: toastfile.image.clone(),
        persist: true,
        interrupted: interrupted.clone(),
        docker_cli: settings.docker_cli.clone(),
    });

    // Run each task in the schedule.
    for (i, task_name) in schedule.iter().enumerate() {
        // Fetch the data for the current task.
        let task_data = &toastfile.tasks[*task_name]; // [ref:tasks_valid]

        // If the current task is not cacheable, don't read or write to any form of cache from now
        // on.
        caching_enabled = caching_enabled
            && task_data.cache
            && !settings
                .forced_tasks
                .iter()
                .any(|forced_task| task_name == forced_task);

        // If the user wants to stop the schedule, quit now.
        if interrupted.load(Ordering::SeqCst) {
            return (
                Err(Failure::Interrupted),
                context,
                Some((*task_name).to_owned()),
            );
        }

        // Run the task.
        info!("Running task {}\u{2026}", task_name.code_str());
        let (result, new_context) = runner::run(
            settings,
            environment,
            interrupted,
            active_containers,
            toastfile,
            task_data,
            caching_enabled,
            settings.force_all && i == 0,
            context.unwrap(), // Safe due to [ref:context_needed_if_not_final_task].
            need_context || i != schedule.len() - 1, // [tag:context_needed_if_not_final_task]
        );

        // Remember the context for the next task, if there is one.
        context = new_context;

        // Return an error if the task failed.
        if let Err(e) = result {
            return (Err(e), context, Some((*task_name).to_owned()));
        }
    }

    // Everything succeeded.
    (
        Ok(()),
        context,
        schedule.last().map(|task_name| (*task_name).to_owned()),
    )
}

// Program entrypoint
#[allow(clippy::too_many_lines)]
fn entry() -> Result<(), Failure> {
    // Determine whether to print colored output.
    colored::control::set_override(std::io::stderr().is_terminal());

    // Set up the logger.
    set_up_logging();

    // Set up global mutable state (yum!).
    let interrupted = Arc::new(AtomicBool::new(false));
    let active_containers = Arc::new(Mutex::new(HashSet::<String>::new()));

    // Parse the command-line arguments;
    let settings = settings()?;

    // Set up the signal handlers.
    set_up_signal_handlers(
        settings.docker_cli.clone(),
        interrupted.clone(),
        active_containers.clone(),
    )?;

    // Parse the toastfile.
    let toastfile = parse_toastfile(&settings.toastfile_path)?;

    // If the user just wants to list all the tasks, do that and quit.
    if settings.list {
        info!("Here are the tasks that have a description:");

        // Select the names of the tasks that have a description [tag:tasks_have_descriptions].
        let mut task_names = toastfile
            .tasks
            .iter()
            .filter(|(_, t)| t.description.is_some())
            .map(|(k, _)| k)
            .collect::<Vec<_>>();

        // Sort the names to avoid relying on the unpredictable order of the tasks in the map.
        task_names.sort();

        // Print a summary of each task.
        for task_name in task_names {
            // Fetch the task data.
            let task_data = &toastfile.tasks[task_name];

            // Print the task name and the description. The `unwrap` is safe due to
            // [ref:tasks_have_descriptions].
            println!(
                "* {} \u{2014} {}",
                task_name.code_str(),
                task_data.description.as_ref().unwrap(),
            );

            // Print the environment variables that can be passed to the task.
            for (variable, optional_default) in &task_data.environment {
                if let Some(default) = optional_default {
                    println!("  {}: {}", variable.code_str(), default.code_str());
                } else {
                    println!("  {}: (no default provided)", variable.code_str());
                }
            }
        }

        // The user just wanted to list the tasks. We're done.
        return Ok(());
    }

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
                    .map(|task| task.code_str().to_string())
                    .collect::<Vec<_>>()
                    .as_ref(),
            ),
        );
    }

    // Fetch all the environment variables used by the tasks in the schedule.
    let environment = fetch_environment(&schedule, &toastfile.tasks)?;

    // Execute the schedule.
    let (result, context, last_task) = run_tasks(
        &schedule,
        &settings,
        &toastfile,
        &environment,
        settings.spawn_shell, // [tag:spawn_shell_requires_context]
        &interrupted,
        &active_containers,
    );

    // Return early if needed.
    match result {
        Ok(()) | Err(Failure::User(_, _)) => {
            // Proceed in case the user wants to drop into a shell.
        }
        Err(Failure::Interrupted | Failure::System(_, _)) => {
            // There was an error not caused by a regular task failure. Quit now.
            return result;
        }
    }

    // Drop the user into a shell if requested.
    if settings.spawn_shell {
        // If one of the tasks failed, tell the user now before we drop into a shell.
        if let Err(e) = &result {
            error!("{e}");
        }

        // Inform the user of what's about to happen.
        info!("Preparing a shell\u{2026}");

        // Determine the environment, location, mount settings, ports, and user for the shell.
        let (task_environment, location, mount_paths, mount_readonly, ports, user, extra_args) =
            if let Some(last_task) = last_task {
                // Get the data for the last task.
                let last_task = &toastfile.tasks[&last_task]; // [ref:tasks_valid]

                // Prepare the environment.
                let mut task_environment = HashMap::<String, String>::new();
                for variable in last_task.environment.keys() {
                    // [ref:environment_valid]
                    task_environment.insert(variable.clone(), environment[variable].clone());
                }

                // Use the settings from the last task.
                (
                    task_environment,
                    location(&toastfile, last_task),
                    last_task.mount_paths.clone(),
                    last_task.mount_readonly,
                    last_task.ports.clone(),
                    user(&toastfile, last_task),
                    last_task.extra_docker_arguments.clone(),
                )
            } else {
                // There is no last task, so the context will be the base image. Use default
                // settings.
                (
                    HashMap::default(),            // [ref:default_environment]
                    UnixPath::new("/").to_owned(), // `toastfile::DEFAULT_LOCATION` might not exist.
                    Vec::default(),                // [ref:default_mount_paths]
                    default_task_mount_readonly(),
                    Vec::default(), // [ref:default_ports]
                    DEFAULT_USER.to_owned(),
                    Vec::default(),
                )
            };

        // All relative paths are relative to where the toastfile lives.
        let mut toastfile_dir = PathBuf::from(&settings.toastfile_path);
        toastfile_dir.pop();

        // Spawn the shell.
        docker::spawn_shell(
            &settings.docker_cli,
            &context.unwrap().image, // Safe due to [ref:spawn_shell_requires_context].
            &toastfile_dir,
            &task_environment,
            &location,
            &mount_paths,
            mount_readonly,
            &ports,
            &user,
            &extra_args,
            &interrupted,
        )?;
    }

    // Return the result to the user.
    result
}

// Let the fun begin!
fn main() {
    // Jump to the entrypoint and handle any resulting errors.
    if let Err(e) = entry() {
        error!("{e}");
        exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }
}
