//! Termination requests from outside: a signal sets a flag, and the loop
//! leaves through its normal restore path.

#[cfg(unix)]
pub(crate) use unix::StopRequest;
#[cfg(windows)]
pub(crate) use windows::StopRequest;

#[cfg(unix)]
mod unix {
    use std::io;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use signal_hook::SigId;
    use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
    use signal_hook::flag;

    const STOPPING: [i32; 4] = [SIGTERM, SIGHUP, SIGINT, SIGQUIT];

    /// Set once the process was asked to stop. Dropping it gives the signals
    /// their usual meaning back.
    #[derive(Debug, Default)]
    pub(crate) struct StopRequest {
        requested: Arc<AtomicBool>,
        handlers: Vec<SigId>,
    }

    impl StopRequest {
        /// Watch the signals that end a program. A second signal while we
        /// restore the terminal ends the process at once, so a hung restore
        /// never traps the user.
        pub(crate) fn watch() -> io::Result<Self> {
            let mut stop = Self::default();
            for signal in STOPPING {
                let shutdown =
                    flag::register_conditional_shutdown(signal, 1, Arc::clone(&stop.requested))?;
                stop.handlers.push(shutdown);
                stop.handlers
                    .push(flag::register(signal, Arc::clone(&stop.requested))?);
            }
            Ok(stop)
        }

        pub(crate) fn requested(&self) -> bool {
            self.requested.load(Ordering::Relaxed)
        }
    }

    impl Drop for StopRequest {
        fn drop(&mut self) {
            for handler in self.handlers.drain(..) {
                signal_hook::low_level::unregister(handler);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::StopRequest;

        /// One test only: signal handlers are process-wide, so a second test
        /// running alongside would see this one's signals.
        #[test]
        fn a_signal_sets_the_request_and_a_later_watch_starts_fresh() {
            let earlier = StopRequest::watch().expect("signal handlers register");
            assert!(!earlier.requested());
            signal_hook::low_level::raise(signal_hook::consts::SIGQUIT).expect("raise works");
            assert!(
                earlier.requested(),
                "the signal set the flag and the process lives"
            );
            drop(earlier);
            let later = StopRequest::watch().expect("signal handlers register");
            assert!(!later.requested());
            // with the earlier handlers still installed, its set flag would
            // end this test process here instead of setting the new flag
            signal_hook::low_level::raise(signal_hook::consts::SIGQUIT).expect("raise works");
            assert!(later.requested());
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::io;

    /// Never set: in the raw console Ctrl-C reaches the app as the byte
    /// 0x03, and closing the console ends the process outright.
    #[derive(Debug, Default)]
    pub(crate) struct StopRequest {}

    impl StopRequest {
        #[allow(clippy::unnecessary_wraps)] // the unix watch can fail
        pub(crate) fn watch() -> io::Result<Self> {
            Ok(Self {})
        }

        #[allow(clippy::unused_self)] // the unix request carries a flag
        pub(crate) fn requested(&self) -> bool {
            false
        }
    }
}
