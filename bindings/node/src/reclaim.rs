//! Frees the trees of dead elements while `run` blocks the event loop.
//!
//! Node runs finalizers from the event loop, and V8 hardly ever starts a
//! full collection while `run` blocks it, so every element a view makes
//! would keep its tree until the app quits. We keep a weak reference to each
//! element we take in, ask for a full collection once enough of them pile
//! up, and empty the tree of each element the collector freed.

use std::cell::RefCell;
use std::ptr;
use std::rc::Rc;

use napi::bindgen_prelude::{Function, Unknown};
use napi::{Env, JsValue, Result, Status, sys};

use crate::element::Node;
use crate::message::{Obj, fetch, hold};

/// An element's tree, `None` once its element is gone.
pub(crate) type Tree = Rc<RefCell<Option<Box<Node>>>>;

/// How many watched elements may pile up before we ask for a collection.
const PILE: usize = 20_000;

struct Watched {
    reference: sys::napi_ref,
    tree: Tree,
}

thread_local! {
    static WATCHED: RefCell<Vec<Watched>> = const { RefCell::new(Vec::new()) };
    static LIVE_AFTER_COLLECTION: RefCell<usize> = const { RefCell::new(0) };
}

/// Whether so many elements wait that a full collection is worth its time.
fn due() -> bool {
    let live = LIVE_AFTER_COLLECTION.with_borrow(|live| *live);
    WATCHED.with_borrow(Vec::len) > PILE.max(live * 2)
}

/// V8's full collection, which Node hides unless we ask for it.
const COLLECTOR: &str = "(() => {
    process.getBuiltinModule('node:v8').setFlagsFromString('--expose-gc');
    return process.getBuiltinModule('node:vm').runInNewContext('gc');
})()";

/// The function that runs a full collection.
pub(crate) fn collector(env: &Env) -> Result<Obj> {
    let collect: Unknown<'_> = env.run_script(COLLECTOR)?;
    hold(env, collect)
}

/// Run a full collection when enough elements wait, then free what it found.
pub(crate) fn reclaim(env: &Env, collect: &Obj) -> Result<()> {
    if !due() {
        sweep(env);
        return Ok(());
    }
    fetch::<Function<'_, (), Unknown<'_>>>(env, collect)?.call(())?;
    sweep(env);
    let live = WATCHED.with_borrow(Vec::len);
    LIVE_AFTER_COLLECTION.with_borrow_mut(|after| *after = live);
    Ok(())
}

/// Empty `tree` once the collector frees `owner`.
pub(crate) fn watch(env: &Env, owner: &Unknown<'_>, tree: Tree) -> Result<()> {
    let mut reference = ptr::null_mut();
    // SAFETY: `env` and `owner` are live handles of the current call.
    let status =
        unsafe { sys::napi_create_reference(env.raw(), owner.raw(), 0, &raw mut reference) };
    if status != sys::Status::napi_ok {
        return Err(napi::Error::new(
            Status::from(status),
            "watch an element".to_owned(),
        ));
    }
    WATCHED.with_borrow_mut(|watched| watched.push(Watched { reference, tree }));
    Ok(())
}

/// Empty the trees of the elements the collector has freed.
pub(crate) fn sweep(env: &Env) {
    WATCHED.with_borrow_mut(|watched| watched.retain(|entry| alive(env, entry)));
}

fn alive(env: &Env, entry: &Watched) -> bool {
    let mut value = ptr::null_mut();
    // SAFETY: `entry.reference` was made on this thread's env and not deleted yet.
    let status =
        unsafe { sys::napi_get_reference_value(env.raw(), entry.reference, &raw mut value) };
    if status != sys::Status::napi_ok || !value.is_null() {
        return true;
    }
    // SAFETY: the reference is ours, and we drop the entry that held it.
    unsafe { sys::napi_delete_reference(env.raw(), entry.reference) };
    entry.tree.borrow_mut().take();
    false
}
