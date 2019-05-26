use std::{error, fmt};

// We distinguish between three kinds of failures:
// 1. The user interrupted the program
// 2. Some system operation (e.g., creating a container) failed
// 3. There was a problem with the user's input (e.g., their task failed)
#[derive(Debug)]
pub enum Failure {
    Interrupted, // E.g., by SIGINT or SIGTERM
    System(String, Option<Box<dyn error::Error + 'static>>),
    User(String, Option<Box<dyn error::Error + 'static>>),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Failure::System(message, None) | Failure::User(message, None) => {
                write!(f, "{}", message)
            }
            Failure::System(message, Some(source)) | Failure::User(message, Some(source)) => {
                write!(f, "{} Reason: {}", message, source)
            }
            Failure::Interrupted => write!(f, "Interrupted."),
        }
    }
}

impl error::Error for Failure {
    fn source<'a>(&'a self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Failure::System(_, source) => source.as_ref().map(|e| &**e),
            Failure::User(_, source) => source.as_ref().map(|e| &**e),
            Failure::Interrupted => None,
        }
    }
}

// This is a helper function to convert a `std::error::Error` into a system failure. It's written
// in a curried style so it can be used in a higher-order fashion, e.g.,
// `foo.map_err(system_error("Error doing foo."))`.
pub fn system_error<E: error::Error + 'static>(message: &str) -> impl FnOnce(E) -> Failure {
    let message = message.to_owned();
    move |error: E| Failure::System(message, Some(Box::new(error)))
}

// This is a helper function to convert a `std::error::Error` into a user failure. It's written in a
// curried style so it can be used in a higher-order fashion, e.g.,
// `foo.map_err(user_error("Error doing foo."))`.
pub fn user_error<E: error::Error + 'static>(message: &str) -> impl FnOnce(E) -> Failure {
    let message = message.to_owned();
    move |error: E| Failure::User(message, Some(Box::new(error)))
}
