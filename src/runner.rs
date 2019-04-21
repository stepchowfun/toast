use crate::bakefile::Task;
use std::{fs, process::Command};
use tempdir::TempDir;

// Run a task and return the ID of the resulting Docker image.
pub fn run(
  task: &Task,
  from_image: &str,
  to_image: &str,
) -> Result<(), String> {
  let dockerfile_from: String = format!("FROM {}", from_image);
  let dockerfile_run = task.command.clone().map_or_else(
    || "".to_owned(),
    |command| {
      escape_dockerfile(&format!("RUN sh -c {}", escape_shell(&command)))
    },
  );
  let dockerfile = format!("{}\n{}", dockerfile_from, dockerfile_run);
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
  let status = Command::new("docker")
    .arg("build")
    .arg(build_context.path())
    .arg("--tag")
    .arg(to_image)
    .status()
    .map_err(|_| "Docker is not installed.".to_owned())?;

  // Check the exit status.
  if status.success() {
    Ok(())
  } else {
    Err("Task failed.".to_owned())
  }
}

// Escape a string for shell interpolation.
fn escape_shell(command: &str) -> String {
  format!("'{}'", command.replace("'", "'\\''"))
}

// Escape a string for Dockerfile interpolation.
fn escape_dockerfile(command: &str) -> String {
  command.replace("\n", "\\\n")
}

#[cfg(test)]
mod tests {
  use crate::runner::{escape_dockerfile, escape_shell};

  #[test]
  fn escape_shell_empty() {
    assert_eq!(escape_shell(""), "''");
  }

  #[test]
  fn escape_shell_word() {
    assert_eq!(escape_shell("foo"), "'foo'");
  }

  #[test]
  fn escape_shell_single_quote() {
    assert_eq!(escape_shell("f'o'o"), "'f'\\''o'\\''o'");
  }

  #[test]
  fn escape_dockerfile_empty() {
    assert_eq!(escape_dockerfile(""), "");
  }

  #[test]
  fn escape_dockerfile_word() {
    assert_eq!(escape_dockerfile("foo"), "foo");
  }

  #[test]
  fn escape_dockerfile_newline() {
    assert_eq!(escape_dockerfile("f\no\no"), "f\\\no\\\no");
  }
}
