//! `KatuiScreen`: an app drawn off-terminal, for tests and screenshots.

use std::ffi::c_char;
use std::sync::Arc;

use katui_bindings::screen::Screen;

use crate::app::{CApp, KatuiApp};
use crate::element::KatuiImage;
use crate::event::KatuiEvent;
use crate::ffi::{
    Error, KatuiStatus, borrow, borrow_mut, free, guard, owned_or_null, status, text,
};
use crate::theme::cell;

/// An app drawn into pixels, with clicks and keys sent by hand.
pub struct KatuiScreen(Screen<CApp>);

/// # Safety
/// `screen` is NULL or a live screen nothing else uses now.
unsafe fn with(
    screen: *mut KatuiScreen,
    act: impl FnOnce(&mut Screen<CApp>) -> Result<(), Error>,
) -> KatuiStatus {
    status(guard(|| {
        let screen = unsafe { borrow_mut(screen) }.ok_or(Error::Null("screen"))?;
        act(&mut screen.0)
    }))
}

/// `app` on a screen of `width` by `height` pixels in cells of
/// `cell_width` by `cell_height`. `app` is copied; `user` has to live as
/// long as the screen. NULL when `app` lacks `view` or `update`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_screen_new(
    app: *const KatuiApp,
    width: u32,
    height: u32,
    cell_width: u16,
    cell_height: u16,
) -> *mut KatuiScreen {
    let made = guard(|| {
        let app = unsafe { borrow(app) }.ok_or(Error::Null("app"))?;
        let cell = cell(cell_width, cell_height);
        Screen::new(CApp::new(app)?, (width, height), cell).map(KatuiScreen)
    });
    owned_or_null(made)
}

/// Free a screen. NULL does nothing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_screen_free(screen: *mut KatuiScreen) {
    unsafe { free(screen) };
}

/// Draw the app's view and return the picture, which the caller owns. NULL
/// when a callback failed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_screen_render(screen: *mut KatuiScreen) -> *mut KatuiImage {
    owned_or_null(guard(|| {
        let screen = unsafe { borrow_mut(screen) }.ok_or(Error::Null("screen"))?;
        screen
            .0
            .render()
            .map(|picture| KatuiImage(Arc::new(picture)))
    }))
}

/// Click at `x`, `y` pixels and deliver what the click sends.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_screen_click(
    screen: *mut KatuiScreen,
    x: f32,
    y: f32,
) -> KatuiStatus {
    unsafe { with(screen, |screen| screen.click(x, y)) }
}

/// Press at `x`, `y` and keep holding, for a drag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_screen_press(
    screen: *mut KatuiScreen,
    x: f32,
    y: f32,
) -> KatuiStatus {
    unsafe { with(screen, |screen| screen.press(x, y)) }
}

/// Move a held press to `x`, `y`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_screen_drag(
    screen: *mut KatuiScreen,
    x: f32,
    y: f32,
) -> KatuiStatus {
    unsafe { with(screen, |screen| screen.drag(x, y)) }
}

/// Let go of a held press.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_screen_release(screen: *mut KatuiScreen) -> KatuiStatus {
    unsafe {
        with(screen, |screen| {
            screen.release();
            Ok(())
        })
    }
}

/// Turn the wheel once at `x`, `y`, down or up.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_screen_wheel(
    screen: *mut KatuiScreen,
    x: f32,
    y: f32,
    down: bool,
) -> KatuiStatus {
    unsafe { with(screen, |screen| screen.wheel(x, y, down)) }
}

/// Move the pointer to `x`, `y`: the element it left gets its leave
/// message, the one it entered its hover message.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_screen_hover(
    screen: *mut KatuiScreen,
    x: f32,
    y: f32,
) -> KatuiStatus {
    unsafe { with(screen, |screen| screen.hover(x, y)) }
}

/// Hand `event`, which stays the caller's, to the app's `input`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_screen_send(
    screen: *mut KatuiScreen,
    event: *const KatuiEvent,
) -> KatuiStatus {
    let Some(event) = (unsafe { borrow(event) }) else {
        return status(Err(Error::Null("event")));
    };
    unsafe { with(screen, |screen| screen.send(&event.0)) }
}

/// Type `text`, one key press per character.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_screen_type_text(
    screen: *mut KatuiScreen,
    text: *const c_char,
) -> KatuiStatus {
    let text = unsafe { self::text(text) };
    unsafe { with(screen, |screen| screen.type_text(&text)) }
}

/// Whether the app asked to quit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_screen_quit(screen: *const KatuiScreen) -> bool {
    unsafe { borrow(screen) }.is_some_and(|screen| screen.0.quit())
}
