use crate::{cache, format::CodeStr, spinner::spin};
use std::{
  fs::File,
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

// Add a file or directory to a tar archive.
pub fn append<R: Read, W: Write>(
  builder: &mut Builder<W>,
  path: &Path,
  data: R,
  size: u64,
  entry_type: EntryType,
  executable: bool,
) {
  // Tar archives must contain only relative paths. But for our purposes, the
  // paths will be relative to the filesystem root, so we can just strip the
  // leading `/`. The `unwrap` is safe due to the prefix check.
  let destination = if path.starts_with("/") {
    path.strip_prefix("/").unwrap().to_owned()
  } else {
    path.to_owned()
  };

  // If destination is the root path `/`, there is nothing to do. That path
  // already exists in any file system.
  if destination.parent().is_none() {
    return;
  }

  // Construct a tar header for this entry.
  let mut header = Header::new_gnu();
  header.set_mode(if executable { 0o777 } else { 0o666 });
  header.set_size(size);

  // Add the entry to the archive.
  header.set_entry_type(entry_type);
  builder.append_data(&mut header, destination, data).unwrap();
}

// Construct a tar archive and return a hash of its contents.
pub fn create<W: Write>(
  spinner_message: &str,
  writer: W,
  input_paths: &[PathBuf],
  source_dir: &Path,
  destination_dir: &Path,
  interrupted: &Arc<AtomicBool>,
) -> Result<(W, String), String> {
  // Render a spinner animation in the terminal.
  let _guard = spin(spinner_message);

  // Canonicalize the source directory such that other paths can be relativized
  // with respect to it.
  let source_dir = source_dir.canonicalize().map_err(|e| {
    format!(
      "Unable to canonicalize path {}. Details: {}.",
      source_dir.to_string_lossy().code_str(),
      e
    )
  })?;

  // This vector will store all the hashes of the contents and metadata of all
  // the files in the archive. In the end, we will sort this vector and then
  // take the hash of the whole thing.
  let mut file_hashes = vec![];

  // This builder will be responsible for writing to the tar file.
  let mut builder = Builder::new(writer);
  builder.follow_symlinks(false); // [tag:symlinks]

  // Add `destination_dir` to the archive.
  append(
    &mut builder,
    &destination_dir,
    empty(),
    0,
    EntryType::Directory,
    true,
  );

  // Add each path to the archive.
  for relative_input_path in input_paths {
    // Compute the source path.
    let absolute_input_path = source_dir.join(relative_input_path);

    // The path is a directory, so we need to traverse it.
    for entry in WalkDir::new(&absolute_input_path) {
      // If the user wants to stop the operation, quit now.
      if interrupted.load(Ordering::SeqCst) {
        return Err(super::INTERRUPT_MESSAGE.to_owned());
      }

      // Unwrap the entry.
      let entry = entry.map_err(|e| {
        format!(
          "Unable to traverse path {}. Details: {}.",
          &absolute_input_path.to_string_lossy().code_str(),
          e
        )
      })?;

      // Fetch the metadata for this entry.
      let entry_metadata = entry.metadata().map_err(|e| {
        format!(
          "Unable to fetch filesystem metadata for {}. Details: {}.",
          &absolute_input_path.to_string_lossy().code_str(),
          e
        )
      })?;

      // Fetch the host path.
      let absolute_host_path = entry.path().canonicalize().map_err(|e| {
        format!(
          "Unable to canonicalize path {}. Details: {}.",
          &entry.path().to_string_lossy().code_str(),
          e
        )
      })?;

      // Relativize the host path.
      let relative_path = absolute_host_path
        .strip_prefix(&source_dir)
        .map_err(|e| {
          format!(
            "Unable to relativize path {} with respect to {}. Details: {}.",
            &entry.path().to_string_lossy().code_str(),
            &source_dir.to_string_lossy().code_str(),
            e
          )
        })?
        .to_owned();

      // Check the type of the entry. Note that Toast ignores symbolic links.
      // [ref:symlinks]
      if entry.file_type().is_file() {
        // Determine if the file has the executable bit set.
        let mode = entry_metadata.permissions().mode();
        let executable = mode & 0o1 > 0 || mode & 0o10 > 0 || mode & 0o100 > 0;

        // It's a file. Open it so we can compute the hash of its contents.
        let mut file = File::open(&absolute_host_path).map_err(|e| {
          format!(
            "Unable to open file {}. Details: {}.",
            &absolute_host_path.to_string_lossy().code_str(),
            e
          )
        })?;

        // Compute the hash of the file contents and metadata.
        file_hashes.push(cache::extend(
          &cache::extend(
            &cache::hash_str(&relative_path.to_string_lossy()),
            &cache::hash_read(&mut file)?,
          ),
          if executable { "+x" } else { "-x" },
        ));

        // Jump back to the beginning of the file so the tar builder can read it.
        file.seek(SeekFrom::Start(0)).map_err(|e| {
          format!(
            "Unable to seek file {}. Details: {}.",
            &absolute_host_path.to_string_lossy().code_str(),
            e
          )
        })?;

        // Add the file to the archive and return.
        append(
          &mut builder,
          &destination_dir.join(&relative_path),
          file,
          entry_metadata.len(),
          EntryType::Regular,
          executable,
        );
      } else if entry.file_type().is_dir() {
        // It's a directory. Only its name is relevant for the cache key.
        file_hashes.push(cache::hash_str(&relative_path.to_string_lossy()));

        // Add the directory to the archive.
        append(
          &mut builder,
          &destination_dir.join(&relative_path),
          empty(),
          0,
          EntryType::Directory,
          true,
        );
      }
    }
  }

  // Sort the file hashes to ensure the directory traversal order doesn't
  // matter.
  file_hashes.sort();

  // Return the tar file and the hash of its contents.
  Ok((
    builder
      .into_inner()
      .map_err(|e| format!("Error writing tar archive. Details: {}.", e))?,
    file_hashes
      .iter()
      .fold(cache::hash_str(""), |acc, x| cache::extend(&acc, x)),
  ))
}
