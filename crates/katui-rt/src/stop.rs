//! Termination requests from outside: a signal sets a flag, and the loop
//! leaves through its normal restore path.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use signal_hook::flag;

const STOPPING: [i32; 4] = [SIGTERM, SIGHUP, SIGINT, SIGQUIT];

/// Set once the process was asked to stop.
#[derive(Debug, Clone, Default)]
pub(crate) struct StopRequest(Arc<AtomicBool>);

impl StopRequest {
    /// Watch the signals that end a program. A second signal while we
    /// restore the terminal ends the process at once, so a hung restore
    /// never traps the user.
    pub(crate) fn watch() -> io::Result<Self> {
        let requested = Arc::new(AtomicBool::new(false));
        for signal in STOPPING {
            flag::register_conditional_shutdown(signal, 1, Arc::clone(&requested))?;
            flag::register(signal, Arc::clone(&requested))?;
        }
        Ok(Self(requested))
    }

    pub(crate) fn requested(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::StopRequest;

    #[test]
    fn a_terminating_signal_sets_the_request_instead_of_killing_us() {
        let stop = StopRequest::watch().expect("signal handlers register");
        assert!(!stop.requested());
        signal_hook::low_level::raise(signal_hook::consts::SIGHUP).expect("raise works");
        assert!(stop.requested());
    }
}
