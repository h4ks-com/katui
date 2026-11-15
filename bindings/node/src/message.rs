//! What clicks, drags and timers carry back to JavaScript: the value the app
//! handed over, or a function and the value to call it with.
//!
//! JavaScript values stay in one table object on the JavaScript thread. A
//! message holds only its key, so it can live anywhere in Rust; dropping the
//! last copy queues the key, and the next `hold` on that thread deletes it.

use std::cell::RefCell;
use std::sync::{Arc, Mutex, PoisonError};

use katui_ui::Scroll;
use napi::bindgen_prelude::{
    FnArgs, FromNapiValue, Function, JsObjectValue, Object, ObjectRef, Unknown, ValidateNapiValue,
};
use napi::{Env, JsValue, Result};

use crate::state::ScrollPosition;

type Released = Arc<Mutex<Vec<u64>>>;

struct Table {
    values: ObjectRef<false>,
    next: u64,
    released: Released,
}

thread_local! {
    static TABLE: RefCell<Option<Table>> = const { RefCell::new(None) };
}

/// A JavaScript value kept alive for as long as Rust holds this.
pub(crate) struct Held {
    key: u64,
    released: Released,
}

impl Drop for Held {
    fn drop(&mut self) {
        self.released
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(self.key);
    }
}

pub(crate) type Obj = Arc<Held>;

fn name(key: u64) -> String {
    key.to_string()
}

fn table(env: &Env, slot: &mut Option<Table>) -> Result<()> {
    if slot.is_none() {
        let values = Object::new(env)?.create_ref()?;
        *slot = Some(Table {
            values,
            next: 0,
            released: Released::default(),
        });
    }
    Ok(())
}

/// Keep `value` until every copy of the returned handle is gone.
pub(crate) fn hold(env: &Env, value: Unknown<'_>) -> Result<Obj> {
    TABLE.with_borrow_mut(|slot| {
        table(env, slot)?;
        let Some(table) = slot.as_mut() else {
            return Err(napi::Error::from_reason("keep the value table"));
        };
        let mut values = table.values.get_value(env)?;
        let released = std::mem::take(
            &mut *table
                .released
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for key in released {
            values.delete_named_property(name(key))?;
        }
        table.next += 1;
        values.set_named_property(&name(table.next), value)?;
        Ok(Arc::new(Held {
            key: table.next,
            released: Arc::clone(&table.released),
        }))
    })
}

/// The value `held` keeps, checked to be a `T`.
pub(crate) fn fetch<'env, T>(env: &'env Env, held: &Held) -> Result<T>
where
    T: FromNapiValue + ValidateNapiValue + 'env,
{
    TABLE.with_borrow(|slot| {
        let table = slot
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("hold a value before reading it"))?;
        table
            .values
            .get_value(env)?
            .get_named_property(&name(held.key))
    })
}

pub(crate) fn unknown<'env>(env: &'env Env, held: &Held) -> Result<Unknown<'env>> {
    fetch(env, held)
}

#[derive(Clone)]
pub(crate) enum Msg {
    Value(Obj),
    Index(Obj, u32),
    Id(Obj, u32),
    Float(Obj, f64),
    Point(Obj, f64, f64),
    Scroll(Obj, Scroll),
    Link(Obj, String),
    Href(String),
    /// JavaScript threw while we drew, so the app has to end.
    Stop,
}

impl Msg {
    /// The message as the app's `update` receives it; `None` for [`Msg::Stop`].
    pub(crate) fn resolve<'env>(&self, env: &'env Env) -> Result<Option<Unknown<'env>>> {
        let called = match self {
            Self::Value(value) => return unknown(env, value).map(Some),
            Self::Stop => return Ok(None),
            Self::Href(href) => return Ok(Some(env.create_string(href)?.to_unknown())),
            Self::Index(make, index) | Self::Id(make, index) => {
                fetch::<Make<'_, u32>>(env, make)?.call(*index)
            }
            Self::Float(make, value) => fetch::<Make<'_, f64>>(env, make)?.call(*value),
            Self::Point(make, x, y) => {
                fetch::<Make<'_, FnArgs<(f64, f64)>>>(env, make)?.call((*x, *y).into())
            }
            Self::Scroll(make, scroll) => {
                fetch::<Make<'_, ScrollPosition>>(env, make)?.call((*scroll).into())
            }
            Self::Link(make, href) => fetch::<Make<'_, String>>(env, make)?.call(href.clone()),
        };
        called.map(Some)
    }
}

/// A function that makes a message from what a widget reports.
type Make<'env, Args> = Function<'env, Args, Unknown<'env>>;
