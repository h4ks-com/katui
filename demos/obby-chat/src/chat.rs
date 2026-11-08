//! The chat state and what events do to it. Memory only.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use katui_proto::input::{Button, InputEvent, Key, Mods, MouseAction, MouseEvent};
use katui_rt::app::Control;
use katui_rt::events::Waker;
use katui_scene::surface::Surface;
use obby_client::{Change, Channel, Client, Config, Event, Now, Phase, Typing};

use crate::backdrop::Backdrop;
use crate::effects::{Effects, Kind};
use crate::net::{self, Connection, Endpoint, NetEvent};
use crate::previews;
use crate::theme::nick_hue;

const TYPING_REFRESH_MS: u64 = 3_000;
const TICK: std::time::Duration = std::time::Duration::from_millis(350);
const AVATAR_COLUMNS: u16 = 10;
const SPARK_COLUMNS: u16 = 60;
const MAX_FETCHES: usize = 3;
const MAX_PREVIEWS: usize = 200;
const WHEEL_MESSAGES: usize = 3;
const PAGE_MESSAGES: usize = 12;

#[derive(Debug)]
pub enum Message {
    Net(usize, NetEvent),
    Tick,
    Preview(String, Option<Arc<Surface>>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    SelectServer(usize),
    SelectChannel(usize, String),
    ShowForm,
    FocusField(Field),
    Connect,
    Hover(u64),
    React(String, String, String),
    Reply(String, String),
    CancelReply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Host,
    Nick,
    Channel,
}

pub struct Form {
    pub visible: bool,
    pub focus: Field,
    pub host: String,
    pub nick: String,
    pub channel: String,
}

/// An image preview's progress.
pub enum Preview {
    Loading,
    Ready(Arc<Surface>),
    Failed,
}

pub struct Server {
    pub endpoint: Endpoint,
    pub client: Client,
    connection: Connection,
    join_after_register: Option<String>,
    closed: Option<String>,
}

impl Server {
    /// What the connection is doing, in a few words.
    pub fn status(&self) -> String {
        if let Some(reason) = &self.closed {
            return format!("Disconnected: {reason}");
        }
        match self.client.phase() {
            Phase::Registered => format!("Joining as {}", self.client.nick()),
            Phase::Disconnected => format!("Connecting to {}", self.endpoint.host),
            _ => format!("Signing in to {}", self.endpoint.host),
        }
    }

    /// Whether the connection is still on its way to a joined channel.
    pub fn is_connecting(&self) -> bool {
        self.closed.is_none()
    }

    /// The channels we are in, by name.
    pub fn channels(&self) -> Vec<&Channel> {
        let mut channels: Vec<&Channel> = self
            .client
            .model()
            .channels()
            .map(|(_, channel)| channel)
            .collect();
        channels.sort_by(|left, right| left.name.cmp(&right.name));
        channels
    }

    fn channel(&self, name: &str) -> Option<&Channel> {
        self.client
            .model()
            .channels()
            .map(|(_, channel)| channel)
            .find(|channel| channel.name == name)
    }

    /// Each member's highest prefix and nick, ranked then alphabetical.
    pub fn member_names(&self, channel: &Channel) -> Vec<(String, String)> {
        let model = self.client.model();
        let mut names: Vec<(String, String)> = channel
            .members
            .iter()
            .map(|(key, membership)| {
                let nick = model
                    .person(key)
                    .map_or_else(|| key.to_string(), |person| person.nick.clone());
                (membership.prefixes.chars().take(1).collect(), nick)
            })
            .collect();
        names.sort_by_key(|(prefix, nick): &(String, String)| {
            (prefix.is_empty(), nick.to_lowercase())
        });
        names
    }

    fn flush(&mut self) {
        while let Some(bytes) = self.client.poll_transmit() {
            self.connection.send(bytes);
        }
    }
}

pub struct Chat {
    pub servers: Vec<Server>,
    selection: Option<(usize, Option<String>)>,
    pub form: Form,
    pub draft: String,
    pub replying_to: Option<(String, String)>,
    pub hovered: Option<u64>,
    pub scroll: usize,
    pub previews: HashMap<String, Preview>,
    pub backdrop: Backdrop,
    pub effects: Effects,
    pub pixel_mouse: bool,
    waker: Option<Waker<Message>>,
    started: Instant,
    last_typing_ms: u64,
    connect_when_attached: bool,
}

/// How wide a message's text runs, avatar included, in cells.
fn effect_columns(text: &str) -> u16 {
    u16::try_from(text.chars().count())
        .unwrap_or(u16::MAX)
        .saturating_add(AVATAR_COLUMNS)
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

impl Chat {
    pub fn new(nick: String) -> Self {
        Self {
            servers: Vec::new(),
            selection: None,
            form: Form {
                visible: true,
                focus: Field::Host,
                host: "irc.h4ks.com:6697".to_owned(),
                nick,
                channel: "#bots".to_owned(),
            },
            draft: String::new(),
            replying_to: None,
            hovered: None,
            scroll: 0,
            previews: HashMap::new(),
            backdrop: Backdrop::default(),
            effects: Effects::default(),
            pixel_mouse: false,
            waker: None,
            started: Instant::now(),
            last_typing_ms: 0,
            connect_when_attached: false,
        }
    }

    pub fn attach(&mut self, waker: Waker<Message>) {
        let ticker = waker.clone();
        std::thread::spawn(move || {
            while ticker.send(Message::Tick) {
                std::thread::sleep(TICK);
            }
        });
        self.waker = Some(waker);
        if self.connect_when_attached {
            self.connect();
        }
    }

    /// Which dot of the connecting spinner is lit.
    pub fn spinner_step(&self) -> usize {
        (self.started.elapsed().as_millis() / TICK.as_millis()) as usize % crate::view::SPINNER_DOTS
    }

    pub fn now_ms(&self) -> u64 {
        unix_ms()
    }

    pub fn selected_server(&self) -> Option<usize> {
        self.selection.as_ref().map(|(server, _)| *server)
    }

    pub fn is_selected(&self, server: usize, channel: &str) -> bool {
        self.selection
            .as_ref()
            .is_some_and(|(index, name)| *index == server && name.as_deref() == Some(channel))
    }

    /// The server and channel on screen.
    pub fn active(&self) -> Option<(&Server, &Channel)> {
        let (index, name) = self.selection.as_ref()?;
        let server = self.servers.get(*index)?;
        Some((server, server.channel(name.as_ref()?)?))
    }

    fn active_target(&self) -> Option<(usize, String)> {
        let (index, name) = self.selection.as_ref()?;
        Some((*index, name.clone()?))
    }

    /// Connect to the form defaults as soon as the event loop is ready.
    pub fn connect_on_start(&mut self) {
        self.connect_when_attached = true;
    }

    pub fn connect(&mut self) {
        let Some(waker) = self.waker.clone() else {
            return;
        };
        let (host, port) = self
            .form
            .host
            .rsplit_once(':')
            .and_then(|(host, port)| Some((host.to_owned(), port.parse().ok()?)))
            .unwrap_or_else(|| (self.form.host.clone(), 6697));
        let endpoint = Endpoint {
            host,
            port,
            tls: port != 6667,
        };
        let index = self.servers.len();
        let connection = net::connect(endpoint.clone(), move |event| {
            let _ = waker.send(Message::Net(index, event));
        });
        self.servers.push(Server {
            endpoint,
            client: Client::new(Config::new(self.form.nick.clone())),
            connection,
            join_after_register: Some(self.form.channel.clone()).filter(|name| !name.is_empty()),
            closed: None,
        });
        self.selection = Some((index, None));
        self.form.visible = false;
    }

    pub fn receive(&mut self, message: Message) -> Control {
        match message {
            Message::Net(index, event) => self.net_event(index, event),
            Message::Tick => self.tick(),
            Message::Preview(url, surface) => {
                self.previews
                    .insert(url, surface.map_or(Preview::Failed, Preview::Ready));
                Control::Redraw
            }
        }
    }

    fn tick(&mut self) -> Control {
        let (monotonic_ms, now) = (self.started.elapsed().as_millis() as u64, unix_ms());
        let mut changed = false;
        for index in 0..self.servers.len() {
            self.servers[index].client.tick(Now {
                monotonic_ms,
                unix_ms: now,
            });
            changed |= self.drain_events(index);
        }
        if changed {
            Control::Redraw
        } else {
            Control::Wait
        }
    }

    fn net_event(&mut self, index: usize, event: NetEvent) -> Control {
        let Some(server) = self.servers.get_mut(index) else {
            return Control::Wait;
        };
        match event {
            NetEvent::Connected => server.client.handle_connected(),
            NetEvent::Bytes(bytes) => server.client.handle_bytes(&bytes),
            NetEvent::Closed(reason) => {
                server.client.handle_disconnected();
                server.closed = Some(reason);
            }
        }
        self.drain_events(index);
        Control::Redraw
    }

    fn drain_events(&mut self, index: usize) -> bool {
        let mut changed = false;
        while let Some(event) = self.servers[index].client.poll_event() {
            changed = true;
            match event {
                Event::Registered { .. } => {
                    let server = &mut self.servers[index];
                    if let Some(channel) = server.join_after_register.take() {
                        server.client.join(channel, None);
                    }
                }
                Event::ModelChanged { change } => self.apply_change(index, change),
                _ => {}
            }
        }
        self.servers[index].flush();
        changed
    }

    fn apply_change(&mut self, index: usize, change: Change) {
        match change {
            Change::ChannelJoined { channel } => {
                if self.selection == Some((index, None)) {
                    self.selection = Some((index, Some(channel)));
                }
            }
            Change::MessageAdded { target, key } => self.message_landed(index, &target, &key),
            Change::MessageReacted { target, msgid } => {
                self.message_reacted(index, &target, &msgid)
            }
            _ => {}
        }
    }

    fn message_landed(&mut self, index: usize, target: &str, key: &obby_client::MessageKey) {
        let Some(message) = self.servers[index]
            .channel(target)
            .and_then(|channel| channel.log.get_by_key(key))
        else {
            return;
        };
        let (historical, hue, layer, columns, urls) = (
            message.historical,
            nick_hue(&message.sender),
            crate::view::message_key(message),
            effect_columns(&message.text),
            message
                .link_preview
                .as_ref()
                .and_then(|preview| preview.image.clone())
                .or_else(|| previews::image_url(&message.text).map(str::to_owned)),
        );
        if let Some(url) = urls {
            self.request_preview(url);
        }
        if !historical && self.is_selected(index, target) {
            self.effects.spawn(layer, Kind::Fire, hue, columns);
        }
    }

    fn message_reacted(&mut self, index: usize, target: &str, msgid: &str) {
        if !self.is_selected(index, target) {
            return;
        }
        let found = self.servers[index]
            .channel(target)
            .and_then(|channel| channel.log.get(msgid));
        if let Some(message) = found {
            let (layer, hue) = (crate::view::message_key(message), nick_hue(&message.sender));
            self.effects.spawn(layer, Kind::Sparks, hue, SPARK_COLUMNS);
        }
    }

    fn request_preview(&mut self, url: String) {
        let Some(waker) = self.waker.clone() else {
            return;
        };
        let fetching = self
            .previews
            .values()
            .filter(|preview| matches!(preview, Preview::Loading))
            .count();
        if self.previews.contains_key(&url)
            || fetching >= MAX_FETCHES
            || self.previews.len() >= MAX_PREVIEWS
        {
            return;
        }
        self.previews.insert(url.clone(), Preview::Loading);
        previews::fetch(url, move |url, surface| {
            let _ = waker.send(Message::Preview(url, surface));
        });
    }

    /// React to a click or hover target.
    pub fn perform(&mut self, action: Action) -> Control {
        match action {
            Action::SelectServer(index) => {
                let first = self.servers[index]
                    .channels()
                    .first()
                    .map(|channel| channel.name.clone());
                self.selection = Some((index, first));
                self.scroll = 0;
                self.replying_to = None;
            }
            Action::SelectChannel(server, name) => {
                self.selection = Some((server, Some(name)));
                self.scroll = 0;
                self.replying_to = None;
            }
            Action::ShowForm => self.form.visible = !self.form.visible,
            Action::FocusField(field) => self.form.focus = field,
            Action::Connect => self.connect(),
            Action::Hover(key) => self.hovered = Some(key),
            Action::React(channel, msgid, emoji) => self.toggle_reaction(&channel, msgid, emoji),
            Action::Reply(msgid, nick) => self.replying_to = Some((msgid, nick)),
            Action::CancelReply => self.replying_to = None,
        }
        Control::Redraw
    }

    fn toggle_reaction(&mut self, channel: &str, msgid: String, emoji: String) {
        let Some((index, _)) = self.active_target() else {
            return;
        };
        let server = &mut self.servers[index];
        let me = server.client.nick().to_owned();
        let reacted = server
            .channel(channel)
            .and_then(|found| found.log.get(&msgid))
            .and_then(|message| message.reactions.get(&emoji))
            .is_some_and(|who| who.iter().any(|nick| nick.eq_ignore_ascii_case(&me)));
        if reacted {
            server.client.remove_reaction(channel, msgid, emoji);
        } else {
            server.client.add_reaction(channel, msgid, emoji);
        }
        server.flush();
    }

    /// The caller has already hit-tested `event`.
    pub fn mouse(
        &mut self,
        event: MouseEvent,
        hit: Option<(Option<u64>, Option<Action>)>,
    ) -> Control {
        match event.action {
            MouseAction::WheelUp => self.scroll_by(WHEEL_MESSAGES as isize),
            MouseAction::WheelDown => self.scroll_by(-(WHEEL_MESSAGES as isize)),
            MouseAction::Press(Button::Left) => match hit.and_then(|(_, click)| click) {
                Some(action) => self.perform(action),
                None => Control::Wait,
            },
            MouseAction::Move | MouseAction::Drag(_) => {
                let hovered = hit.and_then(|(id, _)| id);
                if hovered == self.hovered {
                    Control::Wait
                } else {
                    self.hovered = hovered;
                    Control::Redraw
                }
            }
            _ => Control::Wait,
        }
    }

    fn scroll_by(&mut self, messages: isize) -> Control {
        let total = self.active().map_or(0, |(_, channel)| channel.log.len());
        let next = self
            .scroll
            .saturating_add_signed(messages)
            .min(total.saturating_sub(1));
        if next == self.scroll {
            return Control::Wait;
        }
        self.scroll = next;
        Control::Redraw
    }

    /// React to anything the terminal reported other than the mouse.
    pub fn input(&mut self, event: InputEvent) -> Control {
        match event {
            InputEvent::Key(Key::Char('c' | 'q'), Mods { ctrl: true, .. }) => Control::Quit,
            InputEvent::Key(key, mods) => {
                if self.form.visible {
                    self.form_key(key, mods)
                } else {
                    self.composer_key(key, mods)
                }
            }
            InputEvent::Paste(text) => {
                self.focused_text()
                    .push_str(&text.replace(['\r', '\n'], " "));
                Control::Redraw
            }
            InputEvent::Mode(1016, on) => {
                self.pixel_mouse = on;
                Control::Wait
            }
            InputEvent::Mouse(_)
            | InputEvent::Focus(_)
            | InputEvent::Mode(..)
            | InputEvent::GraphicsAnswer(_) => Control::Wait,
        }
    }

    fn focused_text(&mut self) -> &mut String {
        if !self.form.visible {
            return &mut self.draft;
        }
        match self.form.focus {
            Field::Host => &mut self.form.host,
            Field::Nick => &mut self.form.nick,
            Field::Channel => &mut self.form.channel,
        }
    }

    fn form_key(&mut self, key: Key, mods: Mods) -> Control {
        match key {
            Key::Enter => self.connect(),
            Key::Tab => {
                self.form.focus = match self.form.focus {
                    Field::Host => Field::Nick,
                    Field::Nick => Field::Channel,
                    Field::Channel => Field::Host,
                };
            }
            Key::Escape if !self.servers.is_empty() => self.form.visible = false,
            Key::Backspace => {
                self.focused_text().pop();
            }
            Key::Char(character) if !mods.ctrl && !mods.alt => self.focused_text().push(character),
            _ => return Control::Wait,
        }
        Control::Redraw
    }

    fn composer_key(&mut self, key: Key, mods: Mods) -> Control {
        match key {
            Key::Enter => self.send_draft(),
            Key::Escape => self.replying_to = None,
            Key::Backspace => {
                self.draft.pop();
            }
            Key::Char('u') if mods.ctrl => self.draft.clear(),
            Key::Char(character) if !mods.ctrl && !mods.alt => {
                self.draft.push(character);
                self.announce_typing();
            }
            Key::Up => return self.scroll_by(WHEEL_MESSAGES as isize),
            Key::Down => return self.scroll_by(-(WHEEL_MESSAGES as isize)),
            Key::PageUp => return self.scroll_by(PAGE_MESSAGES as isize),
            Key::PageDown => return self.scroll_by(-(PAGE_MESSAGES as isize)),
            Key::End => self.scroll = 0,
            _ => return Control::Wait,
        }
        Control::Redraw
    }

    fn announce_typing(&mut self) {
        let now = unix_ms();
        if now.saturating_sub(self.last_typing_ms) < TYPING_REFRESH_MS {
            return;
        }
        self.last_typing_ms = now;
        if let Some((index, target)) = self.active_target() {
            let server = &mut self.servers[index];
            server.client.set_typing(target, Typing::Active);
            server.flush();
        }
    }

    fn stop_typing(&mut self, index: usize) {
        if let Some((_, target)) = self.active_target() {
            self.servers[index].client.set_typing(target, Typing::Done);
        }
        self.last_typing_ms = 0;
    }

    fn send_draft(&mut self) {
        let text = std::mem::take(&mut self.draft);
        let trimmed = text.trim().to_owned();
        let Some(index) = self.selected_server().filter(|_| !trimmed.is_empty()) else {
            return;
        };
        self.stop_typing(index);
        if let Some(channel) = trimmed.strip_prefix("/join ") {
            self.selection = Some((index, None));
            self.servers[index]
                .client
                .join(channel.trim().to_owned(), None);
        } else if trimmed.starts_with('/') {
            // we keep an unknown command in the composer, because posting
            // it could put something like a password in the channel
            self.draft = text;
            return;
        } else if let Some((_, target)) = self.active_target() {
            match self.replying_to.take() {
                Some((msgid, _)) => self.servers[index]
                    .client
                    .send_raw_line(format!("@+draft/reply={msgid} PRIVMSG {target} :{trimmed}")),
                None => self.servers[index]
                    .client
                    .send_message(target, trimmed.to_owned()),
            }
            self.scroll = 0;
        }
        self.servers[index].flush();
    }

    /// Feed scripted server lines to a new server with no socket.
    #[cfg(test)]
    pub fn add_scripted_server(&mut self, nick: &str, script: &[&str], channel: &str) {
        let (sender, _) = std::sync::mpsc::channel();
        let connection = net::scripted(sender);
        let mut client = Client::new(Config::new(nick.to_owned()));
        client.handle_connected();
        for line in script {
            client.handle_bytes(format!("{line}\r\n").as_bytes());
        }
        self.servers.push(Server {
            endpoint: Endpoint {
                host: "irc.example.com".to_owned(),
                port: 6697,
                tls: true,
            },
            client,
            connection,
            join_after_register: None,
            closed: None,
        });
        self.selection = Some((self.servers.len() - 1, Some(channel.to_owned())));
        self.form.visible = false;
    }
}
