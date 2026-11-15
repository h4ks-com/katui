//! Where host failures wait: across a runner that cannot return them, and
//! across a callback that has to return something anyway.

use std::cell::RefCell;
use std::sync::{Arc, Mutex, PoisonError};

/// The first failure a host reported, kept until someone takes it.
pub struct Failure<E>(Arc<Mutex<Option<E>>>);

impl<E> Failure<E> {
    /// Keep `error` unless an earlier one waits.
    pub fn keep(&self, error: E) {
        self.slot(|slot| {
            slot.get_or_insert(error);
        });
    }

    pub fn take(&self) -> Option<E> {
        self.slot(Option::take)
    }

    #[must_use]
    pub fn is_set(&self) -> bool {
        self.slot(|slot| slot.is_some())
    }

    fn slot<R>(&self, with: impl FnOnce(&mut Option<E>) -> R) -> R {
        with(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl<E> Clone for Failure<E> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<E> Default for Failure<E> {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(None)))
    }
}

/// The first failure of the callbacks inside one call.
pub struct Catch<E>(RefCell<Option<E>>);

impl<E> Catch<E> {
    /// What `result` holds, or `fallback()` with the failure kept.
    pub fn or<T>(&self, result: Result<T, E>, fallback: impl FnOnce() -> T) -> T {
        result.unwrap_or_else(|error| {
            self.0.borrow_mut().get_or_insert(error);
            fallback()
        })
    }
}

/// Run `body`, whose callbacks report failures through the [`Catch`] it
/// gets, and return what it made or the first failure.
pub fn catching<T, E>(body: impl FnOnce(&Catch<E>) -> T) -> Result<T, E> {
    let catch = Catch(RefCell::new(None));
    let made = body(&catch);
    catch.0.into_inner().map_or(Ok(made), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_failure_wins() {
        let failure = Failure::default();
        failure.clone().keep("first");
        failure.keep("second");
        assert!(failure.is_set());
        assert_eq!(failure.take(), Some("first"));
        assert_eq!(failure.take(), None);
    }

    #[test]
    fn catching_returns_the_first_failure() {
        let made = catching(|catch| {
            (0..3)
                .map(|at| catch.or(if at == 0 { Ok(at) } else { Err(at) }, || 9))
                .sum::<i32>()
        });
        assert_eq!(made, Err(1));
        assert_eq!(catching::<_, ()>(|catch| catch.or(Ok(4), || 0)), Ok(4));
    }
}
