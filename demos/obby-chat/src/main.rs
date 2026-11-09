//! A katui demo: an IRC client drawn in real pixels. Memory only.

mod backdrop;
mod chat;
mod effects;
mod irc_text;
mod net;
mod previews;
mod theme;
mod view;

use std::io;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use katui_draw::TextEngine;
use katui_present::kitty::{Images, KittyScreen, RgbaImage};
use katui_proto::input::{InputEvent, Pointer};
use katui_proto::probe::Capabilities;
use katui_rt::app::{App, AppEvent, Control, run_app};
use katui_rt::events::Waker;
use katui_scene::CellSize;
use katui_scene::placement::{CellRect, ImageId, Placement, PlacementId};
use katui_ui::{CellPixels, Ui, UiOptions};

use crate::chat::{Action, Chat, Message};
use crate::view::view;

const FRAMES_PER_SECOND: u32 = 30;
const TEXT_FONT: &[u8] =
    include_bytes!("../../../crates/katui-draw/assets/AtkinsonHyperlegibleNext[wght].ttf");
const SMALL_EMOJI_FONT: &[u8] =
    include_bytes!("../../../crates/katui-draw/assets/NotoColorEmoji-subset.ttf");
const SYSTEM_FALLBACK_FONT: &str = "/System/Library/Fonts/Supplemental/Arial Unicode.ttf";
const SYSTEM_EMOJI_FONTS: [&str; 2] = [
    "/System/Library/Fonts/Apple Color Emoji.ttc",
    "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
];
/// Interface layers get ids from here up, above the backdrop's frames.
const FIRST_LAYER_IMAGE: u32 = 20_000;
const BACKDROP_IMAGES_FROM: u32 = 100;
const BACKDROP_IMAGE_COUNT: u32 = 19_000;

fn fonts() -> Vec<Vec<u8>> {
    let emoji = SYSTEM_EMOJI_FONTS
        .iter()
        .find_map(|path| std::fs::read(path).ok())
        .unwrap_or_else(|| SMALL_EMOJI_FONT.to_vec());
    let mut fonts = vec![TEXT_FONT.to_vec(), emoji];
    fonts.extend(std::fs::read(SYSTEM_FALLBACK_FONT));
    fonts
}

struct ChatApp {
    caps: Capabilities,
    chat: Chat,
    ui: Ui<Action>,
    screen: KittyScreen,
    cell: CellPixels,
    backdrop_image: u32,
    pointer: Pointer,
}

impl ChatApp {
    fn handle_input(&mut self, event: InputEvent) -> Control {
        if let InputEvent::Mouse(mouse) = event {
            let (x, y) = self.pointer.locate(&mouse, self.cell);
            let hit = self.ui.hit_test(x, y).map(|hit| (hit.id, hit.on_click));
            return self.chat.mouse(mouse, hit);
        }
        self.pointer.observe(&event);
        self.chat.input(event)
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
                self.cell = katui_rt::tty::cell_size(&self.caps);
                self.screen.repaint();
                self.chat.hovered = None;
                self.chat.effects.clear();
                Control::Redraw
            }
            AppEvent::Repaint => {
                self.screen.repaint();
                self.chat.effects.clear();
                Control::Redraw
            }
        }
    }

    fn draw(&mut self, out: &mut Vec<u8>, size: CellSize) -> io::Result<Control> {
        self.pointer.resize(size.columns, size.rows);
        if self.screen.sends_everything() {
            self.chat.backdrop.mark_unsent();
        }
        let (cell_width, cell_height) = (u32::from(self.cell.width), u32::from(self.cell.height));
        let metrics = crate::theme::widgets(self.cell);
        let root = view(&self.chat, &metrics);
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
        let ui = &self.ui;
        let effects = self.chat.effects.frames(
            &output.layer_areas,
            |key| ui.layer_surface(key),
            self.cell,
            Instant::now(),
        );
        let hidden: Vec<CellRect> = effects.iter().filter_map(|effect| effect.hides).collect();
        next.extend(
            output
                .placements
                .iter()
                .filter(|placed| !hidden.iter().any(|area| contains(*area, placed.area)))
                .copied(),
        );
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
        self.screen.present(out, next, &images)?;
        Ok(if self.chat.effects.is_animating() {
            Control::Animate
        } else {
            Control::Wait
        })
    }
}

fn contains(outer: CellRect, inner: CellRect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.columns <= outer.x + outer.columns
        && inner.y + inner.rows <= outer.y + outer.rows
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
        eprintln!("{}", katui_rt::info::no_graphics_advice(&caps));
        std::process::exit(1);
    }
    let options = UiOptions::for_terminal(FIRST_LAYER_IMAGE, caps.multiplexed());
    let mut app = ChatApp {
        caps: caps.clone(),
        chat: Chat::new(default_nick()),
        ui: Ui::new(TextEngine::new(fonts())?, options),
        screen: KittyScreen::new(&caps),
        cell: katui_rt::tty::cell_size(&caps),
        backdrop_image: 0,
        pointer: Pointer::new(caps.cell),
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

    use katui_draw::{Canvas, Rect, Rgba, TextEngine, bundled_fonts};
    use katui_scene::surface::Surface;
    use katui_ui::{CellPixels, Ui, UiOptions};

    use crate::chat::{Action, Chat};
    use crate::effects::{Effects, Kind};
    use crate::view::view;

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
        "@msgid=m5;time=2026-11-08T20:03:30.000Z :operator!u@h PRIVMSG #bots :\u{3}4red \u{3}8,1yellow on black\u{f} \u{2}bold\u{2} \u{1d}italic\u{1d} \u{1f}underlined\u{1f} \u{1e}struck\u{1e} plain",
        "@msgid=m4;+draft/reply=m3;time=2026-11-08T20:03:00.000Z :acme!u@h PRIVMSG #bots :it moves placements, no pixels",
    ];

    fn composed(chat: &Chat) -> Surface {
        composed_with(chat, None, 0)
    }

    /// Compose the screen, playing `effect` on the newest message for a
    /// moment first.
    fn composed_with(chat: &Chat, effect: Option<Kind>, steps: u64) -> Surface {
        let metrics = crate::theme::widgets(CELL);
        let engine = TextEngine::new(bundled_fonts())
            .expect("fonts parse");
        let mut ui: Ui<Action> = Ui::new(engine, UiOptions::default());
        let (width, height) = (9 * 170, 19 * 46);
        let output = ui.render(&view(chat, &metrics), (width, height), CELL);
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
        let frames = effect.map_or_else(Vec::new, |kind| {
            let (_, channel) = chat.active().expect("a conversation is open");
            let newest = channel.log.last().expect("the script sent messages");
            let key = crate::view::message_key(newest);
            let mut effects = Effects::default();
            effects.spawn(key, kind, 0.42, 42);
            let source = ui.layer_surface(key);
            let start = Instant::now();
            (0..steps)
                .map(|step| {
                    effects.frames(
                        &output.layer_areas,
                        |_| source.clone(),
                        CELL,
                        start + Duration::from_millis(step * 16),
                    )
                })
                .last()
                .expect("the range is not empty")
        });
        let hidden: Vec<_> = frames.iter().filter_map(|frame| frame.hides).collect();
        for placed in &output.placements {
            if hidden
                .iter()
                .any(|area| super::contains(*area, placed.area))
            {
                continue;
            }
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
        chat.draft.set_text("/");
        save("menu.png", &composed(&chat));
        chat.draft.take();
        chat.form.visible = true;
        save("overlay.png", &composed(&chat));
        chat.form.visible = false;
        save("pop_early.png", &composed_with(&chat, Some(Kind::Pop), 5));
        save("pop_peak.png", &composed_with(&chat, Some(Kind::Pop), 12));
        save("sparks.png", &composed_with(&chat, Some(Kind::Sparks), 34));
    }

    #[test]
    fn joining_a_channel_asks_for_its_recent_history() {
        let script = [
            ":irc CAP * LS :draft/chathistory batch",
            ":irc CAP * ACK :draft/chathistory batch",
            ":irc 001 acme :Welcome",
            ":acme!u@h JOIN #bots",
        ];
        let mut chat = Chat::new("acme".to_owned());
        let sent = chat.add_scripted_server("acme", &script, "#bots");
        let wire: Vec<u8> = sent.try_iter().flatten().collect();
        let text = String::from_utf8_lossy(&wire);
        assert!(text.contains("CHATHISTORY LATEST #bots * 100"), "{text}");
        assert!(
            !chat.typing_is_live("#bots"),
            "typing replayed by history stays hidden right after a join"
        );
    }

    #[test]
    fn a_long_history_never_pushes_the_composer_off_screen() {
        let mut script: Vec<String> = [":irc 001 acme :Welcome", ":acme!u@h JOIN #bots"]
            .map(str::to_owned)
            .to_vec();
        script.extend((0..90).map(|n| format!(":guest!u@h PRIVMSG #bots :line {n}")));
        let lines: Vec<&str> = script.iter().map(String::as_str).collect();
        let mut chat = Chat::new("acme".to_owned());
        chat.add_scripted_server("acme", &lines, "#bots");
        let metrics = crate::theme::widgets(CELL);
        let engine = TextEngine::new(bundled_fonts())
            .expect("the bundled fonts parse");
        let mut ui: Ui<Action> = Ui::new(engine, UiOptions::default());
        let rows = 46;
        let output = ui.render(&view(&chat, &metrics), (9 * 170, 19 * rows), CELL);
        let composer = output.layer_areas[&crate::view::COMPOSER_KEY];
        assert!(
            u32::from(composer.y + composer.rows) <= rows,
            "the composer ends at row {} of {rows}",
            composer.y + composer.rows
        );
    }

    #[test]
    fn hovering_a_message_redraws_only_that_message() {
        let mut chat = Chat::new("acme".to_owned());
        chat.add_scripted_server("acme", SCRIPT, "#bots");
        let metrics = crate::theme::widgets(CELL);
        let engine = TextEngine::new(bundled_fonts())
            .expect("the bundled fonts parse");
        let mut ui: Ui<Action> = Ui::new(engine, UiOptions::default());
        let screen = (9 * 170, 19 * 46);
        let before = ui.render(&view(&chat, &metrics), screen, CELL);
        let (_, channel) = chat.active().expect("the script joins #bots");
        let key = crate::view::message_key(channel.log.first().expect("the script sent messages"));
        chat.hovered = Some(key);
        let after = ui.render(&view(&chat, &metrics), screen, CELL);
        assert_eq!(after.new_images.len(), 1, "one message changed");
        assert_eq!(before.layer_areas, after.layer_areas, "nothing moved");
    }

    #[test]
    fn version_queries_get_a_notice_and_actions_show_in_the_log() {
        let script = [
            ":irc 001 acme :Welcome",
            ":acme!u@h JOIN #bots",
            ":guest!u@h PRIVMSG #bots :\u{1}VERSION\u{1}",
            ":guest!u@h PRIVMSG #bots :\u{1}ACTION waves\u{1}",
        ];
        let mut chat = Chat::new("acme".to_owned());
        let sent = chat.add_scripted_server("acme", &script, "#bots");
        let wire: Vec<u8> = sent.try_iter().flatten().collect();
        let text = String::from_utf8_lossy(&wire);
        assert!(
            text.contains("NOTICE guest :\u{1}VERSION katui"),
            "the query is answered: {text}"
        );
        let (_, channel) = chat.active().expect("the script joins #bots");
        assert!(channel.log.iter().any(|message| message.text == "waves"));
    }

    #[test]
    fn the_form_appears_before_any_server_exists() {
        let chat = Chat::new("acme".to_owned());
        assert!(chat.active().is_none());
        save("form.png", &composed(&chat));
    }
}
