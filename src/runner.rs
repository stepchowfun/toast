use crate::{cache, docker, failure, failure::Failure, tar, toastfile::Task};
use std::{
    collections::{HashMap, HashSet},
    io::{Seek, SeekFrom},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tempfile::tempfile;

// A context is an image that may need to be cleaned up.
#[derive(Clone)]
pub struct Context {
    pub image: String,
    pub persist: bool,
    pub interrupted: Arc<AtomicBool>,
}

impl Drop for Context {
    fn drop(&mut self) {
        // Delete the image if needed.
        if !self.persist {
            if let Err(e) = docker::delete_image(&self.image, &self.interrupted) {
                error!("{}", e);
            }
        }
    }
}

// Run a task in a given context and return a new context. The returned context should not be `None`
// if `need_context` is `true`.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
pub fn run(
    settings: &super::Settings,
    environment: &HashMap<String, String>,
    interrupted: &Arc<AtomicBool>,
    active_containers: &Arc<Mutex<HashSet<String>>>,
    task: &Task,
    caching_enabled: bool,
    context: Context,
    need_context: bool,
) -> (Result<(), Failure>, Option<Context>) {
    // All relative paths are relative to where the toastfile lives.
    let mut toastfile_dir = PathBuf::from(&settings.toastfile_path);
    toastfile_dir.pop();

    // Create a temporary archive for the input file contents.
    let tar_file = match tempfile() {
        Ok(tar_file) => tar_file,
        Err(e) => {
            return (
                Err(failure::system("Unable to create temporary file.")(e)),
                Some(context),
            )
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
        Err(e) => return (Err(e), Some(context)),
    };

    // Seek back to the beginning of the archive to prepare for copying it into the container.
    if let Err(e) = tar_file.seek(SeekFrom::Start(0)) {
        return (
            Err(failure::system("Unable to seek temporary file.")(e)),
            Some(context),
        );
    };

    // Compute the name of the image that this task produces.
    let image = cache::image_name(
        &context.image,
        &settings.docker_repo,
        &task,
        &input_files_hash,
        &environment,
    );

    // Construct the environment.
    let mut task_environment = HashMap::<String, String>::new();
    for variable in task.environment.keys() {
        // [ref:environment_valid]
        task_environment.insert(variable.clone(), environment[variable].clone());
    }

    // Check the cache, if applicable.
    let mut cached = false;
    if caching_enabled {
        // Check the local cache.
        cached = settings.read_local_cache
            && match docker::image_exists(&image, interrupted) {
                Ok(exists) => exists,
                Err(e) => return (Err(e), Some(context)),
            };

        // Check the remote cache.
        if !cached && settings.read_remote_cache {
            if let Err(e) = docker::pull_image(&image, interrupted) {
                // If the pull failed, it could be because the user killed the child process (e.g.,
                // by hitting CTRL+C).
                if interrupted.load(Ordering::SeqCst) {
                    return (Err(e), Some(context));
                }
            } else {
                cached = true;
            }
        }
    }

    // If the task is cached, extract the output files if applicable.
    if cached {
        // The task is cached. Check if there are any output files.
        if !task.output_paths.is_empty() {
            // We need to create a container from which we can extract the output files.
            let container = match docker::create_container(
                &image,
                &toastfile_dir,
                &task_environment,
                &task.mount_paths,
                task.mount_readonly,
                &task.ports,
                &task.location,
                &task.user,
                &task.command,
                interrupted,
            ) {
                Ok(container) => container,
                Err(e) => return (Err(e), Some(context)),
            };

            // Delete the container when we're done.
            defer! {{
              if let Err(e) = docker::delete_container(&container, interrupted) {
                error!("{}", e);
              }
            }}

            // Extract the output files from the container.
            if let Err(e) = docker::copy_from_container(
                &container,
                &task.output_paths,
                &task.location,
                &toastfile_dir,
                interrupted,
            ) {
                return (Err(e), Some(context));
            }
        }

        // The cached image becomes the new context.
        (
            Ok(()),
            Some(Context {
                image,
                persist: true,
                interrupted: interrupted.clone(),
            }),
        )
    } else {
        // Pull the image if necessary. Note that this is not considered reading from the remote
        // cache.
        if !match docker::image_exists(&context.image, interrupted) {
            Ok(exists) => exists,
            Err(e) => return (Err(e), Some(context)),
        } {
            if let Err(e) = docker::pull_image(&context.image, interrupted) {
                return (Err(e), Some(context));
            }
        }

        // Create a container from the image.
        let container = match docker::create_container(
            &context.image,
            &toastfile_dir,
            &task_environment,
            &task.mount_paths,
            task.mount_readonly,
            &task.ports,
            &task.location,
            &task.user,
            &task.command,
            interrupted,
        ) {
            Ok(container) => container,
            Err(e) => return (Err(e), Some(context)),
        };

        // If the user interrupts the program, kill the container. The `unwrap` will only fail if a
        // panic already occurred.
        {
            active_containers.lock().unwrap().insert(container.clone());
        }

        // Delete the container when we're done.
        defer! {{
          // If the user interrupts the program, don't bother killing the container. We're about to
          // kill it here. The `unwrap` will only fail if a panic already occurred.
          {
            active_containers.lock().unwrap().remove(&container);
          }

          // Delete the container.
          if let Err(e) = docker::delete_container(&container, interrupted) {
            error!("{}", e);
          }
        }}

        // Copy files into the container. If `task.input_paths` is empty, then this will just create
        // a directory for `task.location`.
        if let Err(e) = docker::copy_into_container(&container, &mut tar_file, interrupted) {
            return (Err(e), Some(context));
        }

        // Start the container to run the command.
        let result = docker::start_container(&container, interrupted).map_err(|e| match e {
            Failure::Interrupted => e,
            Failure::System(_, _) | Failure::User(_, _) => {
                Failure::User("Command failed.".to_owned(), None)
            }
        });

        // Copy files from the container, if applicable.
        match result {
            Ok(_) if !task.output_paths.is_empty() => {
                if let Err(e) = docker::copy_from_container(
                    &container,
                    &task.output_paths,
                    &task.location,
                    &toastfile_dir,
                    interrupted,
                ) {
                    return (Err(e), Some(context));
                }
            }
            Err(_) if !task.output_paths_on_failure.is_empty() => {
                if let Err(e) = docker::copy_from_container(
                    &container,
                    &task.output_paths_on_failure,
                    &task.location,
                    &toastfile_dir,
                    interrupted,
                ) {
                    return (Err(e), Some(context));
                }
            }
            _ => {}
        }

        // Decide whether to the image needs to be persisted.
        let cacheable = result.is_ok() && caching_enabled;
        let persist_locally = cacheable && settings.write_local_cache;
        let persist_remotely = cacheable && settings.write_remote_cache;

        // Only commit the container if we actually need to return a context.
        if need_context || persist_locally || persist_remotely {
            // Commit the container.
            if let Err(e) = docker::commit_container(&container, &image, interrupted) {
                return (Err(e), Some(context));
            }

            // Construct the new context.
            let new_context = Context {
                image,
                persist: persist_locally,
                interrupted: interrupted.clone(),
            };

            // Write to remote cache, if applicable.
            if persist_remotely {
                if let Err(e) = docker::push_image(&new_context.image, interrupted) {
                    return (Err(e), Some(new_context));
                }
            }

            // Return the new context.
            (result.map(|_| ()), Some(new_context))
        } else {
            // The caller doesn't need a context to be returned.
            (result.map(|_| ()), None)
        }
    }
}
