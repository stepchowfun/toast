use crate::{
    failure::{system_error, Failure},
    toastfile::Task,
};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io, io::Read};

// Determine the cache ID of a task based on the cache ID of the previous task in the schedule (or
// the hash of the base image, if this is the first task).
pub fn key(
    previous_key: &str,
    task: &Task,
    input_files_hash: &str,
    environment: &HashMap<String, String>,
) -> String {
    // Start with the previous key.
    let mut cache_key = previous_key.to_owned();

    // If there are no environment variables, no input paths, no command to run, we can just use the
    // cache key from the previous task.
    if task.environment.is_empty() && task.input_paths.is_empty() && task.command.is_none() {
        return cache_key;
    }

    // Environment variables
    let mut variables = task.environment.keys().collect::<Vec<_>>();
    variables.sort();
    for variable in variables {
        cache_key = extend(&cache_key, variable);
        cache_key = extend(&cache_key, &environment[variable]); // [ref:environment_valid]
    }

    // Input paths and contents
    cache_key = extend(&cache_key, &input_files_hash);

    // Location
    cache_key = extend(&cache_key, &task.location.to_string_lossy());

    // User
    cache_key = extend(&cache_key, &task.user);

    // Command
    if let Some(command) = &task.command {
        cache_key = extend(&cache_key, &command);
    }

    // We add this "toast-" prefix because Docker has a rule that tags cannot be 64-byte hexadecimal
    // strings. See this for more details: https://github.com/moby/moby/issues/20972
    format!("toast-{}", cache_key)
}

// Compute the hash of a readable object (e.g., a file). This function does not need to load all the
// data in memory at the same time.
pub fn hash_read<R: Read>(input: &mut R) -> Result<String, Failure> {
    let mut hasher = Sha256::new();
    io::copy(input, &mut hasher).map_err(system_error("Unable to compute hash."))?;
    Ok(hex::encode(hasher.result()))
}

// Compute the hash of a string.
pub fn hash_str(input: &str) -> String {
    hex::encode(Sha256::digest(input.as_bytes()))
}

// Combine a hash with another string to form a new hash.
pub fn extend(x: &str, y: &str) -> String {
    hash_str(&format!("{}{}", x, y))
}

#[cfg(test)]
mod tests {
    use crate::{
        cache::{extend, hash_read, hash_str, key},
        toastfile::{Task, DEFAULT_LOCATION, DEFAULT_USER},
    };
    use std::{collections::HashMap, path::Path};

    #[test]
    fn key_pure() {
        let mut environment: HashMap<String, Option<String>> = HashMap::new();
        environment.insert("foo".to_owned(), None);

        let previous_key = "corge";

        let task = Task {
            dependencies: vec![],
            cache: true,
            environment,
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: Some("echo wibble".to_owned()),
        };

        let input_files_hash = "grault";

        let mut full_environment = HashMap::new();
        full_environment.insert("foo".to_owned(), "qux".to_owned());

        assert_eq!(
            key(previous_key, &task, input_files_hash, &full_environment),
            key(previous_key, &task, input_files_hash, &full_environment)
        );
    }

    #[test]
    fn key_previous_key() {
        let previous_key1 = "foo";
        let previous_key2 = "bar";

        let task = Task {
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: Some("echo wibble".to_owned()),
        };

        let input_files_hash = "grault";

        let full_environment = HashMap::new();

        assert_ne!(
            key(previous_key1, &task, input_files_hash, &full_environment),
            key(previous_key2, &task, input_files_hash, &full_environment)
        );
    }

    #[test]
    fn key_environment_order() {
        let mut environment1: HashMap<String, Option<String>> = HashMap::new();
        environment1.insert("foo".to_owned(), None);
        environment1.insert("bar".to_owned(), None);

        let mut environment2: HashMap<String, Option<String>> = HashMap::new();
        environment2.insert("bar".to_owned(), None);
        environment2.insert("foo".to_owned(), None);

        let previous_key = "corge";

        let task1 = Task {
            dependencies: vec![],
            cache: true,
            environment: environment1,
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: Some("echo wibble".to_owned()),
        };

        let task2 = Task {
            dependencies: vec![],
            cache: true,
            environment: environment2,
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: Some("echo wibble".to_owned()),
        };

        let input_files_hash = "grault";

        let mut full_environment = HashMap::new();
        full_environment.insert("foo".to_owned(), "qux".to_owned());
        full_environment.insert("bar".to_owned(), "fum".to_owned());

        assert_eq!(
            key(previous_key, &task1, input_files_hash, &full_environment),
            key(previous_key, &task2, input_files_hash, &full_environment)
        );
    }

    #[test]
    fn key_environment_keys() {
        let mut environment1: HashMap<String, Option<String>> = HashMap::new();
        environment1.insert("foo".to_owned(), None);

        let mut environment2: HashMap<String, Option<String>> = HashMap::new();
        environment2.insert("bar".to_owned(), None);

        let previous_key = "corge";

        let task1 = Task {
            dependencies: vec![],
            cache: true,
            environment: environment1,
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: Some("echo wibble".to_owned()),
        };

        let task2 = Task {
            dependencies: vec![],
            cache: true,
            environment: environment2,
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: Some("echo wibble".to_owned()),
        };

        let input_files_hash = "grault";

        let mut full_environment = HashMap::new();
        full_environment.insert("foo".to_owned(), "qux".to_owned());
        full_environment.insert("bar".to_owned(), "fum".to_owned());

        assert_ne!(
            key(previous_key, &task1, input_files_hash, &full_environment),
            key(previous_key, &task2, input_files_hash, &full_environment)
        );
    }

    #[test]
    fn key_environment_values() {
        let mut environment: HashMap<String, Option<String>> = HashMap::new();
        environment.insert("foo".to_owned(), None);

        let previous_key = "corge";

        let task = Task {
            dependencies: vec![],
            cache: true,
            environment,
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: Some("echo wibble".to_owned()),
        };

        let input_files_hash = "grault";

        let mut full_environment1 = HashMap::new();
        full_environment1.insert("foo".to_owned(), "bar".to_owned());
        let mut full_environment2 = HashMap::new();
        full_environment2.insert("foo".to_owned(), "baz".to_owned());

        assert_ne!(
            key(previous_key, &task, input_files_hash, &full_environment1),
            key(previous_key, &task, input_files_hash, &full_environment2)
        );
    }

    #[test]
    fn key_input_files_hash() {
        let previous_key = "corge";

        let task = Task {
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: Some("echo wibble".to_owned()),
        };

        let input_files_hash1 = "foo";
        let input_files_hash2 = "bar";

        let full_environment = HashMap::new();

        assert_ne!(
            key(previous_key, &task, input_files_hash1, &full_environment),
            key(previous_key, &task, input_files_hash2, &full_environment)
        );
    }

    #[test]
    fn key_location() {
        let previous_key = "corge";

        let task1 = Task {
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new("/foo").to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: Some("echo wibble".to_owned()),
        };

        let task2 = Task {
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new("/bar").to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: Some("echo wibble".to_owned()),
        };

        let input_files_hash = "grault";

        let full_environment = HashMap::new();

        assert_ne!(
            key(previous_key, &task1, input_files_hash, &full_environment),
            key(previous_key, &task2, input_files_hash, &full_environment)
        );
    }

    #[test]
    fn key_user() {
        let previous_key = "corge";

        let task1 = Task {
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: "foo".to_owned(),
            command: Some("echo wibble".to_owned()),
        };

        let task2 = Task {
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: "bar".to_owned(),
            command: Some("echo wibble".to_owned()),
        };

        let input_files_hash = "grault";

        let full_environment = HashMap::new();

        assert_ne!(
            key(previous_key, &task1, input_files_hash, &full_environment),
            key(previous_key, &task2, input_files_hash, &full_environment)
        );
    }

    #[test]
    fn key_command_different() {
        let previous_key = "corge";

        let task1 = Task {
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: Some("echo foo".to_owned()),
        };

        let task2 = Task {
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: Some("echo bar".to_owned()),
        };

        let input_files_hash = "grault";

        let full_environment = HashMap::new();

        assert_ne!(
            key(previous_key, &task1, input_files_hash, &full_environment),
            key(previous_key, &task2, input_files_hash, &full_environment)
        );
    }

    #[test]
    fn key_command_some_none() {
        let previous_key = "corge";

        let task1 = Task {
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: Some("echo wibble".to_owned()),
        };

        let task2 = Task {
            dependencies: vec![],
            cache: true,
            environment: HashMap::new(),
            watch: false,
            input_paths: vec![],
            output_paths: vec![],
            ports: vec![],
            location: Path::new(DEFAULT_LOCATION).to_owned(),
            user: DEFAULT_USER.to_owned(),
            command: None,
        };

        let input_files_hash = "grault";

        let full_environment = HashMap::new();

        assert_ne!(
            key(previous_key, &task1, input_files_hash, &full_environment),
            key(previous_key, &task2, input_files_hash, &full_environment)
        )
    }

    #[test]
    fn hash_read_pure() {
        let mut str1 = b"foo" as &[u8];
        let mut str2 = b"foo" as &[u8];
        assert_eq!(hash_read(&mut str1).unwrap(), hash_read(&mut str2).unwrap());
    }

    #[test]
    fn hash_read_not_constant() {
        let mut str1 = b"foo" as &[u8];
        let mut str2 = b"bar" as &[u8];
        assert_ne!(hash_read(&mut str1).unwrap(), hash_read(&mut str2).unwrap());
    }

    #[test]
    fn hash_str_pure() {
        assert_eq!(hash_str("foo"), hash_str("foo"));
    }

    #[test]
    fn hash_str_not_constant() {
        assert_ne!(hash_str("foo"), hash_str("bar"));
    }

    #[test]
    fn extend_pure() {
        assert_eq!(extend("foo", "bar"), extend("foo", "bar"));
    }

    #[test]
    fn extend_not_constant() {
        assert_ne!(extend("foo", "bar"), extend("foo", "baz"));
        assert_ne!(extend("foo", "bar"), extend("baz", "bar"));
    }
}
