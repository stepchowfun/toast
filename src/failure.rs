use std::{error, fmt};

// We distinguish between three kinds of failures:
// 1. The user interrupted the program
// 2. Some system operation (e.g., creating a container) failed
// 3. There was a problem with the user's input (e.g., their task failed)
#[derive(Debug)]
pub enum Failure {
    Interrupted, // E.g., by SIGINT or SIGTERM
    System(String, Option<Box<dyn error::Error>>),
    User(String, Option<Box<dyn error::Error>>),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::System(message, None) | Self::User(message, None) => write!(f, "{}", message),
            Self::System(message, Some(source)) | Self::User(message, Some(source)) => {
                write!(f, "{} Reason: {}", message, source)
            }
            Self::Interrupted => write!(f, "Interrupted."),
        }
    }
}

impl error::Error for Failure {
    fn source<'a>(&'a self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::System(_, source) => source.as_ref().map(|e| &**e),
            Self::User(_, source) => source.as_ref().map(|e| &**e),
            Self::Interrupted => None,
        }
    }
}

// This is a helper function to convert a `std::error::Error` into a system failure. It's written in
// a curried style so it can be used in a higher-order fashion, e.g.,
// `foo.map_err(failure::system("Error doing foo."))`.
pub fn system<S: Into<String>, E: error::Error + 'static>(message: S) -> impl FnOnce(E) -> Failure {
    let message = message.into();
    move |error: E| Failure::System(message, Some(Box::new(error)))
}

// This is a helper function to convert a `std::error::Error` into a user failure. It's written in a
// curried style so it can be used in a higher-order fashion, e.g.,
// `foo.map_err(failure::user("Error doing foo."))`.
pub fn user<S: Into<String>, E: error::Error + 'static>(message: S) -> impl FnOnce(E) -> Failure {
    let message = message.into();
    move |error: E| Failure::User(message, Some(Box::new(error)))
}
