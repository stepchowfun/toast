use crate::{bakefile::Task, docker};
use std::{
  collections::{HashMap, HashSet},
  io::Read,
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
  let container =
    docker::create_container(from_image, &commands_to_run.join(" && "))?;

  // If the user interrupts the program, kill the container. The `unwrap`
  // will only fail if a panic already occurred.
  {
    active_containers.lock().unwrap().insert(container.clone());
  }

  // Delete the container when this function returns.
  defer! {{
    {
      // If the user interrupts the program, don't bother killing the
      // container. We're about to kill it here.
      active_containers.lock().unwrap().remove(&container);
    }

    if let Err(e) = docker::delete_container(&container) {
      error!("{}", e);
    }
  }};

  // Copy files into the container, if applicable.
  if !task.paths.is_empty() {
    docker::copy_into_container(&container, &mut tar).map_err(|e| {
      if running.load(Ordering::SeqCst) {
        e
      } else {
        "Interrupted.".to_owned()
      }
    })?;
  }

  // Start the container to run the command.
  if let Some(command) = &task.command {
    info!("{}", command);
  }
  docker::start_container(&container).map_err(|_| {
    if running.load(Ordering::SeqCst) {
      "Task failed."
    } else {
      "Interrupted."
    }
    .to_owned()
  })?;

  // Create an image from the container.
  docker::commit_container(&container, to_image)?;

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
