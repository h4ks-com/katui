//! Capability probing: one query batch through a [`Transport`], every
//! answer parsed from a single reply stream, plus the environment sniff
//! that seeds the result when a terminal stays silent.
//!
//! The batch ends with the primary device attributes, which every terminal
//! must answer, so that reply terminates the exchange. Time is the
//! transport's concern: its read patience and total budget are configured
//! where it is built.

use std::time::Duration;

use crate::kitty;

/// How long the probe waits per read before asking the transport again.
/// The transport may cut the wait shorter under its own budget.
const READ_WAIT: Duration = Duration::from_millis(50);

/// The channel a probe talks through. The real one is the controlling tty
/// in raw mode; tests script it with recorded replies.
pub trait Transport {
    /// Send bytes to the terminal. A failed write means the channel is
    /// dead; the probe reports nothing rather than wait out its budget.
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<()>;
    /// Read the next reply chunk, waiting at most `wait`. `None` means
    /// nothing arrived in time and the probe stops waiting.
    fn read_timeout(&mut self, wait: Duration) -> Option<Vec<u8>>;
}

/// The environment values the sniff reads, captured by the caller or with
/// [`Env::capture`]. Reading the process environment is the only thing
/// this module touches beside its [`Transport`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    /// `TERM`.
    pub term: Option<String>,
    /// `TERM_PROGRAM`.
    pub term_program: Option<String>,
    /// `COLORTERM`.
    pub colorterm: Option<String>,
    /// `KITTY_WINDOW_ID`.
    pub kitty_window_id: Option<String>,
    /// `TMUX`.
    pub tmux: Option<String>,
    /// `RMUX`, set by the Rust rewrite alongside the tmux variables.
    pub rmux: Option<String>,
}

impl Env {
    /// Capture the environment this process runs in.
    pub fn capture() -> Self {
        let var = |key: &str| std::env::var(key).ok();
        Self {
            term: var("TERM"),
            term_program: var("TERM_PROGRAM"),
            colorterm: var("COLORTERM"),
            kitty_window_id: var("KITTY_WINDOW_ID"),
            tmux: var("TMUX"),
            rmux: var("RMUX"),
        }
    }

    /// `TrueColor` detection from `COLORTERM`.
    pub fn has_truecolor(&self) -> bool {
        self.colorterm
            .as_ref()
            .is_some_and(|value| value.contains("truecolor") || value.contains("24bit"))
    }

    /// Kitty graphics markers in the environment. A plain tmux lies
    /// here: its panes inherit the outer terminal's `TERM_PROGRAM`, so
    /// under tmux no marker counts. rmux sets the tmux variables too,
    /// but it re-emits kitty graphics to the outer terminal, so under
    /// rmux the inherited markers count.
    pub fn supports_kitty(&self) -> bool {
        let multiplexer_forwards = self.rmux.is_some() || self.tmux.is_none();
        multiplexer_forwards
            && (self.kitty_window_id.is_some()
                || self.term.as_deref() == Some("xterm-kitty")
                || self.term_program.as_deref() == Some("kitty"))
    }

    /// A dumb or unknown terminal swallows or mangles queries, so probing
    /// stops before it starts there. A multiplexer is worth probing:
    /// tmux 3.8 answers the kitty query itself, older tmux stays silent
    /// and the sniff decides. GNU screen and kin stay excluded.
    pub fn probing_allowed(&self) -> bool {
        let blocked = ["dumb", "unknown", "", "linux"];
        let Some(term) = self.term.as_deref() else {
            return false;
        };
        !(blocked.contains(&term) || term.starts_with("screen"))
    }
}

/// How a capability was established. Probed answers outrank sniffed
/// markers, which outrank whatever a host declared.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProbeSource {
    /// A host application told us what the terminal is.
    #[default]
    Declared,
    /// Environment markers only.
    Sniffed,
    /// The terminal answered the query batch.
    Probed,
}

/// Terminal identity from XTVERSION, in either `name(version)` or
/// `name version` shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// Lowercase terminal name, for example `kitty`.
    pub name: String,
    /// Version, when the terminal reported one.
    pub version: Option<String>,
}

impl std::fmt::Display for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.version {
            Some(version) => write!(f, "{} {}", self.name, version),
            None => f.write_str(&self.name),
        }
    }
}

/// One cell in pixels, the geometry every image placement needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPixels {
    /// Cell width in pixels.
    pub width: u16,
    /// Cell height in pixels.
    pub height: u16,
}

/// What the terminal can do, and how we know.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// Kitty graphics protocol support.
    pub kitty: bool,
    /// Sixel support, from the fourth DA1 parameter.
    pub sixel: bool,
    /// `TrueColor`, from the environment sniff.
    pub truecolor: bool,
    /// Terminal identity, when XTVERSION answered.
    pub identity: Option<Identity>,
    /// Cell size in pixels, when the `CSI 16 t` answer arrived.
    pub cell: Option<CellPixels>,
    /// The strongest source behind any value in this report: a barrier
    /// answer proves the terminal listened, whatever each field says.
    pub trust: ProbeSource,
}

/// The query batch. DA1 comes last and acts as the barrier: a terminal
/// that supports graphics answers the query before it.
/// The image id the probe's query transmits under; answers naming it
/// prove the kitty protocol listened.
const PROBE_IMAGE_ID: u32 = 31;

/// The query batch: the kitty query under [`PROBE_IMAGE_ID`] with its
/// four-pixel RGBA payload, XTGETTCAP, the cell size and DA1 last as
/// the barrier.
fn batch() -> Vec<u8> {
    format!("\x1b_Gi={PROBE_IMAGE_ID},s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[>0q\x1b[16t\x1b[c")
        .into_bytes()
}

/// Everything the reply stream told us.
#[derive(Debug, Default, PartialEq, Eq)]
struct Replies {
    kitty_ok: bool,
    identity: Option<Identity>,
    cell: Option<CellPixels>,
    sixel: bool,
    barrier: bool,
}

/// Probe the terminal over `transport`.
///
/// The sniff seeds the result and probing only ever adds: a marker the
/// environment claims stays claimed, because markers like
/// `KITTY_WINDOW_ID` are near certain and a lost answer proves nothing.
/// Probing is skipped whole under terminals whose names mangle queries
/// (see [`Env::probing_allowed`]); a multiplexer like tmux is worth
/// probing: tmux 3.8 answers the kitty query itself.
pub fn probe(transport: &mut dyn Transport, env: &Env) -> Capabilities {
    let mut caps = sniff(env);
    if !env.probing_allowed() {
        return caps;
    }
    // a dead channel proves nothing beyond the sniff; stop before the
    // budget instead of waiting out a tty that cannot answer
    if transport.write(&batch()).is_err() {
        return caps;
    }
    let mut stream = Vec::new();
    let mut replies = parse_replies(&stream);
    while !replies.barrier {
        let Some(chunk) = transport.read_timeout(READ_WAIT) else {
            break;
        };
        stream.extend_from_slice(&chunk);
        replies = parse_replies(&stream);
    }
    let answered = replies.barrier
        || replies.kitty_ok
        || replies.sixel
        || replies.identity.is_some()
        || replies.cell.is_some();
    if answered {
        caps.trust = ProbeSource::Probed;
    }
    caps.kitty |= replies.kitty_ok;
    caps.sixel |= replies.sixel;
    if let Some(identity) = replies.identity {
        caps.identity = Some(identity);
    }
    if let Some(cell) = replies.cell {
        caps.cell = Some(cell);
    }
    caps
}

/// Seed capabilities from environment markers alone.
pub fn sniff(env: &Env) -> Capabilities {
    Capabilities {
        kitty: env.supports_kitty(),
        sixel: false,
        truecolor: env.has_truecolor(),
        identity: None,
        cell: None,
        trust: ProbeSource::Sniffed,
    }
}

/// Parse every recognized answer out of the stream, ignoring all other
/// bytes. An unterminated sequence ends the scan: the rest belongs to a
/// reply that has not fully arrived.
fn parse_replies(stream: &[u8]) -> Replies {
    // any answer to the marker query proves the protocol, an error reply
    // included, so the scanner never judges APC frames itself
    let kitty_ok = kitty::parse_responses(stream)
        .into_iter()
        .any(|answer| match answer {
            kitty::Response::Ok { image, .. } | kitty::Response::Error { image, .. } => {
                image == kitty::ImageId(PROBE_IMAGE_ID)
            }
        });
    let mut replies = Replies {
        kitty_ok,
        ..Replies::default()
    };
    let mut cursor = 0;
    while let Some(rest) = stream.get(cursor..) {
        if rest.starts_with(b"\x1b_G") {
            let Some(body_len) = body_len(rest, 3) else {
                break;
            };
            cursor += 3 + body_len + 2;
        } else if rest.starts_with(b"\x1bP") {
            let Some(body_len) = body_len(rest, 2) else {
                break;
            };
            if let Some(body) = rest.get(2..2 + body_len)
                && let Some(identity) = parse_identity(body)
            {
                replies.identity = Some(identity);
            }
            cursor += 2 + body_len + 2;
        } else if rest.starts_with(b"\x1b[") {
            let Some(step) = csi_step(rest) else {
                break;
            };
            let (params, final_byte) = csi_split(rest.get(..step).unwrap_or_default());
            match final_byte {
                b'c' => {
                    replies.barrier = true;
                    replies.sixel |= params.strip_prefix('?').is_some_and(has_sixel);
                }
                b't' => {
                    replies.cell = parse_cell(&params).or(replies.cell);
                }
                _ => {}
            }
            cursor += step;
        } else {
            cursor += 1;
        }
    }
    replies
}

/// Length of the body before the `ESC \` terminator, searching from
/// `start`.
fn body_len(rest: &[u8], start: usize) -> Option<usize> {
    let body = rest.get(start..)?;
    body.windows(2).position(|w| w == b"\x1b\\")
}

/// One past the end of this CSI sequence: the position after its final
/// byte, the first byte in the `0x40..=0x7e` range after the introducer.
fn csi_step(rest: &[u8]) -> Option<usize> {
    rest.get(2..)?
        .iter()
        .position(|b| (0x40..=0x7e).contains(b))
        .map(|p| p + 3)
}

/// Split a CSI sequence into parameter bytes and the final byte.
fn csi_split(seq: &[u8]) -> (String, u8) {
    let params: Vec<u8> = seq
        .get(2..seq.len().saturating_sub(1))
        .map(<[u8]>::to_vec)
        .unwrap_or_default();
    let final_byte = seq.last().copied().unwrap_or_default();
    (String::from_utf8_lossy(&params).into_owned(), final_byte)
}

/// DA1 reports sixel as feature parameter 4; some terminals order
/// features freely, so it counts anywhere in the reply.
fn has_sixel(params: &str) -> bool {
    params.split(';').any(|p| p == "4")
}

/// `6;<height>;<width>` from the cell size answer.
fn parse_cell(params: &str) -> Option<CellPixels> {
    let mut fields = params.split(';');
    if fields.next()? != "6" {
        return None;
    }
    let height = fields.next()?.parse().ok()?;
    let width = fields.next()?.parse().ok()?;
    Some(CellPixels { width, height })
}

/// XTVERSION body after `>|`: `name(version)` or `name version`.
fn parse_identity(body: &[u8]) -> Option<Identity> {
    let text = std::str::from_utf8(body).ok()?.trim_start_matches(">|");
    let text = text.trim();
    if text.starts_with('(') || text.is_empty() {
        return None;
    }
    if let Some(open) = text.find('(')
        && text.ends_with(')')
        && let Some(version) = text.get(open + 1..text.len() - 1)
    {
        let name = text.get(..open)?.to_owned();
        if name.is_empty() {
            return None;
        }
        return Some(Identity {
            name,
            version: Some(version.to_owned()),
        });
    }
    let mut fields = text.split_whitespace();
    let name = fields.next()?.to_owned();
    let version = fields.next().map(str::to_owned);
    Some(Identity { name, version })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Capabilities, CellPixels, Env, Identity, Transport, probe, sniff};

    /// A transport scripted with recorded reply chunks.
    struct Scripted {
        chunks: Vec<Vec<u8>>,
        writes: Vec<Vec<u8>>,
    }

    impl Scripted {
        fn new(chunks: &[&[u8]]) -> Self {
            Self {
                chunks: chunks.iter().map(|c| c.to_vec()).collect(),
                writes: Vec::new(),
            }
        }
    }

    impl Transport for Scripted {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
            self.writes.push(bytes.to_vec());
            Ok(())
        }

        fn read_timeout(&mut self, _wait: Duration) -> Option<Vec<u8>> {
            if self.chunks.is_empty() {
                None
            } else {
                Some(self.chunks.remove(0))
            }
        }
    }

    fn env() -> Env {
        Env {
            term: Some("xterm-kitty".to_owned()),
            term_program: None,
            colorterm: Some("truecolor".to_owned()),
            kitty_window_id: None,
            tmux: None,
            rmux: None,
        }
    }

    #[test]
    fn a_full_kitty_session_reports_everything() {
        let mut transport = Scripted::new(&[
            b"\x1b_Gi=31;OK\x1b\\\x1bP>|kitty(0.29.0)\x1b\\\x1b[6;14;7t\x1b[?62;1;2;6;9;22c",
        ]);
        let caps = probe(&mut transport, &env());
        assert_eq!(
            caps,
            Capabilities {
                kitty: true,
                sixel: false,
                truecolor: true,
                identity: Some(Identity {
                    name: "kitty".to_owned(),
                    version: Some("0.29.0".to_owned())
                }),
                cell: Some(CellPixels {
                    width: 7,
                    height: 14
                }),
                trust: super::ProbeSource::Probed,
            }
        );
    }

    #[test]
    fn da1_alone_reports_sixel_and_ends_the_exchange() {
        let mut transport = Scripted::new(&[b"\x1b[?64;1;2;4;6;9;15c"]);
        let caps = probe(&mut transport, &env());
        assert!(caps.sixel);
        assert_eq!(caps.kitty, env().supports_kitty(), "DA1 alone adds nothing");
        assert_eq!(caps.trust, super::ProbeSource::Probed);
    }

    #[test]
    fn an_error_answer_still_proves_the_protocol() {
        let mut transport = Scripted::new(&[b"\x1b_Gi=31;EINVAL:nope\x1b\\\x1b[?62c"]);
        let caps = probe(&mut transport, &env());
        assert!(caps.kitty, "a rejected query still answered in protocol");
        assert_eq!(caps.trust, super::ProbeSource::Probed);
    }

    #[test]
    fn split_and_interleaved_replies_still_parse() {
        let mut transport = Scripted::new(&[
            b"keystrokes\x1b_Gi=31",
            b";OK\x1b\\more\x1bP>|wezterm 2026",
            b"-01-01\x1b\\\x1b[6;20;10t\x1b[?62;4c",
        ]);
        let caps = probe(&mut transport, &env());
        assert!(caps.kitty);
        assert!(caps.sixel);
        assert_eq!(
            caps.identity,
            Some(Identity {
                name: "wezterm".to_owned(),
                version: Some("2026-01-01".to_owned())
            })
        );
        assert_eq!(
            caps.cell,
            Some(CellPixels {
                width: 10,
                height: 20
            })
        );
    }

    #[test]
    fn a_silent_terminal_keeps_the_sniffed_state() {
        let mut transport = Scripted::new(&[]);
        let caps = probe(&mut transport, &env());
        assert_eq!(caps, sniff(&env()));
        assert_eq!(caps.trust, super::ProbeSource::Sniffed);
    }

    #[test]
    fn tmux_answers_the_probe_and_adds_what_it_supports() {
        let mut mux = env();
        mux.tmux = Some("pane".to_owned());
        mux.term = Some("tmux-256color".to_owned());
        // tmux 3.8: the kitty query is answered natively, DA1 reports sixel
        let mut transport =
            Scripted::new(&[b"\x1b_Gi=31;OK\x1b\\\x1bP>|tmux 3.8\x1b\\\x1b[6;20;10t\x1b[?62;4c"]);
        let caps = probe(&mut transport, &mux);
        assert_eq!(caps.trust, super::ProbeSource::Probed);
        assert!(caps.kitty, "tmux 3.8 speaks kitty graphics natively");
        assert!(caps.sixel);
        assert_eq!(
            caps.identity,
            Some(super::Identity {
                name: "tmux".to_owned(),
                version: Some("3.8".to_owned()),
            })
        );
    }

    #[test]
    fn a_silent_multiplexer_falls_back_to_its_sniff() {
        let mut mux = env();
        mux.tmux = Some("pane".to_owned());
        mux.term = Some("tmux-256color".to_owned());
        let mut transport = Scripted::new(&[]);
        let caps = probe(&mut transport, &mux);
        assert_eq!(caps.trust, super::ProbeSource::Sniffed);
        assert!(!caps.kitty, "an older tmux reports no graphics");
        assert!(!transport.writes.is_empty(), "the probe was attempted");
    }

    #[test]
    fn the_query_batch_is_one_exact_byte_string() {
        assert_eq!(
            super::batch(),
            b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[>0q\x1b[16t\x1b[c".to_vec()
        );
    }

    #[test]
    fn a_dead_channel_returns_the_sniff_without_waiting() {
        struct Dead;
        impl Transport for Dead {
            fn write(&mut self, _: &[u8]) -> std::io::Result<()> {
                Err(std::io::Error::other("no device"))
            }
            fn read_timeout(&mut self, _: Duration) -> Option<Vec<u8>> {
                panic!("a failed write must stop the probe before any read")
            }
        }
        let mut dead = Dead;
        let caps = probe(&mut dead, &env());
        assert_eq!(caps.trust, super::ProbeSource::Sniffed);
    }

    #[test]
    fn garbage_and_unterminated_sequences_never_panic() {
        let mut transport = Scripted::new(&[b"\x1b_G\x1b[?u\x1bP>|only\x1b[[[", &[0xff, 0xfe]]);
        let caps = probe(&mut transport, &env());
        assert_eq!(caps.trust, super::ProbeSource::Sniffed);
    }

    #[test]
    fn random_reply_streams_never_panic() {
        let mut rng = crate::fuzz::XorShift::new(0xf00d_beef_cafe_1234);
        for _ in 0..3000 {
            let chunks: Vec<Vec<u8>> = (0..rng.next() % 4)
                .map(|_| {
                    let len = (rng.next() % 96) as usize;
                    let mut bytes: Vec<u8> = (0..len).map(|_| rng.byte()).collect();
                    if rng.next().is_multiple_of(2) {
                        let at = (rng.next() as usize) % (len + 1);
                        bytes.splice(at..at, b"\x1b_Gi=".iter().copied());
                    }
                    bytes
                })
                .collect();
            let refs: Vec<&[u8]> = chunks.iter().map(std::vec::Vec::as_slice).collect();
            let mut transport = Scripted::new(&refs);
            let caps = probe(&mut transport, &env());
            // a barrier among the noise upgrades trust; garbage alone
            // must still answer with the sniff
            let _ = caps.trust;
        }
    }

    #[test]
    fn probing_only_adds_to_the_sniff() {
        let mut env = env();
        env.term = Some("xterm-256color".to_owned());
        env.kitty_window_id = Some("1".to_owned());
        let mut transport = Scripted::new(&[b"\x1b[?64;4c"]);
        let caps = probe(&mut transport, &env);
        assert!(caps.kitty, "the marker survives a lost answer");
        assert!(caps.sixel);
    }

    #[test]
    fn the_sniff_reads_the_markers_it_knows() {
        let env = env();
        let caps = sniff(&env);
        assert!(caps.kitty);
        assert!(caps.truecolor);
        assert!(!caps.sixel);
        assert_eq!(caps.trust, super::ProbeSource::Sniffed);
    }

    #[test]
    fn a_multiplexers_inherited_markers_do_not_count() {
        let mut mux = env();
        mux.tmux = Some("pane".to_owned());
        assert!(
            !mux.supports_kitty(),
            "tmux panes inherit TERM_PROGRAM from the outer terminal"
        );
        assert!(!sniff(&mux).kitty);
    }

    #[test]
    fn rmux_forwards_the_inherited_markers() {
        let mut mux = env();
        mux.tmux = Some("pane".to_owned());
        mux.rmux = Some("pane".to_owned());
        assert!(
            mux.supports_kitty(),
            "rmux re-emits kitty graphics to the outer terminal"
        );
        assert!(sniff(&mux).kitty);
    }

    #[test]
    fn probing_is_blocked_by_terminal_names_that_mangle_queries() {
        for term in ["dumb", "screen-256color", "linux"] {
            let env = Env {
                term: Some(term.to_owned()),
                ..Env::default()
            };
            assert!(!env.probing_allowed(), "{term} must block probing");
        }
        let tmux = Env {
            term: Some("tmux-256color".to_owned()),
            ..Env::default()
        };
        assert!(
            tmux.probing_allowed(),
            "tmux answers the probe itself, so probing runs"
        );
        assert!(
            !Env::default().probing_allowed(),
            "an unset TERM is an unknown terminal"
        );
    }

    #[test]
    fn identity_accepts_both_reported_shapes() {
        assert_eq!(
            super::parse_identity(b">|kitty(0.29.0)"),
            Some(Identity {
                name: "kitty".to_owned(),
                version: Some("0.29.0".to_owned())
            })
        );
        assert_eq!(
            super::parse_identity(b">|vte 0.76"),
            Some(Identity {
                name: "vte".to_owned(),
                version: Some("0.76".to_owned())
            })
        );
        assert_eq!(
            super::parse_identity(b">|xterm"),
            Some(Identity {
                name: "xterm".to_owned(),
                version: None
            })
        );
        assert_eq!(super::parse_identity(b">|(379)"), None);
    }
}
