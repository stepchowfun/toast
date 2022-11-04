use {
    crate::{cache, cache::CryptoHash, failure, failure::Failure, format::CodeStr, spinner::spin},
    std::{
        collections::HashSet,
        fs::{read_link, symlink_metadata, File, Metadata},
        io::{empty, Read, Seek, SeekFrom, Write},
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
    },
    tar::{Builder, EntryType, Header},
    walkdir::WalkDir,
};

// To help keep track of the various types of paths, we will adopt the following variable suffixes:
//
// - In the container filesystem:
//   - `_acr`: Absolute (e.g., `/scratch/foo.txt`)
//   - `_rcr`: Relative to the root (e.g., `scratch/foo.txt`)
// - In the host filesystem:
//   - `_cd`: Absolute or relative to the current working directory (e.g., `../foo.txt`)
//   - `_rsd`: Relative to the source directory (e.g., `foo.txt`)

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
fn is_file_executable(metadata: &Metadata) -> bool {
    // Determine if the file has the executable bit set.
    let mode = metadata.permissions().mode();
    mode & 0o1 > 0 || mode & 0o10 > 0 || mode & 0o100 > 0
}

#[cfg(windows)]
fn is_file_executable(_metadata: &Metadata) -> bool {
    // Every file on Windows is executable.
    true
}

// Tar archives must contain only relative paths. For our purposes, the paths will be relative to
// the filesystem root, so we need to strip the leading `/` before adding paths to the archive.
fn strip_root_rcr(path_acr: &Path) -> &Path {
    // The `unwrap` is safe since `absolute_path` is absolute and Toast only supports Unix.
    path_acr.strip_prefix("/").unwrap()
}

// Check if a file is denied by `excluded_input_paths`.
fn path_excluded(excluded_input_paths_rcr: &[PathBuf], path_rcr: &Path) -> bool {
    // Don't add this path if it's denied by `excluded_input_paths`.
    for excluded_input_path_rcr in excluded_input_paths_rcr {
        if path_rcr.starts_with(excluded_input_path_rcr) {
            return true;
        }
    }

    false
}

// Check if a path can be added to the archive. This function also adds the path to `visited_paths`.
fn can_add_path(
    visited_paths_rcr: &mut HashSet<PathBuf>,
    excluded_input_paths_rcr: &[PathBuf],
    path_rcr: &Path,
) -> bool {
    // Don't add this path multiple times.
    if !visited_paths_rcr.insert(path_rcr.to_owned()) {
        return false;
    }

    // Don't add this path if it's denied by `excluded_input_paths`.
    if path_excluded(excluded_input_paths_rcr, path_rcr) {
        return false;
    }

    true
}

// Add a file to a tar archive.
fn add_file<R: Read, W: Write>(
    builder: &mut Builder<W>,
    path_rcr: &Path,
    data: R,
    size: u64,
    executable: bool,
) -> Result<(), Failure> {
    // Construct a tar header for this entry.
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_mode(if executable { 0o777 } else { 0o666 });
    header.set_size(size);

    // Add the entry to the archive.
    builder
        .append_data(&mut header, path_rcr, data)
        .map_err(failure::system("Error appending data to tar archive."))?;

    // Everything succeeded.
    Ok(())
}

// Add a symlink to a tar archive.
fn add_symlink<W: Write>(
    builder: &mut Builder<W>,
    path_rcr: &Path,
    target: &Path,
) -> Result<(), Failure> {
    // Construct a tar header for this entry.
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Symlink);
    header.set_link_name(target).map_err(failure::system(
        "Error appending symbolic link to tar archive.",
    ))?;
    header.set_mode(0o777);
    header.set_size(0);

    // Add the entry to the archive.
    builder
        .append_data(&mut header, path_rcr, empty())
        .map_err(failure::system("Error appending data to tar archive."))?;

    // Everything succeeded.
    Ok(())
}

// Add a directory to a tar archive.
fn add_directory<W: Write>(builder: &mut Builder<W>, path_rcr: &Path) -> Result<(), Failure> {
    // If the path has no components, there's nothing to do. The root directory will already exist.
    // Without this check, we could encounter the following error: `paths in archives must have at
    // least one component when setting path for`.
    if path_rcr.components().next().is_none() {
        return Ok(());
    }

    // Construct a tar header for this entry.
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Directory);
    header.set_mode(0o777);
    header.set_size(0);

    // Add the entry to the archive.
    builder
        .append_data(&mut header, path_rcr, empty())
        .map_err(failure::system("Error appending data to tar archive."))?;

    // Everything succeeded.
    Ok(())
}

// Add a file, symlink, or directory to a tar archive.
fn add_path<W: Write>(
    builder: &mut Builder<W>,
    content_hashes: &mut Vec<String>,
    visited_paths_rcr: &mut HashSet<PathBuf>,
    excluded_input_paths_rcr: &[PathBuf],
    path_cd: &Path,
    path_rcr: &Path,
    metadata: &Metadata,
) -> Result<(), Failure> {
    // Check if this path should be added.
    if !can_add_path(visited_paths_rcr, excluded_input_paths_rcr, path_rcr) {
        return Ok(());
    }

    // Add the ancestor directories. They would be created automatically, but we add them explicitly
    // here to ensure they have the right permissions.
    if let Some(parent_rcr) = path_rcr.parent() {
        for ancestor_rcr in parent_rcr.ancestors() {
            if can_add_path(visited_paths_rcr, excluded_input_paths_rcr, ancestor_rcr) {
                add_directory(builder, ancestor_rcr)?;
            }
        }
    }

    // Check the type of the entry.
    if metadata.file_type().is_file() {
        let executable = is_file_executable(metadata);

        // It's a file. Open it so we can compute the hash of its contents and add it to the
        // archive.
        let mut file = File::open(path_cd).map_err(failure::system(format!(
            "Unable to open file {}.",
            path_cd.to_string_lossy().code_str(),
        )))?;

        // Compute the hash of the file contents and metadata.
        content_hashes.push(cache::combine(
            &cache::combine(&path_rcr.crypto_hash(), &cache::hash_read(&mut file)?),
            if executable { "+x" } else { "-x" },
        ));

        // Jump back to the beginning of the file so the tar builder can read it.
        file.seek(SeekFrom::Start(0))
            .map_err(failure::system(format!(
                "Unable to seek file {}.",
                path_cd.to_string_lossy().code_str(),
            )))?;

        // Add the file to the archive and return.
        add_file(builder, path_rcr, file, metadata.len(), executable)
    } else if metadata.file_type().is_symlink() {
        // It's a symlink. Read the target path.
        let target_path = read_link(path_cd).map_err(failure::system(format!(
            "Unable to read target of symbolic link {}.",
            path_cd.to_string_lossy().code_str(),
        )))?;

        // Compute the hash of the symlink path and the target path.
        content_hashes.push(cache::combine(path_rcr, &target_path));

        // Add the symlink to the archive.
        add_symlink(builder, path_rcr, &target_path)
    } else if metadata.file_type().is_dir() {
        // It's a directory. Only its name is relevant for the cache key.
        content_hashes.push(path_rcr.crypto_hash());

        // Add the directory to the archive.
        add_directory(builder, path_rcr)
    } else {
        Err(Failure::User(
            format!(
                "{} is not a file, directory, or symbolic link.",
                path_cd.to_string_lossy().code_str(),
            ),
            None,
        ))
    }
}

// Construct a tar archive and return a hash of its contents. This function does not follow symbolic
// links.
#[allow(clippy::similar_names)]
pub fn create<W: Write>(
    spinner_message: &str,
    writer: W,
    input_paths_rsd: &[PathBuf],
    excluded_input_paths_rsd: &[PathBuf],
    source_dir_cd: &Path,
    destination_dir_acr: &Path,
    interrupted: &Arc<AtomicBool>,
) -> Result<(W, String), Failure> {
    // Render a spinner animation in the terminal.
    let _guard = spin(spinner_message);

    // This vector will store all the hashes of the contents and metadata of all the files in the
    // archive. In the end, we will sort this vector and then take the hash of the whole thing.
    let mut content_hashes = vec![];

    // This set is used to avoid adding the same path to the archive multiple times, which could
    // otherwise easily happen since we explicitly add all ancestor directories for every entry
    // added to the archive.
    let mut visited_paths_rcr = HashSet::new();

    // This builder will be responsible for writing to the tar file.
    let mut builder = Builder::new(writer);

    // Add `destination_dir_acr` to the archive.
    add_directory(&mut builder, strip_root_rcr(destination_dir_acr))?;
    visited_paths_rcr.insert(PathBuf::new());

    // Convert the `excluded_input_paths` to be relative to the container filesystem root.
    let excluded_input_paths_rcr = excluded_input_paths_rsd
        .iter()
        .map(|excluded_input_path| {
            strip_root_rcr(&destination_dir_acr.join(excluded_input_path)).to_owned()
        })
        .collect::<Vec<_>>();

    // Add each path to the archive.
    for input_path_rsd in input_paths_rsd {
        // The original `input_path` is relative to `source_dir_cd`. Here we make it relative to the
        // current working directory instead.
        let input_path_cd = source_dir_cd.join(input_path_rsd);

        // Fetch filesystem metadata for `input_path`.
        let input_path_metadata =
            symlink_metadata(&input_path_cd).map_err(failure::system(format!(
                "Unable to fetch filesystem metadata for {}.",
                input_path_cd.to_string_lossy().code_str(),
            )))?;

        // Check what type of filesystem object the path corresponds to.
        if input_path_metadata.is_dir() {
            // It's a directory. Traverse it.
            let mut iterator = WalkDir::new(&input_path_cd).into_iter();
            loop {
                // If the user wants to stop the operation, quit now.
                if interrupted.load(Ordering::SeqCst) {
                    return Err(Failure::Interrupted);
                }

                // Unwrap the entry.
                let entry = if let Some(entry) = iterator.next() {
                    entry
                } else {
                    break;
                }
                .map_err(failure::user(format!(
                    "Unable to traverse directory {}.",
                    input_path_cd.to_string_lossy().code_str(),
                )))?;

                // Compute the path relative to the container filesystem root.
                let entry_path_acr =
                    destination_dir_acr.join(entry.path().strip_prefix(source_dir_cd).map_err(
                        failure::system(format!(
                            "Unable to relativize path {} with respect to {}.",
                            entry.path().to_string_lossy().code_str(),
                            source_dir_cd.to_string_lossy().code_str(),
                        )),
                    )?);
                let entry_path_rcr = strip_root_rcr(&entry_path_acr);

                // Fetch the metadata for this entry.
                let entry_metadata = entry.metadata().map_err(failure::system(format!(
                    "Unable to fetch filesystem metadata for {}.",
                    entry.path().to_string_lossy().code_str(),
                )))?;

                // Skip descending into directories which are denied by `excluded_input_paths`.
                // This is merely an optimization, since `add_path` would otherwise skip the
                // contents of the directory anyway.
                if entry_metadata.is_dir()
                    && path_excluded(&excluded_input_paths_rcr, entry_path_rcr)
                {
                    iterator.skip_current_dir();
                    continue;
                }

                // Add the path to the archive.
                add_path(
                    &mut builder,
                    &mut content_hashes,
                    &mut visited_paths_rcr,
                    &excluded_input_paths_rcr,
                    entry.path(),
                    entry_path_rcr,
                    &entry_metadata,
                )?;
            }
        } else {
            // It's not a directory, so hopefully it's a file or symlink. Add it to the archive.
            add_path(
                &mut builder,
                &mut content_hashes,
                &mut visited_paths_rcr,
                &excluded_input_paths_rcr,
                &input_path_cd,
                strip_root_rcr(
                    &destination_dir_acr.join(input_path_cd.strip_prefix(source_dir_cd).map_err(
                        failure::system(format!(
                            "Unable to relativize path {} with respect to {}.",
                            input_path_cd.to_string_lossy().code_str(),
                            source_dir_cd.to_string_lossy().code_str(),
                        )),
                    )?),
                ),
                &input_path_metadata,
            )?;
        }
    }

    // Sort the file hashes to ensure the directory traversal order doesn't matter.
    content_hashes.sort();

    // Return the tar file and the hash of its contents.
    Ok((
        builder
            .into_inner()
            .map_err(failure::system("Error writing tar archive."))?,
        content_hashes
            .iter()
            .fold(String::new(), |acc, x| cache::combine(&acc, x)),
    ))
}
