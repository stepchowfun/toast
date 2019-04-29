use crate::bakefile::Task;
use atty::Stream;
use std::{
  collections::HashMap,
  io,
  io::Read,
  process::{ChildStdin, Command, Stdio},
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
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
) -> Result<(), String> {
  // Construct the command to run inside the container.
  let mut commands_to_run = vec![];

  // Ensure the task's location exists within the container and that the user
  // can access it.
  commands_to_run
    .push(format!("mkdir -p '{}'", task.location.to_string_lossy()));
  commands_to_run
    .push(format!("chmod 777 '{}'", task.location.to_string_lossy()));

  // Construct a small script to execute the task's command in the task's
  // location as the task's user with the task's environment variables.
  if let Some(command) = &task.command {
    commands_to_run.push(format!(
      "su -l -c {} {}",
      shell_escape(&format!(
        "{} set -eu; cd {}; {}",
        task
          .environment
          .keys()
          .map(|var| {
            format!(
              "export {}={};",
              shell_escape(&var),
              shell_escape(&environment[var]) // [ref:environment_valid]
            )
          })
          .collect::<Vec<_>>()
          .join(" "),
        shell_escape(&task.location.to_string_lossy()),
        command,
      )),
      shell_escape(&task.user)
    ));
  }

  // Create the container.
  debug!("Creating container from image `{}`...", from_image);
  let command_str = commands_to_run.join(" && ");

  // Why `--init`? (1) PID 1 is supposed to reap orphaned zombie processes,
  // otherwise they can accumulate. Bash does this, but we run `/bin/sh` in the
  // container, which may or may not be Bash. So `--init` runs Tini
  // (https://github.com/krallin/tini) as PID 1, which properly reaps orphaned
  // zombies. (2) PID 1 also does not exhibit the default behavior (crashing)
  // for signals like SIGINT and SIGTERM. However, PID 1 can still handle these
  // signals by explicitly trapping them. Tini traps these signals and forwards
  // them to the child process. Then the default signal handling behavior of
  // the child process (in our case, `/bin/sh`) works normally. [tag:--init]
  let mut create_command = vec!["container", "create", "--init"];

  if atty::is(Stream::Stdout) {
    // [tag:docker-tty] If STDOUT is a terminal, tell the Docker client to
    // behave like a TTY for the container. That means it will, for example,
    // send a SIGINT signal to the container's foreground process group when it
    // receives the end-of-text (^C) character on STDIN. This allows the user
    // to kill the container with CTRL+C. If STDOUT is not a terminal, then
    // we don't have the container behave as if it were attached to one. Some
    // programs (this one included) query whether they are attached to a
    // terminal and exhibit different behavior in that case (e.g., printing
    // with color), and we want to make sure those programs behave correctly.
    // See also [ref:bake-tty].
    create_command.push("--tty");
  }
  create_command
    .extend([from_image, "/bin/sh", "-c", &command_str[..]].iter());
  let container_id =
    run_docker_quiet(&create_command[..], "Unable to create container.")?
      .trim()
      .to_owned();
  debug!("Created container `{}`.", container_id);

  // Delete the container when this function returns.
  defer! {{
    debug!("Deleting container...");
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
      "/bin/sh",
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
