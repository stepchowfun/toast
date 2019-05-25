use crossbeam::channel::{bounded, Sender};
use indicatif::{ProgressBar, ProgressStyle};
use scopeguard::guard;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    thread::sleep,
    time::{Duration, Instant},
};

// Render a spinner in the terminal. When the returned value is dropped, the
// spinner is stopped.
pub fn spin(message: &str) -> impl Drop {
    // Start a thread for our spinner-as-a-service. This thread will only be
    // created once and will live for the duration of the whole program.
    lazy_static! {
      static ref SPINNER_SERVICE: Sender<(String, Arc<AtomicBool>, Sender<()>)> = {
        // Create a channel for requests to start spinning.
        let (request_sender, request_receiver) =
          bounded::<(String, Arc<AtomicBool>, Sender<()>)>(0);

        // Start a thread to handle spinner requests.
        thread::spawn(move || loop {
          // Wait for a request. The `unwrap` is safe since we never hang up the
          // channel.
          let (message, spinning, response_sender) =
            request_receiver.recv().unwrap();

          // Create the spinner!
          let spinner = ProgressBar::new(1);
          spinner.set_style(ProgressStyle::default_spinner());
          spinner.set_message(&message);

          // Animate the spinner for as long as necessary.
          let now = Instant::now();
          while spinning.load(Ordering::SeqCst) {
            // Render the next frame of the spinner.
            spinner.tick();

            // For the first 100ms, we animate on a shorter time interval so we
            // can stop faster if the work finishes instantly. If the work takes
            // longer than that, we slow the animation down out of courtesy for
            // the CPU.
            if now.elapsed() < Duration::from_millis(100) {
              sleep(Duration::from_millis(16));
            } else {
              sleep(Duration::from_millis(100));
            }
          }

          // Clean up the spinner.
          spinner.finish_and_clear();

          // Inform the caller that the spinner has been cleaned up. The `unwrap`
          // is safe since we never hang up the channel.
          response_sender.send(()).unwrap();
        });

        // The sender half of the request channel is the API for this spinner
        // service. Return it.
        request_sender
      };
    }

    // Create a channel for waiting on the spinner.
    let (response_sender, response_receiver) = bounded::<()>(0);

    // This will be set to `false` when it's time to stop the spinner.
    let spinning = Arc::new(AtomicBool::new(true));

    // Create and animate the spinner. The `unwrap` is safe since we never hang
    // up the channel.
    SPINNER_SERVICE
        .send((message.to_owned(), spinning.clone(), response_sender))
        .unwrap();

    // Return a guard that stops the spinner via its destructor.
    guard((), move |_| {
        // Tell the spinner service to stop the spinner.
        spinning.store(false, Ordering::SeqCst);

        // Wait for the spinner to stop. The `unwrap` is safe since we never hang
        // up the channel.
        response_receiver.recv().unwrap();
    })
}
