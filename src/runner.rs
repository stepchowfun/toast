use crate::{bakefile::Task, docker};
use std::{
  collections::{HashMap, HashSet},
  io::Read,
  path::Path,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
  },
};

// A task can be run in the context of a container or an image. The `Container`
// variant "owns" the container in the sense that the destructor deletes the
// container.
#[derive(Clone)]
pub enum Context {
  Container(
    String,                      // Container ID
    Arc<AtomicBool>,             // Whether we are still running
    Arc<Mutex<HashSet<String>>>, // Active containers
  ),
  Image(
    String, // Image name
  ),
}

impl Drop for Context {
  fn drop(&mut self) {
    if let Context::Container(container, running, active_containers) = self {
      // If the user interrupts the program, don't bother killing the
      // container. We're about to kill it here. The `unwrap` will only fail if
      // a panic already occurred.
      {
        active_containers.lock().unwrap().remove(container);
      }

      // Delete the container.
      if let Err(e) = docker::delete_container(container, running) {
        error!("{}", e);
      }
    }
  }
}

// This is a smart constructor for `Context::Container`. It adds the container
// to the active container set, and the destructor automatically removes it.
fn container_context(
  container: &str,
  running: &Arc<AtomicBool>,
  active_containers: &Arc<Mutex<HashSet<String>>>,
) -> Context {
  // If the user interrupts the program, kill the container. The `unwrap`
  // will only fail if a panic already occurred.
  {
    active_containers
      .lock()
      .unwrap()
      .insert(container.to_owned());
  }

  // Construct the context.
  Context::Container(
    container.to_owned(),
    running.to_owned(),
    active_containers.to_owned(),
  )
}

// Run a task. The context is assumed to exist locally.
#[allow(clippy::too_many_arguments)]
pub fn run<R: Read>(
  settings: &super::Settings,
  bakefile_dir: &Path,
  environment: &HashMap<String, String>,
  running: &Arc<AtomicBool>,
  active_containers: &Arc<Mutex<HashSet<String>>>,
  task: &Task,
  cache_key: &str,
  caching_enabled: bool,
  context: Context,
  mut tar: R,
) -> Result<Context, String> {
  // Check if the task is cached.
  let image = format!("{}:{}", settings.docker_repo, cache_key);
  if (settings.read_local_cache && docker::image_exists(&image, running)?)
    || (settings.read_remote_cache
      && docker::pull_image(&image, running).is_ok())
  {
    // The task is cached. Check if there are any output files.
    if task.output_paths.is_empty() {
      // There are no output files, so we're done.
      Ok(Context::Image(image))
    } else {
      // If we made it this far, we need to create a container from which we can
      // extract the output files.
      let container = docker::create_container(&image, running)?;

      // Extract the output files from the container.
      docker::copy_from_container(
        &container,
        &task.output_paths,
        &task.location,
        bakefile_dir,
        running,
      )?;

      // The container becomes the new context.
      Ok(container_context(&container, running, active_containers))
    }
  } else {
    // The task is not cached. Construct the command to run inside the container.
    let mut commands_to_run = vec![];

    // Ensure the task's location exists within the container and that the user
    // can access it.
    commands_to_run.push(format!(
      "mkdir --parents {}",
      shell_escape(&task.location.to_string_lossy())
    ));
    commands_to_run.push(format!(
      "chown --recursive --no-dereference {} {}",
      shell_escape(&task.user),
      shell_escape(&task.location.to_string_lossy())
    ));

    // Construct a small script to run the command.
    if let Some(command) = &task.command {
      // Set the working directory.
      commands_to_run.push(format!(
        "cd {}",
        shell_escape(&task.location.to_string_lossy())
      ));

      // Set the environment variables.
      for variable in task.environment.keys() {
        commands_to_run.push(format!(
          "export {}={}",
          shell_escape(variable),
          shell_escape(&environment[variable]), // [ref:environment_valid]
        ));
      }

      // Run the command as the appropriate user.
      commands_to_run.push(format!(
        "su --command={} {}",
        shell_escape(&command),
        shell_escape(&task.user)
      ));
    }

    // Create a container if needed.
    let (container, context) = match &context {
      Context::Container(container, _, _) => {
        // The context already contains a container. Use it as is.
        (container.to_owned(), context)
      }
      Context::Image(context_image) => {
        // Create a container from the image in the context.
        let container = docker::create_container(&context_image, running)?;

        // Return the container along with a new context to own it.
        (
          container.clone(),
          container_context(&container, running, active_containers),
        )
      }
    };

    // Copy files into the container, if applicable.
    if !task.input_paths.is_empty() {
      docker::copy_into_container(&container, &mut tar, running)?;
    }

    // Start the container to run the command.
    docker::start_container(
      &container,
      &commands_to_run.join(" && "),
      running,
    )
    .map_err(|_| {
      if running.load(Ordering::SeqCst) {
        "Task failed."
      } else {
        super::INTERRUPT_MESSAGE
      }
      .to_owned()
    })?;

    // Copy files from the container, if applicable.
    if !task.output_paths.is_empty() {
      docker::copy_from_container(
        &container,
        &task.output_paths,
        &task.location,
        bakefile_dir,
        running,
      )?;
    }

    // Write to cache, if applicable.
    if caching_enabled {
      if settings.write_local_cache && settings.write_remote_cache {
        // Both local and remote cache writes are enabled. Commit the container
        // to a local image and push it to the remote registry.
        docker::commit_container(&container, &image, running)?;
        docker::push_image(&image, running)?;
      } else if settings.write_local_cache && !settings.write_remote_cache {
        // Only local cache writes are enabled. Commit the container to a local
        // image.
        docker::commit_container(&container, &image, running)?;
      } else if !settings.write_local_cache && settings.write_remote_cache {
        // Only remote cache writes are enabled. Commit the container to a
        // temporary local image, push it to the remote registry, and delete
        // the local copy.
        let temp_image =
          format!("{}:{}", settings.docker_repo, docker::random_tag());
        docker::commit_container(&container, &temp_image, running)?;
        docker::push_image(&temp_image, running)?;
        docker::delete_image(&temp_image, running)?;
      }
    }

    // Return the context back to the caller.
    Ok(context)
  }
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
