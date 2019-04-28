use crate::cache;
use ignore::Walk;
use std::{
  fs,
  fs::File,
  io::{Seek, SeekFrom, Write},
  path::Path,
};
use tar::{Builder, Header, HeaderMode};

// Construct a tar archive and return a hash of its contents.
pub fn tar<W: Write>(
  paths: &[String],
  location: &str,
  writer: W,
) -> Result<(W, String), String> {
  let mut file_hashes = vec![];

  let mut builder = Builder::new(writer);
  builder.follow_symlinks(false);

  for path in paths {
    let metadata = fs::metadata(path).map_err(|e| {
      format!(
        "Unable to fetch filesystem metadata for `{}`. Details: {}",
        path, e
      )
    })?;
    if metadata.is_dir() {
      for entry in Walk::new(path) {
        let entry = entry.map_err(|e| {
          format!("Unable to traverse directory `{}`. Details: {}", path, e)
        })?;
        let entry_metadata = entry.metadata().map_err(|e| {
          format!(
            "Unable to fetch filesystem metadata for `{}`. Details: {}",
            path, e
          )
        })?;

        // Here, `file_type()` should always return a `Some`. It could only
        // return `None` if the file represents STDIN, and that isn't the case
        // here.
        if entry.file_type().unwrap().is_file() {
          let mut header = Header::new_gnu();
          header
            .set_metadata_in_mode(&entry_metadata, HeaderMode::Deterministic);
          let mut file = File::open(entry.path()).map_err(|e| {
            format!(
              "Unable to open file`{}`. Details: {}",
              entry.path().to_string_lossy(),
              e
            )
          })?;
          let mut destination = Path::new(location).join(entry.path());

          // [tag:entry_destination_absolute]
          if destination.starts_with("/") {
            // Safe due to [ref:entry_destination_absolute]
            destination = destination.strip_prefix("/").unwrap().to_owned();
          }

          file_hashes.push(cache::combine(
            &cache::hash(&destination.to_string_lossy().to_string()),
            &cache::hash_read(&mut file)?,
          ));

          file.seek(SeekFrom::Start(0)).map_err(|e| {
            format!("Unable to seek temporary file. Details: {}", e)
          })?;

          builder.append_data(&mut header, destination, file).unwrap();
        }
      }
    } else {
      let mut header = Header::new_gnu();
      header.set_metadata_in_mode(&metadata, HeaderMode::Deterministic);
      let mut file = File::open(path).map_err(|e| {
        format!("Unable to open file`{}`. Details: {}", path, e)
      })?;
      let mut destination = Path::new(location).join(path);

      // [tag:destination_absolute]
      if destination.starts_with("/") {
        // Safe due to [ref:destination_absolute]
        destination = destination.strip_prefix("/").unwrap().to_owned();
      }

      file_hashes.push(cache::combine(
        &cache::hash(&destination.to_string_lossy().to_string()),
        &cache::hash_read(&mut file)?,
      ));

      file.seek(SeekFrom::Start(0)).map_err(|e| {
        format!("Unable to seek temporary file. Details: {}", e)
      })?;

      builder.append_data(&mut header, destination, file).unwrap();
    }
  }

  file_hashes.sort();

  Ok((
    builder
      .into_inner()
      .map_err(|e| format!("Error writing tar archive. Details: {}", e))?,
    file_hashes
      .iter()
      .fold(String::new(), |acc, x| cache::combine(&acc, x)),
  ))
}
