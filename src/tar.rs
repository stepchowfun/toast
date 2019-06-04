use crate::{cache, cache::CryptoHash, failure, failure::Failure, format::CodeStr, spinner::spin};
use std::{
    collections::HashSet,
    fs::{read_link, symlink_metadata, File, Metadata},
    io::{empty, Read, Seek, SeekFrom, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tar::{Builder, EntryType, Header};
use walkdir::WalkDir;

// Tar archives must contain only relative paths. For our purposes, the paths will be relative to
// the filesystem root, so we need to strip the leading `/` before adding paths to the archive.
fn strip_root(absolute_path: &Path) -> &Path {
    // The `unwrap` is safe since `absolute_path` is absolute and Toast only supports Unix.
    absolute_path.strip_prefix("/").unwrap()
}

// Add a file to a tar archive.
fn add_file<R: Read, W: Write>(
    builder: &mut Builder<W>,
    visited_paths: &mut HashSet<PathBuf>,
    path: &Path, // Must be relative
    data: R,
    size: u64,
    executable: bool,
) -> Result<(), Failure> {
    // Only visit this path once.
    if !visited_paths.insert(path.to_owned()) {
        return Ok(());
    }

    // Construct a tar header for this entry.
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_mode(if executable { 0o777 } else { 0o666 });
    header.set_size(size);

    // Add the entry to the archive.
    builder
        .append_data(&mut header, path, data)
        .map_err(failure::system("Error appending data to tar archive."))?;

    // Everything succeeded.
    Ok(())
}

// Add a symlink to a tar archive.
fn add_symlink<W: Write>(
    builder: &mut Builder<W>,
    visited_paths: &mut HashSet<PathBuf>,
    path: &Path, // Must be relative
    target: &Path,
) -> Result<(), Failure> {
    // Only visit this path once.
    if !visited_paths.insert(path.to_owned()) {
        return Ok(());
    }

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
        .append_data(&mut header, path, empty())
        .map_err(failure::system("Error appending data to tar archive."))?;

    // Everything succeeded.
    Ok(())
}

// Add a directory to a tar archive.
fn add_directory<W: Write>(
    builder: &mut Builder<W>,
    visited_paths: &mut HashSet<PathBuf>,
    path: &Path, // Must be relative
) -> Result<(), Failure> {
    // Only visit this path once.
    if !visited_paths.insert(path.to_owned()) {
        return Ok(());
    }

    // If the path has no components, there's nothing to do. The root directory will already exist.
    // Without this check, we could encounter the following error: `paths in archives must have at
    // least one component when setting path for`.
    if path.components().next().is_none() {
        return Ok(());
    }

    // Construct a tar header for this entry.
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Directory);
    header.set_mode(0o777);
    header.set_size(0);

    // Add the entry to the archive.
    builder
        .append_data(&mut header, path, empty())
        .map_err(failure::system("Error appending data to tar archive."))?;

    // Everything succeeded.
    Ok(())
}

// Add a file, symlink, or directory to a tar archive.
fn add_path<W: Write>(
    builder: &mut Builder<W>,
    content_hashes: &mut Vec<String>,
    visited_paths: &mut HashSet<PathBuf>,
    source_path: &Path,
    destination_path: &Path,
    metadata: &Metadata,
) -> Result<(), Failure> {
    // Add the ancestor directories. They would be created automatically, but we add them explicitly
    // here to ensure they have the right permissions.
    if let Some(parent) = destination_path.parent() {
        for ancestor in parent.ancestors() {
            add_directory(builder, visited_paths, ancestor)?;
        }
    }

    // Check the type of the entry.
    if metadata.file_type().is_file() {
        // Determine if the file has the executable bit set.
        let mode = metadata.permissions().mode();
        let executable = mode & 0o1 > 0 || mode & 0o10 > 0 || mode & 0o100 > 0;

        // It's a file. Open it so we can compute the hash of its contents and add it to the
        // archive.
        let mut file = File::open(source_path).map_err(failure::system(format!(
            "Unable to open file {}.",
            source_path.to_string_lossy().code_str(),
        )))?;

        // Compute the hash of the file contents and metadata.
        content_hashes.push(cache::combine(
            &cache::combine(
                &destination_path.crypto_hash(),
                &cache::hash_read(&mut file)?,
            ),
            if executable { "+x" } else { "-x" },
        ));

        // Jump back to the beginning of the file so the tar builder can read it.
        file.seek(SeekFrom::Start(0))
            .map_err(failure::system(format!(
                "Unable to seek file {}.",
                source_path.to_string_lossy().code_str(),
            )))?;

        // Add the file to the archive and return.
        add_file(
            builder,
            visited_paths,
            destination_path,
            file,
            metadata.len(),
            executable,
        )
    } else if metadata.file_type().is_symlink() {
        // It's a symlink. Read the target path.
        let target_path = read_link(source_path).map_err(failure::system(format!(
            "Unable to read target of symbolic link {}.",
            source_path.to_string_lossy().code_str(),
        )))?;

        // Compute the hash of the symlink path and the target path.
        content_hashes.push(cache::combine(destination_path, &target_path));

        // Add the symlink to the archive.
        add_symlink(builder, visited_paths, destination_path, &target_path)
    } else if metadata.file_type().is_dir() {
        // It's a directory. Only its name is relevant for the cache key.
        content_hashes.push(destination_path.crypto_hash());

        // Add the directory to the archive.
        add_directory(builder, visited_paths, destination_path)
    } else {
        Err(Failure::User(
            format!(
                "{} is not a file, directory, or symbolic link.",
                source_path.to_string_lossy().code_str(),
            ),
            None,
        ))
    }
}

// Construct a tar archive and return a hash of its contents. This function does not follow symbolic
// links.
pub fn create<W: Write>(
    spinner_message: &str,
    writer: W,
    input_paths: &[PathBuf],
    source_dir: &Path,
    destination_dir: &Path,
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
    let mut visited_paths = HashSet::new();

    // This builder will be responsible for writing to the tar file.
    let mut builder = Builder::new(writer);

    // Add `destination_dir` to the archive.
    add_directory(
        &mut builder,
        &mut visited_paths,
        strip_root(&destination_dir),
    )?;

    // Add each path to the archive.
    for input_path in input_paths {
        // The original `input_path` is relative to `source_dir`. Here we make it relative to the
        // working directory instead.
        let input_path = source_dir.join(input_path);

        // Fetch filesystem metadata for `input_path`.
        let input_path_metadata =
            symlink_metadata(&input_path).map_err(failure::system(format!(
                "Unable to fetch filesystem metadata for {}.",
                input_path.to_string_lossy().code_str(),
            )))?;

        // Check what type of filesystem object the path corresponds to.
        if input_path_metadata.is_dir() {
            // It's a directory. Traverse it.
            for entry in WalkDir::new(&input_path) {
                // If the user wants to stop the operation, quit now.
                if interrupted.load(Ordering::SeqCst) {
                    return Err(Failure::Interrupted);
                }

                // Unwrap the entry.
                let entry = entry.map_err(failure::user(format!(
                    "Unable to traverse path {}.",
                    input_path.to_string_lossy().code_str(),
                )))?;

                // Fetch the metadata for this entry.
                let entry_metadata = entry.metadata().map_err(failure::system(format!(
                    "Unable to fetch filesystem metadata for {}.",
                    entry.path().to_string_lossy().code_str(),
                )))?;

                // Add the path to the archive.
                add_path(
                    &mut builder,
                    &mut content_hashes,
                    &mut visited_paths,
                    entry.path(),
                    strip_root(
                        &destination_dir.join(entry.path().strip_prefix(&source_dir).map_err(
                            failure::system(format!(
                                "Unable to relativize path {} with respect to {}.",
                                entry.path().to_string_lossy().code_str(),
                                source_dir.to_string_lossy().code_str(),
                            )),
                        )?),
                    ),
                    &entry_metadata,
                )?;
            }
        } else {
            // It's not a directory, so hopefully it's a file or symlink. Add it to the archive.
            add_path(
                &mut builder,
                &mut content_hashes,
                &mut visited_paths,
                &input_path,
                strip_root(
                    &destination_dir.join(input_path.strip_prefix(&source_dir).map_err(
                        failure::system(format!(
                            "Unable to relativize path {} with respect to {}.",
                            input_path.to_string_lossy().code_str(),
                            source_dir.to_string_lossy().code_str(),
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
