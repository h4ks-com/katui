//! One socket per server on its own thread. The protocol engine stays on
//! the UI thread, so this only moves bytes.

use std::io::{self, ErrorKind, Read as _, Write as _};
use std::net::{TcpStream, ToSocketAddrs as _};
use std::sync::Arc;
use std::sync::mpsc::{self, Sender, TryRecvError};
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_POLL: Duration = Duration::from_millis(15);

#[derive(Debug)]
pub enum NetEvent {
    Connected,
    Bytes(Vec<u8>),
    Closed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub tls: bool,
}

/// The sending half of a running connection.
#[derive(Debug, Clone)]
pub struct Connection {
    outgoing: Sender<Vec<u8>>,
}

impl Connection {
    /// Queue bytes for the server; `false` once the socket is gone.
    pub fn send(&self, bytes: Vec<u8>) -> bool {
        self.outgoing.send(bytes).is_ok()
    }
}

pub fn connect(endpoint: Endpoint, report: impl Fn(NetEvent) + Send + 'static) -> Connection {
    let (outgoing, queue) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let reason = match open(&endpoint) {
            Ok(stream) => {
                report(NetEvent::Connected);
                pump(stream, &queue, &report)
            }
            Err(problem) => problem.to_string(),
        };
        report(NetEvent::Closed(reason));
    });
    Connection { outgoing }
}

trait Stream: io::Read + io::Write + Send {}
impl<T: io::Read + io::Write + Send> Stream for T {}

fn open(endpoint: &Endpoint) -> io::Result<Box<dyn Stream>> {
    let address = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "the host has no address"))?;
    let socket = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)?;
    socket.set_nodelay(true)?;
    if !endpoint.tls {
        socket.set_read_timeout(Some(READ_POLL))?;
        return Ok(Box::new(socket));
    }
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(io::Error::other)?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let name = ServerName::try_from(endpoint.host.clone()).map_err(io::Error::other)?;
    let mut session = ClientConnection::new(Arc::new(config), name).map_err(io::Error::other)?;
    let mut socket = socket;
    // we finish the handshake on a long timeout, because a write during
    // it would fail on the short read poll the pump uses afterwards
    socket.set_read_timeout(Some(CONNECT_TIMEOUT))?;
    while session.is_handshaking() {
        session.complete_io(&mut socket)?;
    }
    socket.set_read_timeout(Some(READ_POLL))?;
    Ok(Box::new(StreamOwned::new(session, socket)))
}

fn pump(
    mut stream: Box<dyn Stream>,
    queue: &mpsc::Receiver<Vec<u8>>,
    report: &impl Fn(NetEvent),
) -> String {
    let mut buffer = [0u8; 16 * 1024];
    loop {
        match queue.try_recv() {
            Ok(bytes) => {
                if let Err(problem) = stream.write_all(&bytes).and_then(|()| stream.flush()) {
                    return problem.to_string();
                }
                continue;
            }
            Err(TryRecvError::Disconnected) => return "disconnected".to_owned(),
            Err(TryRecvError::Empty) => {}
        }
        match stream.read(&mut buffer) {
            Ok(0) => return "the server closed the connection".to_owned(),
            Ok(count) => report(NetEvent::Bytes(
                buffer.get(..count).unwrap_or_default().to_vec(),
            )),
            Err(problem)
                if matches!(problem.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(problem) if problem.kind() == ErrorKind::Interrupted => {}
            Err(problem) => return problem.to_string(),
        }
    }
}

/// A connection whose bytes go to `sink`, for driving the client without
/// a socket.
#[cfg(test)]
pub fn scripted(sink: Sender<Vec<u8>>) -> Connection {
    Connection { outgoing: sink }
}
