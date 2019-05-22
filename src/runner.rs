use crate::{cache, docker, tar, toastfile::Task};
use notify::{watcher, RecursiveMode, Watcher};
use std::{
  collections::{HashMap, HashSet},
  io::{Seek, SeekFrom},
  path::PathBuf,
  sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::channel,
    Arc, Mutex,
  },
  thread,
  time::Duration,
};
use tempfile::tempfile;

// A task can be run in the context of a container or an image. The `Container`
// variant "owns" the container in the sense that the destructor deletes the
// container.
#[derive(Clone)]
pub enum Context {
  Container(
    String,                      // Container ID
    Vec<String>,                 // Ports
    Arc<AtomicBool>,             // Whether the schedule has been interrupted
    Arc<Mutex<HashSet<String>>>, // Active containers
  ),
  Image(
    String, // Image name
  ),
}

impl Drop for Context {
  fn drop(&mut self) {
    if let Context::Container(container, _, interrupted, active_containers) =
      self
    {
      // If the user interrupts the program, don't bother killing the
      // container. We're about to kill it here. The `unwrap` will only fail if
      // a panic already occurred.
      {
        active_containers.lock().unwrap().remove(container);
      }

      // Delete the container.
      if let Err(e) = docker::delete_container(container, interrupted) {
        error!("{}", e);
      }
    }
  }
}

// This is a smart constructor for `Context::Container`. It adds the container
// to the active container set, and the destructor automatically removes it.
fn container_context(
  container: &str,
  ports: &[String],
  interrupted: &Arc<AtomicBool>,
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
    ports.to_owned(),
    interrupted.to_owned(),
    active_containers.to_owned(),
  )
}

// Run a task and return the new cache key and context.
#[allow(clippy::too_many_arguments)]
pub fn run(
  settings: &super::Settings,
  environment: &HashMap<String, String>,
  interrupted: &Arc<AtomicBool>,
  active_containers: &Arc<Mutex<HashSet<String>>>,
  task: &Task,
  previous_cache_key: &str,
  caching_enabled: bool,
  context: Context,
) -> Result<(String, Context), (String, Context)> {
  // All relative paths are relative to where the toastfile lives.
  let mut toastfile_dir = PathBuf::from(&settings.toastfile_path);
  toastfile_dir.pop();

  // Create a temporary archive for the input file contents.
  let tar_file = match tempfile() {
    Ok(tar_file) => tar_file,
    Err(e) => {
      return Err((
        format!("Unable to create temporary file. Details: {}.", e),
        context,
      ))
    }
  };

  // Write to the archive.
  let (mut tar_file, input_files_hash) = match tar::create(
    "Reading files\u{2026}",
    tar_file,
    &task.input_paths,
    &toastfile_dir,
    &task.location,
    &interrupted,
  ) {
    Ok((tar_file, input_files_hash)) => (tar_file, input_files_hash),
    Err(e) => return Err((e, context)),
  };

  // Seek back to the beginning of the archive to prepare for copying it into
  // the container.
  if let Err(e) = tar_file.seek(SeekFrom::Start(0)) {
    return Err((
      format!("Unable to seek temporary file. Details: {}.", e),
      context,
    ));
  };

  // Compute the cache key.
  let cache_key =
    cache::key(previous_cache_key, &task, &input_files_hash, &environment);

  // This is the image we'll look for in the caches.
  let image = format!("{}:{}", settings.docker_repo, cache_key);

  // Check the cache, if applicable.
  let mut cached = false;
  if caching_enabled {
    // Check the local cache.
    cached = settings.read_local_cache
      && match docker::image_exists(&image, interrupted) {
        Ok(exists) => exists,
        Err(e) => return Err((e, context)),
      };

    // Check the remote cache.
    if !cached && settings.read_remote_cache {
      if let Err(e) = docker::pull_image(&image, interrupted) {
        // If the pull failed, it could be because the user killed the child
        // process (e.g., by hitting CTRL+C).
        if interrupted.load(Ordering::SeqCst) {
          return Err((e, context));
        }
      } else {
        cached = true;
      }
    }
  }

  // If the task is cached, extract the output files if applicable.
  if cached {
    // The task is cached. Check if there are any output files.
    if task.output_paths.is_empty() {
      // There are no output files, so we're done.
      Ok((cache_key, Context::Image(image)))
    } else {
      // If we made it this far, we need to create a container from which we can
      // extract the output files.
      let container =
        match docker::create_container(&image, &task.ports, interrupted) {
          Ok(container) => container,
          Err(e) => return Err((e, context)),
        };
      let context = container_context(
        &container,
        &task.ports,
        interrupted,
        active_containers,
      );

      // Extract the output files from the container.
      if let Err(e) = docker::copy_from_container(
        &container,
        &task.output_paths,
        &task.location,
        &toastfile_dir,
        interrupted,
      ) {
        return Err((e, context));
      }

      // The container becomes the new context.
      Ok((cache_key, context))
    }
  } else {
    // The task is not cached. Construct the command to run inside the container.
    let mut commands_to_run = vec![];

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

      // Run the command as the appropriate user. For readability, we prefer to
      // use the long forms of command-line options. However, we have to use
      // `-c COMMAND` rather than `--command=COMMAND` because BusyBox's `su`
      // utility doesn't support the latter form, and we want to support
      // BusyBox.
      commands_to_run.push(format!(
        "su -c {} {}",
        shell_escape(&command),
        shell_escape(&task.user)
      ));
    }

    // Create a container if needed.
    let (container, context) = match &context {
      Context::Container(container, ports, _, _) => {
        // The context already contains a container. Check if it has the right
        // ports exposed.
        if *ports == task.ports {
          // The ports are correct. Use the container as is.
          (container.to_owned(), context)
        } else {
          // We need to create a new container to expose the correct ports.
          // First, commit the existing container to a temporary image.
          let temp_image =
            format!("{}:{}", settings.docker_repo, docker::random_tag());
          if let Err(e) =
            docker::commit_container(&container, &temp_image, interrupted)
          {
            return Err((e, context));
          }

          // Now create a new container based on that temporary image with the
          // correct ports exposed.
          let container =
            docker::create_container(&temp_image, &task.ports, interrupted)
              .map_err(|e| (e, context))?;
          (
            container.clone(),
            container_context(
              &container,
              &task.ports,
              interrupted,
              active_containers,
            ),
          )
        }
      }
      Context::Image(context_image) => {
        // If the context is an image, pull it if necessary. Note that this is
        // not considered reading from the remote cache.
        if !match docker::image_exists(&context_image, interrupted) {
          Ok(exists) => exists,
          Err(e) => return Err((e, context)),
        } {
          if let Err(e) = docker::pull_image(&context_image, interrupted) {
            return Err((e, context));
          }
        }

        // Create a container from the image.
        let container =
          docker::create_container(&context_image, &task.ports, interrupted)
            .map_err(|e| (e, context))?;

        // Return the container along with a new context to own it.
        (
          container.clone(),
          container_context(
            &container,
            &task.ports,
            interrupted,
            active_containers,
          ),
        )
      }
    };

    // Copy files into the container. If `task.input_paths` is empty, then this
    // will just create a directory for `task.location`.
    if let Err(e) =
      docker::copy_into_container(&container, &mut tar_file, interrupted)
    {
      return Err((e, context));
    }

    // Create a channel for publishing and listening for filesystem events.
    let (notify_sender, notify_receiver) = channel();

    // Set up a filesystem watcher.
    let mut watcher = match watcher(notify_sender, Duration::from_millis(200))
      .map_err(|e| {
        format!("Unable to initialize filesystem watcher. Details: {}.", e)
      }) {
      Ok(watcher) => watcher,
      Err(e) => {
        return Err((e, context));
      }
    };

    // If applicable, subscribe to filesystem events.
    if task.watch {
      // We'll create a thread to process the events. First, we need to clone
      // these values so they can be owned by the thread.
      let toastfile_dir_clone = toastfile_dir.clone();
      let container_clone = container.clone();
      let interrupted_clone = interrupted.clone();
      let task_clone = task.clone();

      // Spawn the thread.
      thread::spawn(move || {
        // Wait for events.
        while let Ok(_) = notify_receiver.recv() {
          // Create a temporary archive for the input file contents.
          let tar_file = match tempfile() {
            Ok(tar_file) => tar_file,
            Err(e) => {
              error!("Unable to create temporary file. Details: {}.", e);
              break;
            }
          };

          // Write to the archive.
          let (mut tar_file, _) = match tar::create(
            "Reading files\u{2026}",
            tar_file,
            &task_clone.input_paths,
            &toastfile_dir_clone,
            &task_clone.location,
            &interrupted_clone,
          ) {
            Ok((tar_file, input_files_hash)) => (tar_file, input_files_hash),
            Err(e) => {
              error!("{}", e);
              break;
            }
          };

          // Seek back to the beginning of the archive to prepare for copying
          // it into the container.
          if let Err(e) = tar_file.seek(SeekFrom::Start(0)) {
            error!("Unable to seek temporary file. Details: {}.", e);
            break;
          };

          // Copy files into the container. If `task.input_paths` is empty,
          // then this will just create a directory for `task.location`.
          if let Err(e) = docker::copy_into_container(
            &container_clone,
            &mut tar_file,
            &interrupted_clone,
          ) {
            error!("{}", e);
            break;
          }

          // Inform the user that filesystem watching is working.
          info!("Files synced.");
        }
      });

      // Add the `input_paths` from this task to the filesystem watcher.
      for path in &task.input_paths {
        if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
          return Err((
            format!(
              "Unable to register a filesystem watch path. Details: {}.",
              e
            ),
            context,
          ));
        }
      }
    }

    // Start the container to run the command.
    if docker::start_container(
      &container,
      &commands_to_run.join(" && "),
      interrupted,
    )
    .is_err()
    {
      return Err((
        if interrupted.load(Ordering::SeqCst) {
          super::INTERRUPT_MESSAGE
        } else {
          "Command failed."
        }
        .to_owned(),
        context,
      ));
    }

    // Copy files from the container, if applicable.
    if !task.output_paths.is_empty() {
      if let Err(e) = docker::copy_from_container(
        &container,
        &task.output_paths,
        &task.location,
        &toastfile_dir,
        interrupted,
      ) {
        return Err((e, context));
      }
    }

    // Write to cache, if applicable.
    if caching_enabled {
      if settings.write_local_cache && settings.write_remote_cache {
        // Both local and remote cache writes are enabled. Commit the container
        // to a local image and push it to the remote registry.
        if let Err(e) =
          docker::commit_container(&container, &image, interrupted)
        {
          return Err((e, context));
        }
        if let Err(e) = docker::push_image(&image, interrupted) {
          return Err((e, context));
        }
      } else if settings.write_local_cache && !settings.write_remote_cache {
        // Only local cache writes are enabled. Commit the container to a local
        // image.
        if let Err(e) =
          docker::commit_container(&container, &image, interrupted)
        {
          return Err((e, context));
        }
      } else if !settings.write_local_cache && settings.write_remote_cache {
        // Only remote cache writes are enabled. Commit the container to a
        // temporary local image, push it to the remote registry, and delete
        // the local copy.
        let temp_image =
          format!("{}:{}", settings.docker_repo, docker::random_tag());
        defer! {{
          if let Err(e) = docker::delete_image(&temp_image, interrupted) {
            error!("{}", e);
          }
        }}

        if let Err(e) =
          docker::commit_container(&container, &temp_image, interrupted)
        {
          return Err((e, context));
        }

        if let Err(e) = docker::push_image(&temp_image, interrupted) {
          return Err((e, context));
        }
      }
    }

    // Return the context back to the caller.
    Ok((cache_key, context))
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
