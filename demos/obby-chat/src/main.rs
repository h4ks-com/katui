//! A katui demo: an IRC client drawn in real pixels. Memory only.

mod backdrop;
mod chat;
mod effects;
mod net;
mod previews;
mod theme;
mod view;

use std::io;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use katui_draw::TextEngine;
use katui_present::kitty::{Images, Kitty, RgbaImage};
use katui_proto::input::InputEvent;
use katui_proto::probe::Capabilities;
use katui_rt::app::{App, AppEvent, Control, run_app};
use katui_rt::events::Waker;
use katui_scene::CellSize;
use katui_scene::placement::{self, CellRect, ImageId, Placement, PlacementId};
use katui_ui::{CellPixels, Ui, UiOptions};

use crate::chat::{Action, Chat, Message};
use crate::view::{Metrics, view};

const FRAMES_PER_SECOND: u32 = 30;
const TEXT_FONT: &[u8] =
    include_bytes!("../../../crates/katui-draw/assets/AtkinsonHyperlegibleNext[wght].ttf");
const SMALL_EMOJI_FONT: &[u8] =
    include_bytes!("../../../crates/katui-draw/assets/NotoColorEmoji-subset.ttf");
const SYSTEM_EMOJI_FONTS: [&str; 2] = [
    "/System/Library/Fonts/Apple Color Emoji.ttc",
    "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
];
/// Interface layers get ids from here up, above the backdrop's frames.
const FIRST_LAYER_IMAGE: u32 = 20_000;
const BACKDROP_IMAGES_FROM: u32 = 100;
const BACKDROP_IMAGE_COUNT: u32 = 19_000;
/// A size that keeps every upload inside what a multiplexer can carry.
const MULTIPLEXER_LAYER_CELLS: (u16, u16) = (40, 16);

fn fonts() -> Vec<Vec<u8>> {
    let emoji = SYSTEM_EMOJI_FONTS
        .iter()
        .find_map(|path| std::fs::read(path).ok())
        .unwrap_or_else(|| SMALL_EMOJI_FONT.to_vec());
    vec![TEXT_FONT.to_vec(), emoji]
}

struct ChatApp {
    caps: Capabilities,
    chat: Chat,
    ui: Ui<Action>,
    kitty: Kitty,
    multiplexed: bool,
    cell: CellPixels,
    placements: Vec<Placement>,
    backdrop_image: u32,
}

impl ChatApp {
    fn pointer(&self, x: u32, y: u32) -> (f32, f32) {
        if self.chat.pixel_mouse {
            (x as f32, y as f32)
        } else {
            let (width, height) = (f32::from(self.cell.width), f32::from(self.cell.height));
            ((x as f32 + 0.5) * width, (y as f32 + 0.5) * height)
        }
    }

    fn handle_input(&mut self, event: InputEvent) -> Control {
        if let InputEvent::Mouse(mouse) = event {
            let (x, y) = self.pointer(mouse.x, mouse.y);
            let hit = self.ui.hit_test(x, y).map(|hit| (hit.id, hit.on_click));
            return self.chat.mouse(mouse, hit);
        }
        self.chat.input(event)
    }

    /// Forget what the terminal holds, so the next draw sends it all.
    fn resend_everything(&mut self) {
        self.placements.clear();
        self.kitty = Kitty::new().for_multiplexer(self.multiplexed);
        self.chat.backdrop.mark_unsent();
    }

    fn backdrop_placement(&mut self, size: CellSize) -> Placement {
        Placement {
            image: ImageId(BACKDROP_IMAGES_FROM + self.backdrop_image),
            id: PlacementId(1),
            area: CellRect::full(size.columns, size.rows),
            crop: None,
            z: 0,
        }
    }
}

impl App for ChatApp {
    type Message = Message;

    fn start(&mut self, waker: Waker<Message>) {
        self.chat.attach(waker);
    }

    fn update(&mut self, event: AppEvent<Message>) -> Control {
        match event {
            AppEvent::Input(input) => self.handle_input(input),
            AppEvent::Message(message) => self.chat.receive(message),
            AppEvent::Resized(_) => {
                self.cell = cell_pixels(&self.caps);
                Control::Redraw
            }
            AppEvent::Settled => {
                self.resend_everything();
                Control::Redraw
            }
        }
    }

    fn draw(&mut self, out: &mut Vec<u8>, size: CellSize) -> io::Result<Control> {
        let (cell_width, cell_height) = (u32::from(self.cell.width), u32::from(self.cell.height));
        let metrics = Metrics {
            cell_width: f32::from(self.cell.width),
            cell_height: f32::from(self.cell.height),
        };
        let root = view(&self.chat, metrics);
        let surface = (
            u32::from(size.columns) * cell_width,
            u32::from(size.rows) * cell_height,
        );
        let output = self.ui.render(&root, surface, self.cell);

        self.chat
            .backdrop
            .resize(u32::from(size.columns) / 2, u32::from(size.rows));
        let fresh = self
            .chat
            .backdrop
            .frame()
            .map(|(pixels, width, height)| (pixels.to_vec(), width, height));
        if fresh.is_some() {
            self.backdrop_image = (self.backdrop_image + 1) % BACKDROP_IMAGE_COUNT;
        }
        let mut next = vec![self.backdrop_placement(size)];
        next.extend(output.placements.iter().copied());
        let effects = self
            .chat
            .effects
            .frames(&output.layer_areas, Instant::now());
        next.extend(effects.iter().map(|effect| effect.placement));

        let mut images = Images::default();
        if let Some((pixels, width, height)) = &fresh
            && let Some(image) = RgbaImage::new(*width, *height, pixels)
        {
            images.insert(next[0].image, image);
        }
        for effect in &effects {
            if let Some(image) = RgbaImage::new(effect.width, effect.height, &effect.pixels) {
                images.insert(effect.placement.image, image);
            }
        }
        for layer in &output.images {
            if let Some(image) = RgbaImage::new(layer.width(), layer.height(), layer.rgba()) {
                images.insert(layer.id, image);
            }
        }
        let delta = placement::diff(&self.placements, &next);
        self.kitty.render(out, &delta, &images)?;
        self.placements = next;
        Ok(if self.chat.effects.is_animating() {
            Control::Animate
        } else {
            Control::Wait
        })
    }
}

fn cell_pixels(caps: &Capabilities) -> CellPixels {
    caps.cell
        .or_else(katui_rt::tty::measured_cell)
        .unwrap_or_else(|| katui_rt::tty::pixel_cell(caps))
}

fn default_nick() -> String {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.subsec_millis());
    format!("katui{suffix}")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let caps = katui_rt::tty::probe_default();
    if !caps.kitty {
        return Err("this demo needs a terminal with the kitty graphics protocol".into());
    }
    let multiplexed = caps.multiplexed();
    let options = UiOptions {
        first_image_id: FIRST_LAYER_IMAGE,
        max_layer_cells: multiplexed.then_some(MULTIPLEXER_LAYER_CELLS),
    };
    let mut app = ChatApp {
        caps: caps.clone(),
        chat: Chat::new(default_nick()),
        ui: Ui::new(TextEngine::new(fonts())?, options),
        kitty: Kitty::new().for_multiplexer(multiplexed),
        multiplexed,
        cell: cell_pixels(&caps),
        placements: Vec::new(),
        backdrop_image: 0,
    };
    if std::env::args().any(|argument| argument == "--connect") {
        app.chat.connect_on_start();
    }
    run_app(&mut app, io::stdout(), &caps, FRAMES_PER_SECOND)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use katui_draw::{Canvas, Rect, Rgba, TextEngine};
    use katui_scene::surface::Surface;
    use katui_ui::{CellPixels, Ui, UiOptions};

    use super::{SMALL_EMOJI_FONT, TEXT_FONT};
    use crate::chat::{Action, Chat};
    use crate::effects::{Effects, Kind};
    use crate::view::{Metrics, view};

    const CELL: CellPixels = CellPixels {
        width: 9,
        height: 19,
    };

    /// A real registration, join, conversation, reply and reaction, as
    /// the server sends them.
    const SCRIPT: &[&str] = &[
        ":irc CAP * LS :message-tags server-time echo-message",
        ":irc CAP * ACK :message-tags server-time echo-message",
        ":irc 001 acme :Welcome",
        ":acme!u@h JOIN #bots",
        ":irc 332 acme #bots :Flood is allowed",
        ":irc 353 acme = #bots :@acme +reviewer operator guest",
        ":irc 366 acme #bots :End",
        "@msgid=m1;time=2026-11-08T20:00:00.000Z :reviewer!u@h PRIVMSG #bots :the build is green on every terminal we tried",
        "@msgid=m2;time=2026-11-08T20:01:00.000Z :operator!u@h PRIVMSG #bots :text this sharp in a terminal still feels illegal, and it keeps wrapping properly once a line gets long enough to reach the far edge of the pane",
        "@msgid=m3;time=2026-11-08T20:02:00.000Z :guest!u@h PRIVMSG #bots :can it scroll",
        "@+draft/react=\u{1F525};+draft/reply=m2 :reviewer!u@h TAGMSG #bots",
        "@msgid=m4;+draft/reply=m3;time=2026-11-08T20:03:00.000Z :acme!u@h PRIVMSG #bots :it moves placements, no pixels",
    ];

    fn composed(chat: &Chat) -> Surface {
        composed_with(chat, None)
    }

    /// Compose the screen, playing `effect` on the newest message for a
    /// moment first.
    fn composed_with(chat: &Chat, effect: Option<Kind>) -> Surface {
        let metrics = Metrics {
            cell_width: 9.0,
            cell_height: 19.0,
        };
        let engine = TextEngine::new(vec![TEXT_FONT.to_vec(), SMALL_EMOJI_FONT.to_vec()])
            .expect("fonts parse");
        let mut ui: Ui<Action> = Ui::new(engine, UiOptions::default());
        let (width, height) = (9 * 170, 19 * 46);
        let output = ui.render(&view(chat, metrics), (width, height), CELL);
        let mut canvas = Canvas::new(width, height).expect("canvas");
        canvas.clear(Rgba::solid(27, 24, 38));
        let mut backdrop = crate::backdrop::Backdrop::default();
        backdrop.resize(85, 46);
        if let Some((pixels, w, h)) = backdrop.frame() {
            let surface = Surface::from_rgba(w, h, pixels.to_vec()).expect("exact pixels");
            canvas.draw_surface(
                &surface,
                Rect::new(0.0, 0.0, width as f32, height as f32),
                0.0,
            );
        }
        for placed in &output.placements {
            let layer = output
                .images
                .iter()
                .find(|image| image.id == placed.image)
                .expect("placed image exists");
            let surface = Surface::from_rgba(layer.width(), layer.height(), layer.rgba().to_vec())
                .expect("exact");
            let rect = Rect::new(
                f32::from(placed.area.x) * 9.0,
                f32::from(placed.area.y) * 19.0,
                f32::from(placed.area.columns) * 9.0,
                f32::from(placed.area.rows) * 19.0,
            );
            canvas.draw_surface(&surface, rect, 0.0);
        }
        if let Some(kind) = effect {
            let (_, channel) = chat.active().expect("a conversation is open");
            let newest = channel.log.last().expect("the script sent messages");
            let mut effects = Effects::default();
            effects.spawn(crate::view::message_key(newest), kind, 0.42, 42);
            let start = Instant::now();
            let frames = (0..12)
                .map(|step| {
                    effects.frames(
                        &output.layer_areas,
                        start + Duration::from_millis(step * 45),
                    )
                })
                .last()
                .expect("the range is not empty");
            for frame in frames {
                let surface =
                    Surface::from_rgba(frame.width, frame.height, frame.pixels).expect("exact");
                let area = frame.placement.area;
                let rect = Rect::new(
                    f32::from(area.x) * 9.0,
                    f32::from(area.y) * 19.0,
                    f32::from(area.columns) * 9.0,
                    f32::from(area.rows) * 19.0,
                );
                canvas.draw_surface(&surface, rect, 0.0);
            }
        }
        canvas.to_surface().expect("surface")
    }

    fn save(name: &str, surface: &Surface) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/screens");
        std::fs::create_dir_all(&dir).expect("artifact directory");
        image::save_buffer(
            dir.join(name),
            surface.pixels(),
            surface.width(),
            surface.height(),
            image::ExtendedColorType::Rgba8,
        )
        .expect("artifact png");
    }

    #[test]
    fn the_conversation_composes_from_real_server_lines() {
        let mut chat = Chat::new("acme".to_owned());
        chat.add_scripted_server("acme", SCRIPT, "#bots");
        let (server, channel) = chat.active().expect("the script joins #bots");
        assert_eq!(channel.members.len(), 4);
        assert!(channel.log.len() >= 4, "{} messages", channel.log.len());
        assert_eq!(
            server.member_names(channel)[0].1,
            "acme",
            "the op ranks first"
        );
        save("conversation.png", &composed(&chat));
        save("fire.png", &composed_with(&chat, Some(Kind::Fire)));
        save("sparks.png", &composed_with(&chat, Some(Kind::Sparks)));
    }

    #[test]
    fn the_form_appears_before_any_server_exists() {
        let chat = Chat::new("acme".to_owned());
        assert!(chat.active().is_none());
        save("form.png", &composed(&chat));
    }
}
