//! What the interface looks like, as a function of the chat state.

use katui_draw::Paint;
use katui_ui::{Align, Element, Length};
use katui_widgets::{Emphasis, Theme, button, label, text_field};
use obby_client::{Channel, ChatMessage, MessageKind};

use crate::chat::{Action, Chat, Command, Field, Preview, Server};
use crate::irc_text;
use crate::previews::image_url;
use crate::theme::{self, ASH, INK_ON_LIGHT, PUMICE, SULFUR, nick_color};

const REACTIONS: [&str; 5] = [
    "\u{1F44D}",
    "\u{2764}\u{FE0F}",
    "\u{1F602}",
    "\u{1F525}",
    "\u{1F389}",
];
const VISIBLE_MESSAGES: usize = 70;
/// We reserve room for the hover actions so showing them keeps the layout still.
const HEADING_ROWS: f32 = 1.25;

const RAIL_KEY: u64 = 1;
const CHANNELS_KEY: u64 = 2;
const HEADER_KEY: u64 = 3;
pub const COMPOSER_KEY: u64 = 4;
const MEMBERS_KEY: u64 = 5;
const FORM_KEY: u64 = 6;
const CONNECTING_KEY: u64 = 7;
const SCRIM_KEY: u64 = 8;
const MENU_KEY: u64 = 9;
const MESSAGE_KEYS_FROM: u64 = 1_000;
pub const SPINNER_DOTS: usize = 3;

type Node = Element<Action>;

pub fn view(chat: &Chat, theme: &Theme) -> Node {
    let content = match chat.active() {
        Some((server, channel)) => vec![
            channel_list(chat, theme),
            conversation(chat, server, channel, theme),
            members(server, channel, theme),
        ],
        None if chat.selected_server().is_some() && !chat.form.visible => {
            vec![channel_list(chat, theme), connecting(chat, theme)]
        }
        None => vec![welcome(chat, theme)],
    };
    let mut panes = vec![server_rail(chat, theme)];
    panes.extend(content);
    let body = Element::row(panes).size(Length::Fill, Length::Fill);
    if chat.form.visible && chat.active().is_some() {
        let scrim = Element::column([])
            .size(Length::Fill, Length::Fill)
            .background(theme::scrim())
            .on_click(Action::ShowForm)
            .layer(SCRIM_KEY);
        Element::stack([body, scrim, centered(form(chat, theme)).width(Length::Fill)])
    } else {
        body
    }
}

fn centered(child: Node) -> Node {
    Element::column([child])
        .align(Align::Center)
        .justify(Align::Center)
        .grow(1.0)
        .height(Length::Fill)
}

fn server_rail(chat: &Chat, theme: &Theme) -> Node {
    let side = theme.rows(2.0);
    let mut buttons: Vec<Node> = chat
        .servers
        .iter()
        .enumerate()
        .map(|(index, server)| {
            let selected = chat.selected_server() == Some(index);
            let initial = server
                .endpoint
                .host
                .chars()
                .next()
                .unwrap_or('?')
                .to_uppercase()
                .to_string();
            badge(initial, selected, side, theme).on_click(Action::SelectServer(index))
        })
        .collect();
    buttons.push(badge("+".to_owned(), chat.form.visible, side, theme).on_click(Action::ShowForm));
    Element::column(buttons)
        .gap(theme.cell_height() * 0.5)
        .align(Align::Center)
        .padding(theme.cell_height() * 0.5)
        .width(theme.columns(7.0))
        .height(Length::Fill)
        .background(theme::rail())
        .layer(RAIL_KEY)
}

fn badge(text: String, selected: bool, side: Length, theme: &Theme) -> Node {
    let (fill, ink) = if selected {
        (Paint::Solid(ASH), INK_ON_LIGHT)
    } else {
        (theme::raised(), ASH)
    };
    Element::column([label(text, theme.body(), ink).weight(700)])
        .align(Align::Center)
        .justify(Align::Center)
        .size(side, side)
        .radius(theme.cell_height() * 0.6)
        .background(fill)
}

fn channel_list(chat: &Chat, theme: &Theme) -> Node {
    let Some(index) = chat.selected_server() else {
        return Element::spacer();
    };
    let server = &chat.servers[index];
    let mut rows = vec![
        label(server.endpoint.host.clone(), theme.body(), ASH).weight(700),
        label(server.status(), theme.small(), PUMICE),
    ];
    for channel in server.channels() {
        rows.push(channel_row(chat, index, channel, theme));
    }
    Element::column(rows)
        .gap(theme.cell_height() * 0.25)
        .padding(theme.cell_height() * 0.75)
        .width(theme.columns(26.0))
        .height(Length::Fill)
        .background(theme::panel())
        .layer(CHANNELS_KEY)
}

fn channel_row(chat: &Chat, server: usize, channel: &Channel, theme: &Theme) -> Node {
    let selected = chat.is_selected(server, &channel.name);
    let name_color = if selected || channel.unread > 0 {
        ASH
    } else {
        PUMICE
    };
    let mut parts = vec![
        label(channel.name.clone(), theme.body(), name_color)
            .weight(if channel.unread > 0 { 700 } else { 400 })
            .grow(1.0),
    ];
    if !selected && channel.mentions > 0 {
        parts.push(label(channel.mentions.to_string(), theme.small(), SULFUR).weight(700));
    } else if !selected && channel.unread > 0 {
        parts.push(label(channel.unread.to_string(), theme.small(), PUMICE));
    }
    let row = Element::row(parts)
        .gap(theme.cell_width())
        .align(Align::Center)
        .padding_xy(theme.cell_width(), theme.cell_height() * 0.2)
        .radius(theme.cell_height() * 0.4)
        .on_click(Action::SelectChannel(server, channel.name.clone()));
    if selected {
        row.background(theme::lifted())
    } else {
        row
    }
}

fn conversation(chat: &Chat, server: &Server, channel: &Channel, theme: &Theme) -> Node {
    let total = channel.log.len();
    let end = total.saturating_sub(chat.scroll);
    let start = end.saturating_sub(VISIBLE_MESSAGES);
    let rows: Vec<Node> = channel
        .log
        .iter()
        .skip(start)
        .take(end - start)
        .filter(|message| shows_in_log(message))
        .map(|message| message_row(chat, server, channel, message, theme))
        .collect();
    let log = if rows.is_empty() {
        centered(label(
            "No messages yet. Say something below.",
            theme.body(),
            PUMICE,
        ))
    } else {
        Element::column(rows)
            .justify(Align::End)
            .size(Length::Fill, Length::Fill)
            .scroll(0.0)
    };
    let pane = Element::column([header(channel, theme), log, composer(chat, channel, theme)])
        .size(Length::Fill, Length::Fill);
    let matches = chat.command_matches();
    if matches.is_empty() {
        return pane.grow(1.0);
    }
    let menu = command_menu(&matches, chat.menu_selection(), theme);
    let anchored = Element::column([menu])
        .justify(Align::End)
        .padding_xy(theme.cell_width() * 2.0, theme.cell_height() * 3.5)
        .size(Length::Fill, Length::Fill);
    Element::stack([pane, anchored])
        .grow(1.0)
        .height(Length::Fill)
}

fn command_menu(matches: &[&Command], selected: usize, theme: &Theme) -> Node {
    let rows = matches.iter().enumerate().map(|(index, command)| {
        let row = Element::row([
            label(format!("/{}", command.name), theme.body(), ASH).weight(700),
            label(command.usage, theme.small(), PUMICE),
        ])
        .gap(theme.cell_width())
        .align(Align::Center)
        .padding_xy(theme.cell_width(), theme.cell_height() * 0.2)
        .radius(theme.cell_height() * 0.3)
        .width(Length::Fill)
        .on_click(Action::Complete(command.name.to_owned()));
        if index == selected {
            row.background(theme::lifted())
        } else {
            row
        }
    });
    Element::column(rows)
        .padding(theme.cell_height() * 0.3)
        .radius(theme.cell_height() * 0.5)
        .background(Paint::Solid(theme::MENU))
        .border(1.0, theme::lifted())
        .width(theme.columns(40.0))
        .layer(MENU_KEY)
}

fn shows_in_log(message: &ChatMessage) -> bool {
    (matches!(message.kind, MessageKind::Privmsg | MessageKind::Notice) || is_action(message))
        && !message.redacted
}

fn header(channel: &Channel, theme: &Theme) -> Node {
    let topic = channel.topic.clone().unwrap_or_default();
    Element::column([
        label(channel.name.clone(), theme.title(), ASH).weight(800),
        label(topic, theme.small(), PUMICE),
    ])
    .padding_xy(theme.cell_width() * 2.0, theme.cell_height() * 0.5)
    .width(Length::Fill)
    .layer(HEADER_KEY)
}

pub fn message_key(message: &ChatMessage) -> u64 {
    MESSAGE_KEYS_FROM + message.key.seq
}

fn message_row(
    chat: &Chat,
    server: &Server,
    channel: &Channel,
    message: &ChatMessage,
    theme: &Theme,
) -> Node {
    let key = message_key(message);
    let hovered = chat.hovered == Some(key);
    let mut lines = vec![message_heading(chat, channel, message, hovered, theme)];
    if let Some(quote) = reply_quote(channel, message, theme) {
        lines.push(quote);
    }
    lines.push(message_body(message, theme));
    if let Some(card) = preview_card(chat, message, theme) {
        lines.push(card);
    }
    if let Some(chips) = reaction_chips(server, channel, message, theme) {
        lines.push(chips);
    }
    let body = Element::column(lines)
        .gap(theme.cell_height() * 0.15)
        .grow(1.0);
    let row = Element::row([
        avatar(
            &message.sender,
            is_online(server, channel, &message.sender),
            theme,
        ),
        body,
    ])
    .gap(theme.cell_width() * 1.5)
    .padding_xy(theme.cell_width() * 2.0, theme.cell_height() * 0.4)
    .width(Length::Fill)
    .id(key)
    .on_hover(Action::Hover(key));
    let row = if hovered {
        row.background(theme::lifted())
    } else {
        row
    };
    row.layer(key)
}

fn is_online(server: &Server, channel: &Channel, nick: &str) -> bool {
    channel
        .members
        .contains_key(&server.client.isupport().fold(nick))
}

fn message_body(message: &ChatMessage, theme: &Theme) -> Node {
    let mut spans = irc_text::parse(&message.text);
    if is_action(message) {
        spans.iter_mut().for_each(|span| span.italic = true);
    }
    Element::rich(spans).font_size(theme.body()).color(ASH)
}

fn is_action(message: &ChatMessage) -> bool {
    matches!(&message.kind, MessageKind::Ctcp { command } if command == "ACTION")
}

fn avatar(nick: &str, online: bool, theme: &Theme) -> Node {
    let side = theme.rows(2.0);
    let initial = nick
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let disc = Element::column([label(initial, theme.body(), theme::WHITE).weight(800)])
        .align(Align::Center)
        .justify(Align::Center)
        .size(side, side)
        .radius(theme.cell_height())
        .background(Paint::Solid(theme::AVATAR));
    if !online {
        return disc;
    }
    let dot_side = Length::Px(theme.cell_height() * 0.5);
    let dot = Element::row([])
        .size(dot_side, dot_side)
        .radius(theme.cell_height() * 0.25)
        .border(2.0, Paint::Solid(theme::AVATAR))
        .background(Paint::Solid(theme::ONLINE));
    let corner = Element::column([dot])
        .align(Align::End)
        .justify(Align::End)
        .size(side, side);
    Element::stack([disc, corner]).size(side, side)
}

fn message_heading(
    chat: &Chat,
    channel: &Channel,
    message: &ChatMessage,
    hovered: bool,
    theme: &Theme,
) -> Node {
    let mut parts = vec![
        label(
            message.sender.clone(),
            theme.body(),
            nick_color(&message.sender),
        )
        .weight(700),
        label(
            relative_age(chat.now_ms(), message.key.time_ms),
            theme.small(),
            PUMICE,
        )
        .grow(1.0),
    ];
    if hovered && let Some(msgid) = &message.msgid {
        parts.push(hover_actions(channel, msgid, &message.sender, theme));
    }
    Element::row(parts)
        .gap(theme.cell_width())
        .align(Align::Center)
        .height(theme.rows(HEADING_ROWS))
}

fn hover_actions(channel: &Channel, msgid: &str, sender: &str, theme: &Theme) -> Node {
    let mut actions: Vec<Node> = REACTIONS
        .iter()
        .map(|emoji| {
            label(*emoji, theme.body(), ASH)
                .padding_xy(theme.cell_width() * 0.4, 0.0)
                .on_click(Action::React(
                    channel.name.clone(),
                    msgid.to_owned(),
                    (*emoji).to_owned(),
                ))
        })
        .collect();
    actions.push(
        label("Reply", theme.small(), ASH)
            .padding_xy(theme.cell_width(), 0.0)
            .on_click(Action::Reply(msgid.to_owned(), sender.to_owned())),
    );
    Element::row(actions)
        .align(Align::Center)
        .radius(theme.cell_height() * 0.4)
        .background(theme::raised())
}

fn reply_quote(channel: &Channel, message: &ChatMessage, theme: &Theme) -> Option<Node> {
    let original = channel.log.get(message.reply_to.as_deref()?)?;
    let excerpt: String = irc_text::strip(&original.text).chars().take(90).collect();
    let accent = nick_color(&original.sender);
    let bar = Element::row([])
        .size(Length::Px(2.0), Length::Fill)
        .background(Paint::Solid(accent));
    let quote = Element::column([
        Element::row([
            label("\u{21A9}", theme.small(), accent),
            label(original.sender.clone(), theme.small(), accent).weight(700),
        ])
        .gap(theme.cell_width() * 0.5)
        .align(Align::Center),
        label(excerpt, theme.small(), PUMICE),
    ])
    .gap(theme.cell_height() * 0.1)
    .padding_xy(theme.cell_width(), theme.cell_height() * 0.25);
    Some(
        Element::row([bar, quote])
            .radius(theme.cell_height() * 0.2)
            .background(Paint::Solid(theme::QUOTE))
            .width(Length::Fill),
    )
}

fn preview_card(chat: &Chat, message: &ChatMessage, theme: &Theme) -> Option<Node> {
    let link = message.link_preview.as_ref();
    let url = link
        .and_then(|preview| preview.image.as_deref())
        .or_else(|| image_url(&message.text))?;
    match chat.previews.get(url)? {
        Preview::Ready(surface) => {
            let picture = Element::image(surface.clone())
                .size(
                    Length::Px(surface.width() as f32),
                    Length::Px(surface.height() as f32),
                )
                .radius(theme.cell_height() * 0.5);
            let title = link.map(|preview| label(preview.title.clone(), theme.small(), PUMICE));
            Some(
                Element::column(std::iter::once(picture).chain(title))
                    .gap(theme.cell_height() * 0.2),
            )
        }
        Preview::Loading => Some(label("Loading image", theme.small(), PUMICE)),
        Preview::Failed => None,
    }
}

fn reaction_chips(
    server: &Server,
    channel: &Channel,
    message: &ChatMessage,
    theme: &Theme,
) -> Option<Node> {
    let msgid = message.msgid.as_ref()?;
    if message.reactions.is_empty() {
        return None;
    }
    let me = server.client.nick();
    let chips = message.reactions.iter().map(|(emoji, who)| {
        let mine = who.iter().any(|nick| nick.eq_ignore_ascii_case(me));
        label(
            format!("{emoji} {}", who.len()),
            theme.small(),
            if mine { SULFUR } else { ASH },
        )
        .padding_xy(theme.cell_width() * 0.7, theme.cell_height() * 0.05)
        .radius(theme.cell_height() * 0.5)
        .background(if mine {
            theme::raised()
        } else {
            theme::lifted()
        })
        .on_click(Action::React(
            channel.name.clone(),
            msgid.clone(),
            emoji.clone(),
        ))
    });
    Some(Element::row(chips).gap(theme.cell_width() * 0.5))
}

fn composer(chat: &Chat, channel: &Channel, theme: &Theme) -> Node {
    let mut lines = Vec::new();
    let typing = chat
        .typing_is_live(&channel.name)
        .then_some(channel)
        .and_then(|channel| {
            typing_line(
                channel,
                chat.active().map_or("", |(server, _)| server.client.nick()),
            )
        });
    if let Some(line) = typing {
        lines.push(label(line, theme.small(), PUMICE));
    }
    if let Some((_, nick)) = &chat.replying_to {
        lines.push(
            Element::row([
                label(
                    format!("Replying to {nick}"),
                    theme.small(),
                    nick_color(nick),
                )
                .grow(1.0),
                label("Cancel reply", theme.small(), PUMICE).on_click(Action::CancelReply),
            ])
            .align(Align::Center),
        );
    }
    lines.push(text_field(
        &chat.draft,
        &format!("Message {}", channel.name),
        !chat.form.visible,
        theme,
    ));
    Element::column(lines)
        .gap(theme.cell_height() * 0.25)
        .padding_xy(theme.cell_width() * 2.0, theme.cell_height() * 0.5)
        .width(Length::Fill)
        .layer(COMPOSER_KEY)
}

fn typing_line(channel: &Channel, me: &str) -> Option<String> {
    let names: Vec<String> = channel
        .typing
        .iter()
        .map(ToString::to_string)
        .filter(|who| !who.eq_ignore_ascii_case(me))
        .take(3)
        .collect();
    (!names.is_empty()).then(|| format!("{} typing", names.join(", ")))
}

fn members(server: &Server, channel: &Channel, theme: &Theme) -> Node {
    let mut rows = vec![label(
        format!("{} here", channel.members.len()),
        theme.small(),
        PUMICE,
    )];
    rows.extend(
        server
            .member_names(channel)
            .into_iter()
            .take(80)
            .map(|(prefix, nick)| {
                Element::row([
                    label(prefix, theme.small(), SULFUR),
                    label(nick.clone(), theme.body(), nick_color(&nick)),
                ])
                .gap(theme.cell_width() * 0.3)
            }),
    );
    Element::column(rows)
        .gap(theme.cell_height() * 0.1)
        .padding(theme.cell_height() * 0.75)
        .width(theme.columns(24.0))
        .height(Length::Fill)
        .background(theme::panel())
        .layer(MEMBERS_KEY)
}

fn connecting(chat: &Chat, theme: &Theme) -> Node {
    let Some(server) = chat
        .selected_server()
        .and_then(|index| chat.servers.get(index))
    else {
        return Element::spacer();
    };
    let heading = label(server.status(), theme.title(), ASH).weight(800);
    let mut lines = vec![heading];
    if server.is_connecting() {
        lines.push(spinner(chat.spinner_step(), theme));
        lines.push(label(
            "Some servers take up to 20 seconds to let you in.",
            theme.body(),
            PUMICE,
        ));
    }
    Element::column([Element::column(lines)
        .align(Align::Center)
        .gap(theme.cell_height())
        .layer(CONNECTING_KEY)])
    .align(Align::Center)
    .justify(Align::Center)
    .grow(1.0)
    .height(Length::Fill)
}

fn spinner(step: usize, theme: &Theme) -> Node {
    let side = Length::Px(theme.cell_height() * 0.5);
    Element::row((0..SPINNER_DOTS).map(|dot| {
        Element::row([])
            .size(side, side)
            .radius(theme.cell_height() * 0.25)
            .background(Paint::Solid(if dot == step { ASH } else { theme::DIM_DOT }))
    }))
    .gap(theme.cell_width())
}

fn welcome(chat: &Chat, theme: &Theme) -> Node {
    centered(form(chat, theme))
}

fn form(chat: &Chat, theme: &Theme) -> Node {
    let field = |kind: Field, name: &str| {
        Element::column([
            label(name.to_owned(), theme.small(), PUMICE),
            text_field(chat.form.field(kind), "", chat.form.focus.is(kind), theme),
        ])
        .gap(theme.cell_height() * 0.2)
        .width(Length::Fill)
        .on_click(Action::FocusField(kind))
    };
    let connect = button("Connect", Action::Connect, Emphasis::Primary, theme).width(Length::Fill);
    Element::column([
        label("Add a server", theme.title(), ASH).weight(800),
        field(Field::Host, "Host and port"),
        field(Field::Nick, "Nick"),
        field(Field::Channel, "Channel"),
        connect,
    ])
    .gap(theme.cell_height() * 0.75)
    .padding(theme.cell_height() * 1.5)
    .width(theme.columns(60.0))
    .radius(theme.cell_height())
    .background(theme::raised())
    .border(1.0, theme::lifted())
    .on_click(Action::FocusField(chat.form.focus.current()))
    .layer(FORM_KEY)
}

/// How long ago `then` was, in the fewest characters that say so.
fn relative_age(now_ms: u64, then_ms: u64) -> String {
    let minutes = now_ms.saturating_sub(then_ms) / 60_000;
    match minutes {
        0 => "now".to_owned(),
        1..=59 => format!("{minutes}m"),
        60..=1439 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1440),
    }
}

#[cfg(test)]
mod tests {
    use super::relative_age;

    #[test]
    fn ages_round_down_to_the_largest_unit() {
        assert_eq!(relative_age(30_000, 0), "now");
        assert_eq!(relative_age(120_000, 0), "2m");
        assert_eq!(relative_age(7_200_000, 0), "2h");
        assert_eq!(relative_age(172_800_000, 0), "2d");
    }
}
