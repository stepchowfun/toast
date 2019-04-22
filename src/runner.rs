use crate::bakefile::Task;
use std::{fs, process::Command};
use tempdir::TempDir;

// Run a task and return the ID of the resulting Docker image.
pub fn run(
  task: &Task,
  from_image: &str,
  to_image: &str,
) -> Result<(), String> {
  let dockerfile_from: String = format!("FROM {}\n", from_image);
  let dockerfile_command = if task.command.is_some() {
    "ARG COMMAND\nRUN sh -c \"$COMMAND\"\n"
  } else {
    ""
  };
  let dockerfile = format!("{}{}", dockerfile_from, dockerfile_command);
  debug!("Dockerfile:\n{}", dockerfile);

  // Create a temporary directory for the Docker build context.
  let build_context = TempDir::new("build_context").map_err(|e| {
    format!("Unable to create temporary directory. Reason: {}", e)
  })?;

  // Write the Dockerfile to the build context.
  let dockerfile_path = build_context.path().join("Dockerfile");
  fs::write(&dockerfile_path, dockerfile).map_err(|e| {
    format!(
      "Unable to write to {}. Reason: {}",
      dockerfile_path.to_string_lossy(),
      e
    )
  })?;

  // Run the Dockerfile.
  let mut cmd = Command::new("docker");
  cmd
    .arg("build")
    .arg(build_context.path())
    .arg("--tag")
    .arg(to_image);
  if let Some(command) = &task.command {
    cmd
      .env("COMMAND", command)
      .arg("--build-arg")
      .arg("COMMAND");
  }
  let status = cmd
    .status()
    .map_err(|e| format!("Docker is not installed. Error: {}", e))?;

  // Check the exit status.
  if status.success() {
    Ok(())
  } else {
    Err("Task failed.".to_owned())
  }
}

#[cfg(test)]
mod tests {}
