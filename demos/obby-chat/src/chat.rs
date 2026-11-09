//! The chat state and what events do to it. Memory only.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use katui_proto::input::{Button, InputEvent, Key, Mods, MouseAction, MouseEvent};
use katui_rt::app::Control;
use katui_rt::events::Waker;
use katui_scene::surface::Surface;
use katui_widgets::{Edit, Focus, TextField};
use obby_client::{
    Change, Channel, Client, Config, Event, MessageKey, MessageKind, Now, Phase, Typing,
};

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
/// How long after a join we hide typing: chat history replays old typing
/// notices as if they were live, and they take this long to expire.
const HISTORY_TYPING_QUIET: std::time::Duration = std::time::Duration::from_secs(8);
const DRIFT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(83);
const CTCP_REPLY_INTERVAL_MS: u64 = 1_000;
/// The numerics a server answers a failed `JOIN` with.
const JOIN_FAILURES: [&str; 8] = ["403", "405", "471", "473", "474", "475", "477", "489"];
const HISTORY_CAPABILITY: &str = "draft/chathistory";
const HISTORY_ON_JOIN: u16 = 100;
const WHEEL_MESSAGES: usize = 3;
const PAGE_MESSAGES: usize = 12;

#[derive(Debug)]
pub enum Message {
    Net(usize, NetEvent),
    Tick,
    Drift,
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
    Complete(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Host,
    Nick,
    Channel,
}

pub struct Form {
    pub visible: bool,
    pub focus: Focus<Field>,
    pub host: TextField,
    pub nick: TextField,
    pub channel: TextField,
}

impl Form {
    pub fn field(&self, field: Field) -> &TextField {
        match field {
            Field::Host => &self.host,
            Field::Nick => &self.nick,
            Field::Channel => &self.channel,
        }
    }

    fn focused_mut(&mut self) -> &mut TextField {
        match self.focus.current() {
            Field::Host => &mut self.host,
            Field::Nick => &mut self.nick,
            Field::Channel => &mut self.channel,
        }
    }
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
    joining: bool,
    pending_join: Option<String>,
    join_error: Option<String>,
    closed: Option<String>,
}

impl Server {
    /// What the connection is doing, in a few words.
    pub fn status(&self) -> String {
        if let Some(reason) = &self.closed {
            return format!("Disconnected: {reason}");
        }
        if let Some(problem) = &self.join_error {
            return format!("Could not join: {problem}");
        }
        match self.client.phase() {
            Phase::Registered if self.joining => format!("Joining as {}", self.client.nick()),
            Phase::Registered => format!("Connected as {}", self.client.nick()),
            Phase::Disconnected => format!("Connecting to {}", self.endpoint.host),
            _ => format!("Signing in to {}", self.endpoint.host),
        }
    }

    /// Whether the connection is still on its way to a joined channel.
    pub fn is_connecting(&self) -> bool {
        self.closed.is_none() && (self.client.phase() != Phase::Registered || self.joining)
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
    pub draft: TextField,
    pub replying_to: Option<(String, String)>,
    pub hovered: Option<u64>,
    pub scroll: usize,
    pub previews: HashMap<String, Preview>,
    pub backdrop: Backdrop,
    pub effects: Effects,
    waker: Option<Waker<Message>>,
    started: Instant,
    last_typing_ms: u64,
    connect_when_attached: bool,
    menu_index: usize,
    joined_at: HashMap<(usize, String), Instant>,
    focused: bool,
    last_ctcp_reply_ms: u64,
}

pub struct Command {
    pub name: &'static str,
    pub usage: &'static str,
}

const COMMANDS: [Command; 6] = [
    Command {
        name: "join",
        usage: "#channel",
    },
    Command {
        name: "part",
        usage: "reason",
    },
    Command {
        name: "me",
        usage: "action",
    },
    Command {
        name: "nick",
        usage: "new nick",
    },
    Command {
        name: "topic",
        usage: "new topic",
    },
    Command {
        name: "msg",
        usage: "nick message",
    },
];

/// How wide a message's text runs, avatar included, in cells.
fn effect_columns(text: &str) -> u16 {
    u16::try_from(text.chars().count())
        .unwrap_or(u16::MAX)
        .saturating_add(AVATAR_COLUMNS)
}

/// What we answer to a CTCP query, framed in its `\x01` markers.
fn ctcp_reply(command: &str, parameters: &str) -> Option<String> {
    let body = match command {
        "VERSION" => "VERSION katui obby-chat demo".to_owned(),
        "PING" => format!("PING {parameters}"),
        "CLIENTINFO" => "CLIENTINFO ACTION CLIENTINFO PING VERSION".to_owned(),
        _ => return None,
    };
    Some(format!("\u{1}{body}\u{1}"))
}

fn client_join(server: &mut Server, channel: &str) {
    server.client.join(channel.to_owned(), None);
    server.pending_join = Some(channel.to_owned());
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
                focus: Focus::new(Field::Host, [Field::Nick, Field::Channel]),
                host: TextField::with_text("irc.h4ks.com:6697"),
                nick: TextField::with_text(nick),
                channel: TextField::with_text("#bots"),
            },
            draft: TextField::default(),
            replying_to: None,
            hovered: None,
            scroll: 0,
            previews: HashMap::new(),
            backdrop: Backdrop::default(),
            effects: Effects::default(),
            waker: None,
            started: Instant::now(),
            last_typing_ms: 0,
            connect_when_attached: false,
            menu_index: 0,
            joined_at: HashMap::new(),
            focused: true,
            last_ctcp_reply_ms: 0,
        }
    }

    pub fn attach(&mut self, waker: Waker<Message>) {
        let ticker = waker.clone();
        std::thread::spawn(move || {
            while ticker.send(Message::Tick) {
                std::thread::sleep(TICK);
            }
        });
        let drifter = waker.clone();
        std::thread::spawn(move || {
            while drifter.send(Message::Drift) {
                std::thread::sleep(DRIFT_INTERVAL);
            }
        });
        self.waker = Some(waker);
        if self.connect_when_attached {
            self.connect();
        }
    }

    /// Whether typing notices in `channel` can be trusted to be live.
    pub fn typing_is_live(&self, channel: &str) -> bool {
        let Some(server) = self.selected_server() else {
            return false;
        };
        self.joined_at
            .get(&(server, channel.to_owned()))
            .is_none_or(|joined| joined.elapsed() >= HISTORY_TYPING_QUIET)
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
        let address = self.form.host.text();
        let (host, port) = address
            .rsplit_once(':')
            .and_then(|(host, port)| Some((host.to_owned(), port.parse().ok()?)))
            .unwrap_or_else(|| (address.to_owned(), 6697));
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
            client: Client::new(Config::new(self.form.nick.text().to_owned())),
            connection,
            join_after_register: Some(self.form.channel.text().to_owned())
                .filter(|name| !name.is_empty()),
            joining: false,
            pending_join: None,
            join_error: None,
            closed: None,
        });
        self.selection = Some((index, None));
        self.form.visible = false;
    }

    pub fn receive(&mut self, message: Message) -> Control {
        match message {
            Message::Net(index, event) => self.net_event(index, event),
            Message::Tick => self.tick(),
            Message::Drift if self.focused => {
                self.backdrop.advance(self.started.elapsed().as_secs_f32());
                Control::Redraw
            }
            Message::Drift => Control::Wait,
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
                        server.joining = true;
                        server.client.join(channel, None);
                    }
                }
                Event::ModelChanged { change } => self.apply_change(index, change),
                Event::RawLine { message } if JOIN_FAILURES.contains(&message.command.as_str()) => {
                    let server = &mut self.servers[index];
                    server.joining = false;
                    server.pending_join = None;
                    server.join_error = message.params.last().cloned();
                }
                _ => {}
            }
        }
        self.servers[index].flush();
        changed
    }

    fn apply_change(&mut self, index: usize, change: Change) {
        match change {
            Change::ChannelJoined { channel } => {
                self.joined_at
                    .insert((index, channel.clone()), Instant::now());
                let server = &mut self.servers[index];
                server.joining = false;
                server.join_error = None;
                let requested = server
                    .pending_join
                    .take_if(|name| name.eq_ignore_ascii_case(&channel))
                    .is_some();
                if server.client.capabilities().has(HISTORY_CAPABILITY) {
                    server
                        .client
                        .fetch_history(channel.clone(), None, HISTORY_ON_JOIN);
                }
                if requested || self.selection == Some((index, None)) {
                    self.open_channel(index, Some(channel));
                }
            }
            Change::MessageAdded { key, .. } if self.answer_ctcp(index, &key) => {}
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
            self.effects.spawn(layer, Kind::Pop, hue, columns);
        }
    }

    /// Answer a CTCP query addressed to us; `true` when `key` was a CTCP
    /// other than an action, which never shows in the log.
    fn answer_ctcp(&mut self, index: usize, key: &MessageKey) -> bool {
        let model = self.servers[index].client.model();
        let found = model
            .channels()
            .map(|(_, channel)| &channel.log)
            .chain(
                model
                    .conversations()
                    .map(|(_, conversation)| &conversation.log),
            )
            .find_map(|log| log.get_by_key(key));
        let Some(message) = found else {
            return false;
        };
        let MessageKind::Ctcp { command } = &message.kind else {
            return false;
        };
        if command == "ACTION" {
            return false;
        }
        let reply = ctcp_reply(command, &message.text);
        let (sender, stale) = (message.sender.clone(), message.historical || message.own);
        let now = unix_ms();
        if let Some(reply) = reply.filter(|_| {
            !stale && now.saturating_sub(self.last_ctcp_reply_ms) >= CTCP_REPLY_INTERVAL_MS
        }) {
            self.last_ctcp_reply_ms = now;
            self.servers[index].client.send_notice(sender, reply);
        }
        true
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
            Action::FocusField(field) => {
                self.form.focus.set(field);
            }
            Action::Connect => self.connect(),
            Action::Hover(key) => self.hovered = Some(key),
            Action::React(channel, msgid, emoji) => self.toggle_reaction(&channel, msgid, emoji),
            Action::Reply(msgid, nick) => self.replying_to = Some((msgid, nick)),
            Action::CancelReply => self.replying_to = None,
            Action::Complete(name) => self.draft.set_text(format!("/{name} ")),
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
            InputEvent::Key(..) | InputEvent::Paste(_) if self.form.visible => {
                self.form_key(&event)
            }
            InputEvent::Key(..) | InputEvent::Paste(_) => self.composer_key(&event),
            InputEvent::Focus(focused) => {
                self.focused = focused;
                Control::Wait
            }
            InputEvent::Mouse(_)
            | InputEvent::KeyUp(..)
            | InputEvent::Mode(..)
            | InputEvent::GraphicsAnswer(_)
            | InputEvent::DeviceAttributes => Control::Wait,
        }
    }

    fn form_key(&mut self, event: &InputEvent) -> Control {
        if self.form.focus.handle(event) {
            return Control::Redraw;
        }
        match event {
            InputEvent::Key(Key::Escape, _) if !self.servers.is_empty() => {
                self.form.visible = false;
            }
            _ => match self.form.focused_mut().handle(event) {
                Edit::Submitted => self.connect(),
                Edit::Changed | Edit::Moved => {}
                Edit::Ignored => return Control::Wait,
            },
        }
        Control::Redraw
    }

    fn composer_key(&mut self, event: &InputEvent) -> Control {
        let key = match event {
            InputEvent::Key(key, _) => Some(*key),
            _ => None,
        };
        match key {
            Some(Key::Tab | Key::Enter) if self.completion().is_some() => {
                if let Some(name) = self.completion() {
                    self.draft.set_text(format!("/{name} "));
                }
            }
            Some(Key::Escape) if self.draft.text().starts_with('/') => {
                self.draft.take();
            }
            Some(Key::Escape) => self.replying_to = None,
            Some(Key::Up | Key::Down) if !self.command_matches().is_empty() => {
                let count = self.command_matches().len();
                let step = if key == Some(Key::Up) { count - 1 } else { 1 };
                self.menu_index = (self.menu_index + step) % count;
            }
            Some(Key::Up) => return self.scroll_by(WHEEL_MESSAGES as isize),
            Some(Key::Down) => return self.scroll_by(-(WHEEL_MESSAGES as isize)),
            Some(Key::PageUp) => return self.scroll_by(PAGE_MESSAGES as isize),
            Some(Key::PageDown) => return self.scroll_by(-(PAGE_MESSAGES as isize)),
            _ => return self.edit_draft(event),
        }
        Control::Redraw
    }

    fn edit_draft(&mut self, event: &InputEvent) -> Control {
        match self.draft.handle(event) {
            Edit::Submitted => self.send_draft(),
            Edit::Changed => {
                self.menu_index = 0;
                if !self.draft.text().starts_with('/') {
                    self.announce_typing();
                }
            }
            Edit::Moved => {}
            // we jump to the newest message on End once the cursor is
            // already at the end of the draft
            Edit::Ignored if matches!(event, InputEvent::Key(Key::End, _)) => self.scroll = 0,
            Edit::Ignored => return Control::Wait,
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

    /// The commands the draft could still become, while it is one word.
    pub fn command_matches(&self) -> Vec<&'static Command> {
        let Some(typed) = self
            .draft
            .text()
            .strip_prefix('/')
            .filter(|typed| !typed.contains(' '))
        else {
            return Vec::new();
        };
        COMMANDS
            .iter()
            .filter(|command| command.name.starts_with(typed))
            .collect()
    }

    pub fn menu_selection(&self) -> usize {
        self.menu_index
            .min(self.command_matches().len().saturating_sub(1))
    }

    fn completion(&self) -> Option<&'static str> {
        self.command_matches()
            .get(self.menu_selection())
            .map(|command| command.name)
    }

    /// Run a slash command; `false` when it is unknown or incomplete.
    fn run_command(&mut self, index: usize, line: &str) -> bool {
        let (name, argument) = line
            .trim_start_matches('/')
            .split_once(' ')
            .unwrap_or((line.trim_start_matches('/'), ""));
        let argument = argument.trim();
        let target = self.active_target().map(|(_, target)| target);
        match (name, target) {
            ("join", _) if !argument.is_empty() => {
                let already_in = self.servers[index]
                    .channels()
                    .iter()
                    .find(|channel| channel.name.eq_ignore_ascii_case(argument))
                    .map(|channel| channel.name.clone());
                match already_in {
                    Some(name) => self.open_channel(index, Some(name)),
                    None => {
                        client_join(&mut self.servers[index], argument);
                    }
                }
            }
            ("part", Some(target)) => {
                let reason = Some(argument.to_owned()).filter(|reason| !reason.is_empty());
                self.servers[index].client.part(target.clone(), reason);
                let next = self.servers[index]
                    .channels()
                    .iter()
                    .map(|channel| channel.name.clone())
                    .find(|name| *name != target);
                self.open_channel(index, next);
            }
            ("me", Some(target)) if !argument.is_empty() => self.servers[index]
                .client
                .send_action(target, argument.to_owned()),
            ("nick", _) if !argument.is_empty() => {
                self.servers[index].client.set_nick(argument.to_owned())
            }
            ("topic", Some(target)) if !argument.is_empty() => self.servers[index]
                .client
                .set_topic(target, Some(argument.to_owned())),
            ("msg", _) => match argument.split_once(' ') {
                Some((nick, text)) => self.servers[index]
                    .client
                    .send_message(nick.to_owned(), text.to_owned()),
                None => return false,
            },
            _ => return false,
        }
        true
    }

    fn open_channel(&mut self, server: usize, channel: Option<String>) {
        self.selection = Some((server, channel));
        self.scroll = 0;
        self.replying_to = None;
    }

    fn stop_typing(&mut self, index: usize) {
        if let Some((_, target)) = self.active_target() {
            self.servers[index].client.set_typing(target, Typing::Done);
        }
        self.last_typing_ms = 0;
    }

    fn send_draft(&mut self) {
        let text = self.draft.take();
        let trimmed = text.trim().to_owned();
        let Some(index) = self.selected_server().filter(|_| !trimmed.is_empty()) else {
            return;
        };
        self.stop_typing(index);
        if trimmed.starts_with('/') {
            // we keep an unknown command in the composer, because posting
            // it could put something like a password in the channel
            if !self.run_command(index, &trimmed) {
                self.draft.set_text(text);
                return;
            }
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
    pub fn add_scripted_server(
        &mut self,
        nick: &str,
        script: &[&str],
        channel: &str,
    ) -> std::sync::mpsc::Receiver<Vec<u8>> {
        let (sender, sent) = std::sync::mpsc::channel();
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
            joining: false,
            pending_join: None,
            join_error: None,
            closed: None,
        });
        self.drain_events(self.servers.len() - 1);
        self.selection = Some((self.servers.len() - 1, Some(channel.to_owned())));
        self.form.visible = false;
        sent
    }
}
