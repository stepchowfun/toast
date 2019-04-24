use crate::bakefile::Task;
use std::{collections::HashMap, path::Path, process::Command};
use tempfile::TempDir;

// Run a task and return the ID of the resulting Docker image.
pub fn run(
  task: &Task,
  from_image: &str,
  to_image: &str,
  env: &HashMap<String, String>,
) -> Result<(), String> {
  // Construct the command to run inside the container.
  let mut commands_to_run = vec![];

  // Ensure the task's location exists within the container and that the user
  // can access it.
  commands_to_run.push(format!("mkdir -p '{}'", task.location));
  commands_to_run.push(format!("chmod 777 '{}'", task.location));

  // Construct a small script to execute the task's command in the task's
  // location as the task's user with the task's environment variables.
  if let Some(command) = &task.command {
    commands_to_run.push(format!(
      "su -l -c {} {}",
      shell_escape(&format!(
        "{} set -eu; cd {}; {}",
        task
          .env
          .keys()
          .map(|var| {
            format!(
              "export {}={};",
              shell_escape(&var),
              shell_escape(&env[var]) // [ref:env_valid]
            )
          })
          .collect::<Vec<_>>()
          .join(" "),
        shell_escape(&task.location),
        command,
      )),
      shell_escape(&task.user)
    ));
  }

  // Create the container.
  debug!("Creating container from image `{}`...", from_image);
  let container_id = run_docker_quiet(
    &[
      "create",
      "--tty", // [tag:tty]
      from_image,
      "/bin/sh",
      "-c",
      &commands_to_run.join(" && "),
    ],
    "Unable to create container.",
  )?
  .trim()
  .to_owned();
  debug!("Created container `{}`.", container_id);

  // Delete the container when this function returns.
  defer! {{
    debug!("Deleting container...");
    if let Err(e) = run_docker_quiet(
      &["rm", "--force", &container_id],
      "Unable to delete container."
    ) {
      error!("{}", e);
    }
  }};

  // Create a temporary directory for creating ancestor directories in the
  // container via `docker cp`.
  let empty_dir = TempDir::new().map_err(|e| {
    format!("Unable to create temporary directory. Reason: {}", e)
  })?;

  // Copy files into the container, if applicable.
  for path in &task.paths {
    let destination = Path::new(&task.location)
      .join(path)
      .components()
      .as_path()
      .to_owned();
    let destination_str = destination.to_string_lossy().to_string();

    // Create ancestor directories in the container.
    for ancestor in destination.ancestors().collect::<Vec<_>>()[1..]
      .iter()
      .rev()
    {
      let ancestor_str = ancestor.to_string_lossy();
      debug!("Creating directory `{}` in container...", ancestor_str);
      run_docker_quiet(
        &[
          "cp",
          &empty_dir.path().join(".").to_string_lossy(),
          &format!("{}:{}", container_id, ancestor_str),
        ],
        &format!(
          "Unable to copy `{}` into container at `{}`.",
          path, destination_str
        ),
      )?;
    }

    // Copy the target into the container.
    info!(
      "Copying `{}` into container at `{}`...",
      path, destination_str
    );
    run_docker_quiet(
      &[
        "cp",
        &path,
        &format!("{}:{}", container_id, destination_str),
      ],
      &format!(
        "Unable to copy `{}` into container at `{}`.",
        path, destination_str
      ),
    )?;
  }

  // Start the container.
  debug!("Starting container...");
  if let Some(command) = &task.command {
    info!("{}", command);
  }
  run_docker_loud(&["start", "--attach", &container_id], "Task failed.")?;

  // Create an image from the container.
  debug!("Creating image...");
  run_docker_quiet(
    &["commit", &container_id, to_image],
    "Unable to create image.",
  )?;
  debug!("Created image `{}`.", to_image);

  Ok(())
}

// Query whether a Docker image exists locally.
pub fn image_exists(image: &str) -> bool {
  run_docker_quiet(
    &["inspect", "--type", "image", image],
    &format!("The image `{}` does not exist.", image),
  )
  .is_ok()
}

// Delete a Docker image.
pub fn delete_image(image: &str) -> Result<(), String> {
  run_docker_quiet(
    &["rmi", "--force", image],
    &format!("Unable to delete image `{}`.", image),
  )
  .map(|_| ())
}

// Run an interactive shell.
pub fn run_shell(image: &str) -> Result<(), String> {
  run_docker_loud(
    &["run", "--rm", "--interactive", "--tty", image, "/bin/sh"],
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

// Run a command and return its standard output or an error message.
fn run_docker_quiet(args: &[&str], error: &str) -> Result<String, String> {
  let output = docker_command(args)
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

// Run a command and forward its standard input, output, and error streams.
fn run_docker_loud(args: &[&str], error: &str) -> Result<(), String> {
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
