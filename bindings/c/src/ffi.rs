//! What every function shares: status codes, the last error, and the
//! conversions from C pointers and strings.

use std::borrow::Cow;
use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};

use katui_bindings::ErrorKind;

/// What a function that does something returns: `KATUI_STATUS_OK`, or the
/// kind of failure, with `katui_last_error()` giving the message.
pub type KatuiStatus = i32;
pub const KATUI_STATUS_OK: KatuiStatus = 0;
/// A required pointer was NULL.
pub const KATUI_STATUS_NULL_ARGUMENT: KatuiStatus = 1;
/// A value was out of range or could not be parsed.
pub const KATUI_STATUS_INVALID_ARGUMENT: KatuiStatus = 2;
/// Reading, writing or the terminal failed.
pub const KATUI_STATUS_IO: KatuiStatus = 3;
/// A callback broke its contract, like a view that returned NULL.
pub const KATUI_STATUS_CALLBACK: KatuiStatus = 4;
/// katui hit a bug and stopped it at the boundary.
pub const KATUI_STATUS_PANIC: KatuiStatus = 5;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("pass a {0}, not NULL")]
    Null(&'static str),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Callback(&'static str),
    #[error("katui stopped on a bug: {0}")]
    Panic(String),
}

impl Error {
    fn status(&self) -> KatuiStatus {
        match self {
            Self::Null(_) => KATUI_STATUS_NULL_ARGUMENT,
            Self::Invalid(_) => KATUI_STATUS_INVALID_ARGUMENT,
            Self::Io(_) => KATUI_STATUS_IO,
            Self::Callback(_) => KATUI_STATUS_CALLBACK,
            Self::Panic(_) => KATUI_STATUS_PANIC,
        }
    }
}

/// `error` as the status of its kind.
pub(crate) fn raise(error: katui_bindings::Error) -> Error {
    match error {
        katui_bindings::Error::EmptyScreen => Error::Invalid(error.to_string()),
        other if other.kind() == ErrorKind::Argument => Error::Invalid(other.to_string()),
        other => Error::Io(other.to_string()),
    }
}

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// Keep `error` as this thread's last error and return its status.
pub(crate) fn fail(error: &Error) -> KatuiStatus {
    let message = CString::new(error.to_string().replace('\0', " ")).unwrap_or_default();
    LAST_ERROR.with(|last| *last.borrow_mut() = Some(message));
    error.status()
}

pub(crate) fn status(result: Result<(), Error>) -> KatuiStatus {
    result.map_or_else(|error| fail(&error), |()| KATUI_STATUS_OK)
}

/// `made`, owned by the caller, or NULL with the error kept.
pub(crate) fn owned_or_null<T>(made: Result<T, Error>) -> *mut T {
    match made {
        Ok(value) => owned(value),
        Err(error) => {
            fail(&error);
            std::ptr::null_mut()
        }
    }
}

/// Run `work`, turning a panic into an error, so none crosses into C.
pub(crate) fn guard<T>(work: impl FnOnce() -> Result<T, Error>) -> Result<T, Error> {
    catch_unwind(AssertUnwindSafe(work)).unwrap_or_else(|panic| {
        let reason = panic
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        Err(Error::Panic(reason))
    })
}

pub(crate) fn owned<T>(value: T) -> *mut T {
    Box::into_raw(Box::new(value))
}

/// # Safety
/// `pointer` is NULL or points at a live `T`.
pub(crate) unsafe fn borrow<'a, T>(pointer: *const T) -> Option<&'a T> {
    unsafe { pointer.as_ref() }
}

/// # Safety
/// `pointer` is NULL or points at a live `T` nothing else uses now.
pub(crate) unsafe fn borrow_mut<'a, T>(pointer: *mut T) -> Option<&'a mut T> {
    unsafe { pointer.as_mut() }
}

/// # Safety
/// `pointer` is NULL or came from `owned` and is not used again.
pub(crate) unsafe fn take<T>(pointer: *mut T) -> Option<T> {
    if pointer.is_null() {
        return None;
    }
    Some(*unsafe { Box::from_raw(pointer) })
}

/// # Safety
/// `pointer` is NULL or came from `owned` and is not used again.
pub(crate) unsafe fn free<T>(pointer: *mut T) {
    if !pointer.is_null() {
        drop(unsafe { Box::from_raw(pointer) });
    }
}

/// # Safety
/// `pointer` is NULL or a NUL-terminated string.
pub(crate) unsafe fn text<'a>(pointer: *const c_char) -> Cow<'a, str> {
    if pointer.is_null() {
        return Cow::Borrowed("");
    }
    unsafe { CStr::from_ptr(pointer) }.to_string_lossy()
}

/// # Safety
/// `pointer` is NULL or points at `count` live values.
pub(crate) unsafe fn slice<'a, T>(pointer: *const T, count: usize) -> &'a [T] {
    if pointer.is_null() || count == 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(pointer, count) }
}

/// # Safety
/// `pointer` is NULL or points at `count` strings, each NULL or
/// NUL-terminated.
pub(crate) unsafe fn texts(pointer: *const *const c_char, count: usize) -> Vec<String> {
    let pointers = unsafe { slice(pointer, count) };
    pointers
        .iter()
        .map(|&item| unsafe { text(item) }.into_owned())
        .collect()
}

/// `text` as a string the caller frees with `katui_string_free`.
pub(crate) fn c_string(text: &str) -> *mut c_char {
    CString::new(text.replace('\0', "")).map_or(std::ptr::null_mut(), CString::into_raw)
}

/// The message of this thread's last error, or NULL before the first one.
/// katui owns the string; it stays valid until the next error on this
/// thread.
#[unsafe(no_mangle)]
pub extern "C" fn katui_last_error() -> *const c_char {
    LAST_ERROR.with(|last| {
        last.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |message| message.as_ptr())
    })
}

/// Free a string katui returned. NULL does nothing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_string_free(text: *mut c_char) {
    if !text.is_null() {
        drop(unsafe { CString::from_raw(text) });
    }
}

/// The version of katui, like "0.1.0". katui owns the string.
#[unsafe(no_mangle)]
pub extern "C" fn katui_version() -> *const c_char {
    const VERSION: &CStr =
        match CStr::from_bytes_with_nul(concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes()) {
            Ok(version) => version,
            Err(_) => c"",
        };
    VERSION.as_ptr()
}
