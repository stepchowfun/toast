use {
    crate::{failure, failure::Failure, format::CodeStr, spinner::spin, toastfile::MappingPath},
    std::{
        collections::HashMap,
        env::current_dir,
        fs::{Metadata, copy, create_dir_all, rename, symlink_metadata},
        io,
        io::Read,
        path::Path,
        process::{ChildStdin, Command, Stdio},
        string::ToString,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    },
    tempfile::tempdir,
    typed_path::{TryAsRef, UnixPath, UnixPathBuf},
    walkdir::WalkDir,
};

#[cfg(unix)]
use std::fs::read_link;

// Query whether an image exists locally.
pub fn image_exists(
    docker_cli: &str,
    image: &str,
    interrupted: &Arc<AtomicBool>,
) -> Result<bool, Failure> {
    debug!("Checking existence of image {}\u{2026}", image.code_str());

    match run_quiet(
        docker_cli,
        "Checking existence of image\u{2026}",
        "The image doesn't exist.",
        &vec!["image", "inspect", image]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<_>>(),
        false,
        interrupted,
    ) {
        Ok(_) => Ok(true),
        Err(Failure::Interrupted) => Err(Failure::Interrupted),
        Err(Failure::System(_, _) | Failure::User(_, _)) => Ok(false),
    }
}

// Push an image.
pub fn push_image(
    docker_cli: &str,
    image: &str,
    interrupted: &Arc<AtomicBool>,
) -> Result<(), Failure> {
    debug!("Pushing image {}\u{2026}", image.code_str());

    run_quiet(
        docker_cli,
        "Pushing image\u{2026}",
        "Unable to push image.",
        &vec!["image", "push", image]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<_>>(),
        false,
        interrupted,
    )
    .map(|_| ())
}

// Pull an image.
pub fn pull_image(
    docker_cli: &str,
    image: &str,
    interrupted: &Arc<AtomicBool>,
) -> Result<(), Failure> {
    debug!("Pulling image {}\u{2026}", image.code_str());

    run_quiet(
        docker_cli,
        "Pulling image\u{2026}",
        "Unable to pull image.",
        &vec!["image", "pull", image]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<_>>(),
        false,
        interrupted,
    )
    .map(|_| ())
}

// Delete an image.
pub fn delete_image(
    docker_cli: &str,
    image: &str,
    interrupted: &Arc<AtomicBool>,
) -> Result<(), Failure> {
    debug!("Deleting image {}\u{2026}", image.code_str());

    run_quiet(
        docker_cli,
        "Deleting image\u{2026}",
        "Unable to delete image.",
        &vec!["image", "rm", "--force", image]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<_>>(),
        false,
        interrupted,
    )
    .map(|_| ())
}

// Create a container and return its ID.
#[allow(clippy::too_many_arguments)]
pub fn create_container(
    docker_cli: &str,
    image: &str,
    source_dir: &Path,
    environment: &HashMap<String, String>,
    mount_paths: &[MappingPath],
    mount_readonly: bool,
    ports: &[String],
    location: &UnixPath,
    user: &str,
    command: &str,
    extra_args: &[String],
    interrupted: &Arc<AtomicBool>,
) -> Result<String, Failure> {
    debug!("Creating container from image {}\u{2026}", image.code_str());

    let mut args = vec!["container", "create"]
        .into_iter()
        .map(std::borrow::ToOwned::to_owned)
        .collect::<Vec<_>>();

    args.extend(container_args(
        source_dir,
        environment,
        location,
        mount_paths,
        mount_readonly,
        ports,
        extra_args,
    )?);

    args.extend(
        vec![image, "/bin/su", "-c", command, user]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<_>>(),
    );

    Ok(run_quiet(
        docker_cli,
        "Creating container\u{2026}",
        "Unable to create container.",
        &args,
        false,
        interrupted,
    )?
    .trim()
    .to_owned())
}

// Copy files into a container.
pub fn copy_into_container<R: Read>(
    docker_cli: &str,
    container: &str,
    mut tar: R,
    interrupted: &Arc<AtomicBool>,
) -> Result<(), Failure> {
    debug!(
        "Copying files into container {}\u{2026}",
        container.code_str(),
    );

    run_quiet_stdin(
        docker_cli,
        "Copying files into container\u{2026}",
        "Unable to copy files into the container.",
        &[
            "container".to_owned(),
            "cp".to_owned(),
            "-".to_owned(),
            format!("{container}:/"),
        ],
        false,
        |mut stdin| {
            io::copy(&mut tar, &mut stdin)
                .map_err(failure::system("Unable to copy files into the container."))?;

            Ok(())
        },
        interrupted,
    )
    .map(|_| ())
}

// This is a helper function for the `copy_from_container` function. The `source_path` is expected
// to point to a file or symlink. This function first tries to rename the file or symlink. If that
// fails, a copy is attempted instead.
fn rename_or_copy_file_or_symlink(
    source_path: &Path,
    destination_path: &Path,
    metadata: &Metadata,
) -> Result<(), Failure> {
    // Try to rename the file or symlink.
    if rename(source_path, destination_path).is_err() {
        // The `rename` can fail if the source and the destination are not on the same mounted
        // filesystem. This occurs for example on Fedora 18+, where `/tmp` is an in-memory tmpfs
        // filesystem. If this happens, don't give up just yet. We can try to copy the file or
        // symlink instead of moving it. First, let's determine what it is.
        if metadata.file_type().is_symlink() {
            // It's a symlink. Figure out what it points to.
            #[cfg(unix)]
            let target_path = read_link(source_path).map_err(failure::system(format!(
                "Unable to read target of symbolic link {}.",
                source_path.to_string_lossy().code_str(),
            )))?;

            // Create a copy of the symlink at the destination.
            #[cfg(unix)]
            std::os::unix::fs::symlink(target_path, destination_path).map_err(failure::system(
                format!(
                    "Unable to create symbolic link at {}.",
                    destination_path.to_string_lossy().code_str(),
                ),
            ))?;

            #[cfg(windows)]
            return Err(failure::Failure::System(
                format!(
                    "Unable to create symbolic link at {}, because symlinks are not currently \
                    supported on Windows.",
                    destination_path.to_string_lossy().code_str(),
                ),
                None,
            ));
        } else {
            // It's a file. Copy it to the destination.
            copy(source_path, destination_path).map_err(failure::system(format!(
                "Unable to move or copy file {} to destination {}.",
                source_path.to_string_lossy().code_str(),
                destination_path.to_string_lossy().code_str(),
            )))?;
        }
    }

    // If we got here, the `rename` succeeded.
    Ok(())
}

// Copy files from a container.
pub fn copy_from_container(
    docker_cli: &str,
    container: &str,
    paths: &[UnixPathBuf],
    source_dir: &UnixPath,
    destination_dir: &Path,
    interrupted: &Arc<AtomicBool>,
) -> Result<(), Failure> {
    // Copy each path from the container to the host.
    for path in paths {
        debug!(
            "Copying {} from container {}\u{2026}",
            path.to_string_lossy().code_str(),
            container.code_str(),
        );

        // `docker container cp` is not idempotent. For example, suppose there is a directory called
        // `/foo` in the container and `/bar` does not exist on the host. Consider the command
        // `docker cp container:/foo /bar`. The first time that command is run, Docker will create
        // the directory `/bar` on the host and copy the files from `/foo` into it. But if you run
        // it again, Docker will copy `/foo` into the directory `/bar`, resulting in `/bar/foo`,
        // which is undesirable. To work around this, we first copy the path from the container into
        // a temporary directory (where the target path is guaranteed to not exist). Then we
        // copy/move that path to the final destination.
        let temp_dir =
            tempdir().map_err(failure::system("Unable to create temporary directory."))?;

        // Figure out what needs to go where.
        let source = source_dir.join(path);
        let intermediate = temp_dir.path().join("data");
        let destination = destination_dir.join(path.try_as_ref().ok_or_else(|| {
            Failure::User(
                format!("Invalid path {}", path.to_string_lossy().code_str()),
                None,
            )
        })?);

        // Get the path from the container.
        run_quiet(
            docker_cli,
            "Copying files from the container\u{2026}",
            "Unable to copy files from the container.",
            &[
                "container".to_owned(),
                "cp".to_owned(),
                format!("{}:{}", container, source.to_string_lossy()),
                intermediate.to_string_lossy().into_owned(),
            ],
            true,
            interrupted,
        )
        .map(|_| ())?;

        // Fetch filesystem metadata for `input_path`.
        let intermediate_metadata =
            symlink_metadata(&intermediate).map_err(failure::system(format!(
                "Unable to fetch filesystem metadata for {}.",
                intermediate.to_string_lossy().code_str(),
            )))?;

        // Determine what we got from the container.
        if intermediate_metadata.is_dir() {
            // It's a directory. Traverse it.
            for entry in WalkDir::new(&intermediate) {
                // If we run into an error traversing the filesystem, report it.
                let entry = entry.map_err(failure::system(format!(
                    "Unable to traverse directory {}.",
                    intermediate.to_string_lossy().code_str(),
                )))?;

                // Fetch the metadata for this entry.
                let entry_metadata = entry.metadata().map_err(failure::system(format!(
                    "Unable to fetch filesystem metadata for {}.",
                    entry.path().to_string_lossy().code_str(),
                )))?;

                // Figure out what needs to go where. The `unwrap` is safe because `entry` is
                // guaranteed to be inside `intermediate` (or equal to it).
                let entry_source_path = entry.path();
                let entry_destination_path =
                    destination.join(entry_source_path.strip_prefix(&intermediate).unwrap());

                // Check if the entry is a file or a directory.
                if entry.file_type().is_dir() {
                    // It's a directory. Create a directory at the destination.
                    create_dir_all(&entry_destination_path).map_err(failure::system(format!(
                        "Unable to create directory {}.",
                        entry_destination_path.to_string_lossy().code_str(),
                    )))?;
                } else {
                    // It's a file or symlink. Move or copy it to the destination.
                    rename_or_copy_file_or_symlink(
                        entry_source_path,
                        &entry_destination_path,
                        &entry_metadata,
                    )?;
                }
            }
        } else {
            // It's a file or symlink. Determine the destination directory. The `unwrap` is safe
            // because the root of the filesystem cannot be a file or symlink.
            let destination_parent = destination.parent().unwrap().to_owned();

            // Make sure the destination directory exists.
            create_dir_all(&destination_parent).map_err(failure::system(format!(
                "Unable to create directory {}.",
                destination_parent.to_string_lossy().code_str(),
            )))?;

            // Move or copy it to the destination.
            rename_or_copy_file_or_symlink(&intermediate, &destination, &intermediate_metadata)?;
        }
    }

    Ok(())
}

// Start a container.
pub fn start_container(
    docker_cli: &str,
    container: &str,
    interrupted: &Arc<AtomicBool>,
) -> Result<(), Failure> {
    debug!("Starting container {}\u{2026}", container.code_str());

    run_loud(
        docker_cli,
        "Unable to start container.",
        &vec!["container", "start", "--attach", container]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<_>>(),
        true,
        interrupted,
    )
}

// Stop a container.
pub fn stop_container(
    docker_cli: &str,
    container: &str,
    interrupted: &Arc<AtomicBool>,
) -> Result<(), Failure> {
    debug!("Stopping container {}\u{2026}", container.code_str());

    run_quiet(
        docker_cli,
        "Stopping container\u{2026}",
        "Unable to stop container.",
        &vec!["container", "stop", container]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<_>>(),
        false,
        interrupted,
    )
    .map(|_| ())
}

// Commit a container to an image.
pub fn commit_container(
    docker_cli: &str,
    container: &str,
    image: &str,
    interrupted: &Arc<AtomicBool>,
) -> Result<(), Failure> {
    debug!(
        "Committing container {} to image {}\u{2026}",
        container.code_str(),
        image.code_str(),
    );

    run_quiet(
        docker_cli,
        "Committing container\u{2026}",
        "Unable to commit container.",
        &vec!["container", "commit", container, image]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<_>>(),
        false,
        interrupted,
    )
    .map(|_| ())
}

// Delete a container.
pub fn delete_container(
    docker_cli: &str,
    container: &str,
    interrupted: &Arc<AtomicBool>,
) -> Result<(), Failure> {
    debug!("Deleting container {}\u{2026}", container.code_str());

    run_quiet(
        docker_cli,
        "Deleting container\u{2026}",
        "Unable to delete container.",
        &vec!["container", "rm", "--force", container]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<_>>(),
        false,
        interrupted,
    )
    .map(|_| ())
}

// Run an interactive shell.
#[allow(clippy::too_many_arguments)]
pub fn spawn_shell(
    docker_cli: &str,
    image: &str,
    source_dir: &Path,
    environment: &HashMap<String, String>,
    location: &UnixPath,
    mount_paths: &[MappingPath],
    mount_readonly: bool,
    ports: &[String],
    user: &str,
    extra_args: &[String],
    interrupted: &Arc<AtomicBool>,
) -> Result<(), Failure> {
    debug!(
        "Spawning an interactive shell for image {}\u{2026}",
        image.code_str(),
    );

    let mut args = vec!["container", "run", "--rm", "--interactive", "--tty"]
        .into_iter()
        .map(std::borrow::ToOwned::to_owned)
        .collect::<Vec<_>>();

    args.extend(container_args(
        source_dir,
        environment,
        location,
        mount_paths,
        mount_readonly,
        ports,
        extra_args,
    )?);

    args.extend(
        vec![image, "/bin/su", user]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect::<Vec<_>>(),
    );

    run_attach(
        docker_cli,
        "The shell exited with a failure.",
        &args,
        true,
        interrupted,
    )
}

// This function returns arguments for `docker create` or `docker run`.
fn container_args(
    source_dir: &Path,
    environment: &HashMap<String, String>,
    location: &UnixPath,
    mount_paths: &[MappingPath],
    mount_readonly: bool,
    ports: &[String],
    extra_args: &[String],
) -> Result<Vec<String>, Failure> {
    // Why `--init`? (1) PID 1 is supposed to reap orphaned zombie processes, otherwise they can
    // accumulate. Bash does this, but we run `/bin/sh` in the container, which may or may not be
    // Bash. So `--init` runs Tini (https://github.com/krallin/tini) as PID 1, which properly reaps
    // orphaned zombies. (2) PID 1 also does not exhibit the default behavior (crashing) for signals
    // like SIGINT and SIGTERM. However, PID 1 can still handle these signals by explicitly trapping
    // them. Tini traps these signals and forwards them to the child process. Then the default
    // signal handling behavior of the child process (in our case, `/bin/sh`) works normally.
    let mut args = vec!["--init".to_owned()];

    // Run as the `root` user. We always run `/bin/su` in the container, which switches to the user
    // specified in the toastfile. We want to run `/bin/su` as root so it can switch users without
    // requiring a password. Most Docker images already use `root` as the default user, but not
    // all.
    args.extend(vec!["--user".to_owned(), "root".to_owned()]);

    // Environment
    args.extend(
        environment.iter().flat_map(|(variable, value)| {
            vec!["--env".to_owned(), format!("{}={}", variable, value)]
        }),
    );

    // Location
    args.extend(vec![
        "--workdir".to_owned(),
        location.to_string_lossy().into_owned(),
    ]);

    // For bind mounts, Docker requires the host path to be absolute. We can't
    // use `std::fs::canonicalize` here, since on Windows that generates an
    // extended-length path (e.g., `\\?\C:\Users\...`) which Docker doesn't
    // understand.
    let absolute_source_dir = current_dir()
        .map_err(failure::user(
            "Unable to determine the current working directory.",
        ))?
        .join(source_dir);

    // Mount paths
    args.extend(mount_paths.iter().flat_map(|mount_path| {
        // [ref:mount_paths_no_commas]
        vec![
            "--mount".to_owned(),
            if mount_readonly {
                format!(
                    "type=bind,source={},target={},readonly",
                    absolute_source_dir
                        .join(&mount_path.host_path)
                        .to_string_lossy(),
                    location.join(&mount_path.container_path).to_string_lossy(),
                )
            } else {
                format!(
                    "type=bind,source={},target={}",
                    absolute_source_dir
                        .join(&mount_path.host_path)
                        .to_string_lossy(),
                    location.join(&mount_path.container_path).to_string_lossy(),
                )
            },
        ]
    }));

    // Ports
    args.extend(
        ports
            .iter()
            .flat_map(|port| {
                vec!["--publish", port]
                    .into_iter()
                    .map(std::borrow::ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
    );

    // User-provided arguments
    args.extend_from_slice(extra_args);

    Ok(args)
}

// Run a command and return its standard output.
fn run_quiet(
    docker_cli: &str,
    spinner_message: &str,
    error: &str,
    args: &[String],
    user_command: bool,
    interrupted: &Arc<AtomicBool>,
) -> Result<String, Failure> {
    // Render a spinner animation and clear it when we're done.
    let _guard = spin(spinner_message);

    // This is used to determine whether the user interrupted the program during the execution of
    // the child process.
    let was_interrupted = interrupted.load(Ordering::SeqCst);

    // Run the child process.
    let child = command(docker_cli, args)
        .output()
        .map_err(failure::system(format!(
            "{error} Perhaps you don't have Docker installed.",
        )))?;

    // Handle the result.
    if child.status.success() {
        Ok(String::from_utf8_lossy(&child.stdout).to_string())
    } else {
        Err(
            if child.status.code().is_none()
                || (!was_interrupted && interrupted.load(Ordering::SeqCst))
            {
                interrupted.store(true, Ordering::SeqCst);
                Failure::Interrupted
            } else if user_command {
                Failure::User(
                    format!("{}\n{}", error, String::from_utf8_lossy(&child.stderr)),
                    None,
                )
            } else {
                Failure::System(
                    format!("{}\n{}", error, String::from_utf8_lossy(&child.stderr)),
                    None,
                )
            },
        )
    }
}

// Run a command and return its standard output. Accepts a closure which receives a pipe to the
// standard input stream of the child process.
fn run_quiet_stdin<W: FnOnce(&mut ChildStdin) -> Result<(), Failure>>(
    docker_cli: &str,
    spinner_message: &str,
    error: &str,
    args: &[String],
    user_command: bool,
    writer: W,
    interrupted: &Arc<AtomicBool>,
) -> Result<String, Failure> {
    // Render a spinner animation and clear it when we're done.
    let _guard = spin(spinner_message);

    // This is used to determine whether the user interrupted the program during the execution of
    // the child process.
    let was_interrupted = interrupted.load(Ordering::SeqCst);

    // Run the child process.
    let mut child = command(docker_cli, args)
        .stdin(Stdio::piped()) // [tag:run_quiet_stdin_piped]
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(failure::system(format!(
            "{error} Perhaps you don't have Docker installed.",
        )))?;

    // Pipe data to the child's standard input stream.
    writer(child.stdin.as_mut().unwrap())?; // [ref:run_quiet_stdin_piped]

    // Wait for the child to terminate.
    let output = child.wait_with_output().map_err(failure::system(format!(
        "{error} Perhaps you don't have Docker installed.",
    )))?;

    // Handle the result.
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(
            if output.status.code().is_none()
                || (!was_interrupted && interrupted.load(Ordering::SeqCst))
            {
                interrupted.store(true, Ordering::SeqCst);
                Failure::Interrupted
            } else if user_command {
                Failure::User(
                    format!("{}\n{}", error, String::from_utf8_lossy(&output.stderr)),
                    None,
                )
            } else {
                Failure::System(
                    format!("{}\n{}", error, String::from_utf8_lossy(&output.stderr)),
                    None,
                )
            },
        )
    }
}

// Run a command and inherit standard output and error streams.
fn run_loud(
    docker_cli: &str,
    error: &str,
    args: &[String],
    user_command: bool,
    interrupted: &Arc<AtomicBool>,
) -> Result<(), Failure> {
    // This is used to determine whether the user interrupted the program during the execution of
    // the child process.
    let was_interrupted = interrupted.load(Ordering::SeqCst);

    // Run the child process.
    let mut child = command(docker_cli, args)
        .stdin(Stdio::null())
        .spawn()
        .map_err(failure::system(format!(
            "{error} Perhaps you don't have Docker installed.",
        )))?;

    // Wait for the child to terminate.
    let status = child.wait().map_err(failure::system(format!(
        "{error} Perhaps you don't have Docker installed.",
    )))?;

    // Handle the result.
    if status.success() {
        Ok(())
    } else {
        Err(
            if status.code().is_none() || (!was_interrupted && interrupted.load(Ordering::SeqCst)) {
                interrupted.store(true, Ordering::SeqCst);
                Failure::Interrupted
            } else if user_command {
                Failure::User(error.to_owned(), None)
            } else {
                Failure::System(error.to_owned(), None)
            },
        )
    }
}

// Run a command and inherit standard input, output, and error streams.
fn run_attach(
    docker_cli: &str,
    error: &str,
    args: &[String],
    user_command: bool,
    interrupted: &Arc<AtomicBool>,
) -> Result<(), Failure> {
    // This is used to determine whether the user interrupted the program during the execution of
    // the child process.
    let was_interrupted = interrupted.load(Ordering::SeqCst);

    // Run the child process.
    let child = command(docker_cli, args)
        .status()
        .map_err(failure::system(format!(
            "{error} Perhaps you don't have Docker installed.",
        )))?;

    // Handle the result.
    if child.success() {
        Ok(())
    } else {
        Err(
            if child.code().is_none() || (!was_interrupted && interrupted.load(Ordering::SeqCst)) {
                interrupted.store(true, Ordering::SeqCst);
                Failure::Interrupted
            } else if user_command {
                Failure::User(error.to_owned(), None)
            } else {
                Failure::System(error.to_owned(), None)
            },
        )
    }
}

// Construct a Docker `Command` from an array of arguments.
fn command(docker_cli: &str, args: &[String]) -> Command {
    let mut command = Command::new(docker_cli);
    for arg in args {
        command.arg(arg);
    }
    command
}
