use crate::bakefile::Task;
use fs_extra::dir;
use std::{fs, path::Path, process::Command};
use tempdir::TempDir;

// Run a task and return the ID of the resulting Docker image.
pub fn run(
  task: &Task,
  from_image: &str,
  to_image: &str,
) -> Result<(), String> {
  // Create a temporary directory for the Docker build context.
  let build_context = TempDir::new("build_context").map_err(|e| {
    format!("Unable to create temporary directory. Reason: {}", e)
  })?;

  // Construct the Dockerfile.
  let dockerfile_image = format!("FROM {}\n", from_image);
  let dockerfile_paths = if task.paths.is_empty() {
    "".to_owned()
  } else {
    let sources = task.paths.iter().fold(String::new(), |acc, path| {
      format!(
        "{}\"{}\", ",
        acc,
        Path::new("files").join(path).to_string_lossy()
      )
    });
    let destination = format!(
      "{}{}",
      task.location,
      if task.location.ends_with('/') {
        ""
      } else {
        "/"
      }
    );
    format!(
      "COPY --chown={} [{}\"{}\"]\n",
      task.user, sources, destination
    )
  };
  let dockerfile_command = if task.command.is_some() {
    format!(
      "ARG COMMAND\nRUN mkdir -p '{}'; chmod 777 '{}'; su -l -c \"set -eu; cd '{}'; $COMMAND\" {}\n",
      task.location, task.location, task.location, task.user
    )
  } else {
    "".to_owned()
  };
  let dockerfile = format!(
    "{}{}{}",
    dockerfile_image, dockerfile_paths, dockerfile_command
  );
  debug!("Dockerfile:\n{}", dockerfile);

  // Write the Dockerfile to the build context.
  let dockerfile_path = build_context.path().join("Dockerfile");
  fs::write(&dockerfile_path, dockerfile).map_err(|e| {
    format!(
      "Unable to write to {}. Reason: {}",
      dockerfile_path.to_string_lossy(),
      e
    )
  })?;

  // Copy paths into the build context if applicable.
  let context_path = build_context.path().join("files");
  fs::create_dir(&context_path).map_err(|e| {
    format!(
      "Unable to create directory `{}`. Reason: {}",
      context_path.to_string_lossy(),
      e
    )
    .to_owned()
  })?;
  for path in &task.paths {
    let destination = build_context.path().join("files").join(path);
    let metadata = fs::metadata(path).map_err(|e| {
      format!("Unable to retrieve metadata for `{}`. Reason: {}", path, e)
    })?;
    if metadata.is_file() {
      fs::copy(path, &destination).map_err(|e| {
        format!(
          "Unable to copy file {} to {}. Reason: {}",
          path,
          destination.to_string_lossy(),
          e
        )
        .to_owned()
      })?;
    } else if metadata.is_dir() {
      fs::create_dir_all(&destination).map_err(|e| {
        format!(
          "Unable to create directory `{}`. Reason: {}",
          context_path.to_string_lossy(),
          e
        )
        .to_owned()
      })?;
      dir::copy(path, &destination, &dir::CopyOptions::new()).map_err(
        |e| {
          format!(
            "Unable to copy directory {} to {}. Reason: {}",
            path,
            destination.to_string_lossy(),
            e
          )
          .to_owned()
        },
      )?;
    } else {
      debug!(
        "Path `{}` ignored because it isn't a file or a directory.",
        path
      );
    }
  }

  // Build the Dockerfile to run the task.
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
  let status = cmd.status().map_err(|e| {
    format!("It appears Docker is not installed. Details: {}", e)
  })?;

  // Check the exit status.
  if status.success() {
    Ok(())
  } else {
    Err("Task failed.".to_owned())
  }
}

#[cfg(test)]
mod tests {}
