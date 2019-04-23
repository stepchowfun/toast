use crate::bakefile::Task;
use std::{collections::HashMap, path::Path, process::Command};
use tempfile::TempDir;

// Run a task and return the ID of the resulting Docker image.
pub fn run(
  task: &Task,
  from_image: &str,
  to_image: &str,
  args: &HashMap<String, String>,
) -> Result<(), String> {
  // Construct the command to run inside the container.
  let mut commands_to_run = vec![];

  commands_to_run.push(format!("mkdir -p '{}'", task.location));
  commands_to_run.push(format!("chmod 777 '{}'", task.location));

  if let Some(command) = &task.command {
    commands_to_run.push(format!(
      "su -l -c {} {}",
      shell_escape(&format!(
        "{} set -eu; cd {}; {}",
        task
          .args
          .keys()
          .map(|arg| {
            format!(
              "export {}={};",
              shell_escape(&arg),
              shell_escape(&args[arg]) // [ref:args_valid]
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

  let command = commands_to_run.join(" && ");

  // Create the container.
  debug!("Creating container from image `{}`...", from_image);
  let mut create_command = Command::new("docker");
  create_command.arg("create");
  create_command.arg("--tty"); // [tag:tty]
  create_command.arg(from_image);
  create_command.arg("/bin/sh");
  create_command.arg("-c");
  create_command.arg(command);
  let container_id =
    run_command_quiet(create_command, "Unable to create container.")?
      .trim()
      .to_owned();
  debug!("Created container `{}`.", container_id);

  // Delete the container when this function returns.
  defer! {{
    debug!("Deleting container...");
    let mut delete_command = Command::new("docker");
    delete_command.arg("rm");
    delete_command.arg("--force");
    delete_command.arg(&container_id);
    if let Err(e) = run_command_quiet(
      delete_command,
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
      let mut copy_command = Command::new("docker");
      copy_command.arg("cp");
      copy_command.arg(empty_dir.path().join("."));
      copy_command.arg(format!("{}:{}", container_id, ancestor_str));
      run_command_quiet(
        copy_command,
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
    let mut copy_command = Command::new("docker");
    copy_command.arg("cp");
    copy_command.arg(path);
    copy_command.arg(format!("{}:{}", container_id, destination_str));
    run_command_quiet(
      copy_command,
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

  let mut start_command = Command::new("docker");
  start_command
    .arg("start")
    .arg("--attach")
    .arg(&container_id);
  run_command_loud(start_command, "Task failed.")?;

  // Create an image from the container.
  debug!("Creating image...");
  let mut commit_command = Command::new("docker");
  commit_command.arg("commit");
  commit_command.arg(&container_id);
  commit_command.arg(to_image);
  run_command_quiet(commit_command, "Unable to create image.")?;
  debug!("Created image `{}`.", to_image);

  Ok(())
}

// Query whether a Docker image exists locally.
pub fn image_exists(image: &str) -> bool {
  let mut inspect_command = Command::new("docker");
  inspect_command.arg("inspect");
  inspect_command.arg("--type");
  inspect_command.arg("image");
  inspect_command.arg(image);
  run_command_quiet(inspect_command, "The image does not exist.").is_ok()
}

// Delete a Docker image.
pub fn delete_image(image: &str) -> Result<(), String> {
  let mut inspect_command = Command::new("docker");
  inspect_command.arg("rmi");
  inspect_command.arg("--force");
  inspect_command.arg(image);
  run_command_quiet(inspect_command, "Unable to delete image.").map(|_| ())
}

// Run a command and return its standard output or an error message.
fn run_command_quiet(
  mut command: Command,
  error: &str,
) -> Result<String, String> {
  let output = command
    .output()
    .map_err(|e| format!("{} Details: {}", error, e))?;
  if !output.status.success() {
    return Err(format!(
      "{} Details: {}",
      error,
      String::from_utf8_lossy(&output.stderr)
    ));
  }
  Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// Run a command and forward its standard input and output.
fn run_command_loud(mut command: Command, error: &str) -> Result<(), String> {
  let status = command
    .status()
    .map_err(|e| format!("{} Details: {}", error, e))?;
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
