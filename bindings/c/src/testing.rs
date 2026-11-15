//! `KatuiScreen`: an app drawn off-terminal, for tests and screenshots.

use std::ffi::c_char;

use katui_app::Control;
use katui_draw::{TextEngine, bundled_fonts};
use katui_proto::input::{InputEvent, Key, Mods};
use katui_ui::{CellPixels, Ui, UiOptions};

use crate::app::{Bridge, Failure, KatuiApp};
use crate::element::KatuiImage;
use crate::ffi::{
    Error, KatuiStatus, borrow, borrow_mut, free, guard, owned_or_null, status, text,
};
use crate::message::Msg;
use crate::state::KatuiEvent;
use crate::theme::cell;

/// An app drawn into pixels, with clicks and keys sent by hand.
pub struct KatuiScreen {
    bridge: Bridge,
    ui: Ui<Msg>,
    size: (u32, u32),
    cell: CellPixels,
    rendered: bool,
    quit: bool,
    hovered: Option<(usize, Option<Msg>)>,
}

impl KatuiScreen {
    fn checked(&mut self, control: Control) -> Result<(), Error> {
        if let Some(error) = self.bridge.failure() {
            return Err(error);
        }
        if control == Control::Quit {
            self.quit = true;
        }
        Ok(())
    }

    fn deliver(&mut self, message: Option<Msg>) -> Result<(), Error> {
        let Some(message) = message else {
            return Ok(());
        };
        let control = self.bridge.deliver(&message);
        self.checked(control)
    }

    fn ensure_rendered(&mut self) -> Result<(), Error> {
        if self.rendered {
            Ok(())
        } else {
            self.render().map(drop)
        }
    }

    fn render(&mut self) -> Result<KatuiImage, Error> {
        let root = self
            .bridge
            .theme_for(self.cell)
            .scope(|_| self.bridge.render());
        self.checked(Control::Wait)?;
        let output = self.ui.render(&root, self.size, self.cell);
        self.rendered = true;
        for message in self.ui.take_clamped_scrolls() {
            self.deliver(Some(message))?;
        }
        let picture = output
            .flatten(self.cell, self.size)
            .ok_or_else(|| Error::Invalid("draw a screen of at least one pixel".into()))?;
        Ok(KatuiImage(std::sync::Arc::new(picture)))
    }

    fn press(&mut self, x: f32, y: f32) -> Result<(), Error> {
        self.ensure_rendered()?;
        let message = self
            .ui
            .press(x, y)
            .or_else(|| self.ui.hit_test(x, y).and_then(|hit| hit.on_click));
        self.deliver(message)
    }

    fn wheel(&mut self, x: f32, y: f32, down: bool) -> Result<(), Error> {
        self.ensure_rendered()?;
        let message = self.ui.hit_test(x, y).and_then(|hit| {
            if down {
                hit.on_wheel_down
            } else {
                hit.on_wheel_up
            }
        });
        self.deliver(message)
    }

    fn hover(&mut self, x: f32, y: f32) -> Result<(), Error> {
        self.ensure_rendered()?;
        let hit = self.ui.hit_test(x, y);
        let target = hit.as_ref().and_then(|hit| hit.hover_target);
        if self.hovered.as_ref().map(|(index, _)| *index) == target {
            return Ok(());
        }
        let left = self.hovered.take().and_then(|(_, leave)| leave);
        self.deliver(left)?;
        if let Some(hit) = hit {
            if let Some(index) = hit.hover_target {
                self.hovered = Some((index, hit.on_leave));
            }
            self.deliver(hit.on_hover)?;
        }
        Ok(())
    }

    fn send(&mut self, event: &InputEvent) -> Result<(), Error> {
        let control = self.bridge.react(event);
        self.checked(control)
    }
}

/// # Safety
/// `screen` is NULL or a live screen nothing else uses now.
unsafe fn with(
    screen: *mut KatuiScreen,
    act: impl FnOnce(&mut KatuiScreen) -> Result<(), Error>,
) -> KatuiStatus {
    status(guard(|| {
        let screen = unsafe { borrow_mut(screen) }.ok_or(Error::Null("screen"))?;
        act(screen)
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
        let engine = TextEngine::new(bundled_fonts())
            .map_err(|error| Error::Io(format!("load the fonts: {error}")))?;
        Ok(KatuiScreen {
            bridge: Bridge::new(app, Failure::default())?,
            ui: Ui::new(engine, UiOptions::default()),
            size: (width, height),
            cell: cell(cell_width, cell_height),
            rendered: false,
            quit: false,
            hovered: None,
        })
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
        unsafe { borrow_mut(screen) }
            .ok_or(Error::Null("screen"))?
            .render()
    }))
}

/// Click at `x`, `y` pixels and deliver what the click sends.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_screen_click(
    screen: *mut KatuiScreen,
    x: f32,
    y: f32,
) -> KatuiStatus {
    unsafe {
        with(screen, |screen| {
            screen.press(x, y)?;
            screen.ui.release();
            Ok(())
        })
    }
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
    unsafe {
        with(screen, |screen| {
            let message = screen.ui.drag(x, y);
            screen.deliver(message)
        })
    }
}

/// Let go of a held press.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_screen_release(screen: *mut KatuiScreen) -> KatuiStatus {
    unsafe {
        with(screen, |screen| {
            screen.ui.release();
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
    unsafe {
        with(screen, |screen| {
            text.chars().try_for_each(|character| {
                screen.send(&InputEvent::Key(Key::Char(character), Mods::NONE))
            })
        })
    }
}

/// Whether the app asked to quit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_screen_quit(screen: *const KatuiScreen) -> bool {
    unsafe { borrow(screen) }.is_some_and(|screen| screen.quit)
}
