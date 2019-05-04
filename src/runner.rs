use crate::bakefile::Task;
use std::{
  collections::{HashMap, HashSet},
  io,
  io::Read,
  process::{ChildStdin, Command, Stdio},
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
  },
};

// Run a task and return the ID of the resulting Docker image.
pub fn run<R: Read>(
  task: &Task,
  from_image: &str,
  to_image: &str,
  environment: &HashMap<String, String>,
  mut tar: R,
  running: &Arc<AtomicBool>,
  active_containers: &Arc<Mutex<HashSet<String>>>,
) -> Result<(), String> {
  // Construct the command to run inside the container.
  let mut commands_to_run = vec![];

  // Ensure the task's location exists within the container and that the user
  // can access it.
  commands_to_run.push(format!(
    "mkdir -p {}",
    shell_escape(&task.location.to_string_lossy())
  ));
  commands_to_run.push(format!(
    "chmod 777 {}",
    shell_escape(&task.location.to_string_lossy())
  ));

  // Construct a small script to execute the task's command in the task's
  // location as the task's user with the task's environment variables.
  if let Some(command) = &task.command {
    commands_to_run.push(format!(
      "cd {}",
      shell_escape(&task.location.to_string_lossy())
    ));

    for variable in task.environment.keys() {
      commands_to_run.push(format!(
        "export {}={}",
        shell_escape(variable),
        shell_escape(&environment[variable]), // [ref:environment_valid]
      ));
    }

    commands_to_run.push(format!(
      "su -c {} {}",
      shell_escape(&command),
      shell_escape(&task.user)
    ));
  }

  // Create the container.
  let command_str = commands_to_run.join(" && ");
  debug!(
    "Creating container from image `{}` with command `{}`...",
    from_image, command_str
  );

  // Why `--init`? (1) PID 1 is supposed to reap orphaned zombie processes,
  // otherwise they can accumulate. Bash does this, but we run `/bin/sh` in the
  // container, which may or may not be Bash. So `--init` runs Tini
  // (https://github.com/krallin/tini) as PID 1, which properly reaps orphaned
  // zombies. (2) PID 1 also does not exhibit the default behavior (crashing)
  // for signals like SIGINT and SIGTERM. However, PID 1 can still handle these
  // signals by explicitly trapping them. Tini traps these signals and forwards
  // them to the child process. Then the default signal handling behavior of
  // the child process (in our case, `/bin/sh`) works normally. [tag:--init]
  let container_id = run_docker_quiet(
    vec![
      "container",
      "create",
      "--init",
      from_image,
      "/bin/sh",
      "-c",
      command_str.as_ref(),
    ]
    .as_ref(),
    "Unable to create container.",
  )?
  .trim()
  .to_owned();
  debug!("Created container `{}`.", container_id);

  {
    // If the user interrupts the program, kill the container. The `unwrap`
    // will only fail if a panic already occurred.
    active_containers
      .lock()
      .unwrap()
      .insert(container_id.clone());
  }

  // Delete the container when this function returns.
  defer! {{
    debug!("Deleting container...");
    {
      // If the user interrupts the program, don't bother killing the
      // container. We're about to kill it here.
      active_containers.lock().unwrap().remove(&container_id);
    }
    if let Err(e) = run_docker_quiet(
      &["container", "rm", "--force", &container_id],
      "Unable to delete container."
    ) {
      error!("{}", e);
    }
  }};

  // Copy files into the container, if applicable.
  if !task.paths.is_empty() {
    run_docker_quiet_stdin(
      &["container", "cp", "-", &format!("{}:{}", container_id, "/")],
      "Unable to copy files into the container.",
      |mut stdin| {
        io::copy(&mut tar, &mut stdin).map_err(|e| {
          format!("Unable to copy files into the container.. Details: {}", e)
        })?;

        Ok(())
      },
    )
    .map_err(|e| {
      if running.load(Ordering::SeqCst) {
        e
      } else {
        "Interrupted.".to_owned()
      }
    })?;
  }

  // Start the container.
  debug!("Starting container...");
  if let Some(command) = &task.command {
    info!("{}", command);
  }
  run_docker_loud(
    &["container", "start", "--attach", &container_id],
    "Task failed.",
  )
  .map_err(|e| {
    if running.load(Ordering::SeqCst) {
      e
    } else {
      "Interrupted.".to_owned()
    }
  })?;

  // Create an image from the container.
  debug!("Creating image...");
  run_docker_quiet(
    &["container", "commit", &container_id, to_image],
    "Unable to create image.",
  )?;
  debug!("Created image `{}`.", to_image);

  Ok(())
}

// Query whether a Docker image exists locally.
pub fn image_exists(image: &str) -> bool {
  debug!("Checking existence of image `{}`...", image);
  run_docker_quiet(
    &["image", "inspect", image],
    &format!("The image `{}` does not exist.", image),
  )
  .is_ok()
}

// Push a Docker image.
pub fn push_image(image: &str) -> Result<(), String> {
  debug!("Pushing image `{}`...", image);
  run_docker_loud(
    &["image", "push", image],
    &format!("Unable to push image `{}`.", image),
  )
  .map(|_| ())
}

// Pull a Docker image.
pub fn pull_image(image: &str) -> Result<(), String> {
  debug!("Pulling image `{}`...", image);
  run_docker_loud(
    &["image", "pull", image],
    &format!("Unable to pull image `{}`.", image),
  )
  .map(|_| ())
}

// Stop a Docker container.
pub fn stop_container(container: &str) -> Result<(), String> {
  debug!("Stopping container `{}`...", container);
  run_docker_quiet(
    &["stop", container],
    &format!("Unable to stop container `{}`.", container),
  )
  .map(|_| ())
}

// Delete a Docker image.
pub fn delete_image(image: &str) -> Result<(), String> {
  debug!("Deleting image `{}`...", image);
  run_docker_quiet(
    &["image", "rm", "--force", image],
    &format!("Unable to delete image `{}`.", image),
  )
  .map(|_| ())
}

// Run an interactive shell and block until it exits.
pub fn spawn_shell(image: &str) -> Result<(), String> {
  run_docker_attach(
    &[
      "container",
      "run",
      "--rm",
      "--interactive",
      "--tty",
      "--init", // [ref:--init]
      image,
      "/bin/su", // We use `su` rather than `sh` to use the root user's shell.
      "-l",
    ],
    "The shell exited with a failure.",
  )
}

// Construct a Docker `Command` from an array of arguments.
fn docker_command(args: &[&str]) -> Command {
  let mut command = Command::new("docker");
  for arg in args {
    command.arg(arg);
  }
  command
}

// Run a command and return its standard output or an error message. Accepts a
// closure which receives a pipe to the STDIN of the child process.
fn run_docker_quiet_stdin<W: FnOnce(&mut ChildStdin) -> Result<(), String>>(
  args: &[&str],
  error: &str,
  writer: W,
) -> Result<String, String> {
  let mut child = docker_command(args)
    .stdin(Stdio::piped()) // [tag:stdin_piped]
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|e| format!("{}\nDetails: {}", error, e))?;
  writer(child.stdin.as_mut().unwrap())?; // [ref:stdin_piped]
  let output = child
    .wait_with_output()
    .map_err(|e| format!("{}\nDetails: {}", error, e))?;
  if !output.status.success() {
    return Err(format!(
      "{}\nDetails: {}",
      error,
      String::from_utf8_lossy(&output.stderr)
    ));
  }
  Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// Run a command and return its standard output or an error message.
fn run_docker_quiet(args: &[&str], error: &str) -> Result<String, String> {
  let output = docker_command(args)
    .stdin(Stdio::null())
    .output()
    .map_err(|e| format!("{}\nDetails: {}", error, e))?;
  if !output.status.success() {
    return Err(format!(
      "{}\nDetails: {}",
      error,
      String::from_utf8_lossy(&output.stderr)
    ));
  }
  Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// Run a command and forward its standard output and error streams.
fn run_docker_loud(args: &[&str], error: &str) -> Result<(), String> {
  let status = docker_command(args)
    .stdin(Stdio::null())
    .status()
    .map_err(|e| format!("{}\nDetails: {}", error, e))?;
  if !status.success() {
    return Err(error.to_owned());
  }
  Ok(())
}

// Run a command and forward its standard input, output, and error streams.
fn run_docker_attach(args: &[&str], error: &str) -> Result<(), String> {
  let status = docker_command(args)
    .status()
    .map_err(|e| format!("{}\nDetails: {}", error, e))?;
  if !status.success() {
    return Err(error.to_owned());
  }
  Ok(())
}

// Escape a string for shell interpolation.
fn shell_escape(command: &str) -> String {
  format!("'{}'", command.replace("'", "'\\''"))
}

#[cfg(test)]
mod tests {
  use crate::runner::shell_escape;

  #[test]
  fn shell_escape_empty() {
    assert_eq!(shell_escape(""), "''");
  }

  #[test]
  fn shell_escape_word() {
    assert_eq!(shell_escape("foo"), "'foo'");
  }

  #[test]
  fn shell_escape_single_quote() {
    assert_eq!(shell_escape("f'o'o"), "'f'\\''o'\\''o'");
  }
}
