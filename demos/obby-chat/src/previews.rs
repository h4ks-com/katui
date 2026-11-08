//! Image previews: fetched and decoded off the UI thread, kept in memory.

use std::io::{Cursor, Read as _};
use std::net::{IpAddr, ToSocketAddrs as _};
use std::sync::Arc;
use std::time::Duration;

use image::imageops::FilterType;
use image::{ImageReader, Limits};
use katui_scene::surface::Surface;

const MAX_DOWNLOAD: u64 = 8 * 1024 * 1024;
const MAX_DECODED: u64 = 64 * 1024 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_WIDTH: u32 = 420;
const MAX_HEIGHT: u32 = 280;
const IMAGE_ENDINGS: [&str; 5] = [".png", ".jpg", ".jpeg", ".gif", ".webp"];

/// The first link in `text` that points straight at an image.
pub fn image_url(text: &str) -> Option<&str> {
    text.split_whitespace().find(|word| {
        let path = word
            .split(['?', '#'])
            .next()
            .unwrap_or(word)
            .to_ascii_lowercase();
        (word.starts_with("https://") || word.starts_with("http://"))
            && IMAGE_ENDINGS.iter().any(|ending| path.ends_with(ending))
    })
}

pub fn fetch(url: String, report: impl FnOnce(String, Option<Arc<Surface>>) + Send + 'static) {
    std::thread::spawn(move || {
        let surface = download(&url).map(Arc::new);
        report(url, surface);
    });
}

fn download(url: &str) -> Option<Surface> {
    if !host_is_public(url)? {
        return None;
    }
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(FETCH_TIMEOUT))
        .max_redirects(0)
        .build()
        .new_agent();
    let mut response = agent.get(url).call().ok()?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_DOWNLOAD)
        .read_to_end(&mut bytes)
        .ok()?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODED);
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    reader.limits(limits);
    let scaled = reader
        .decode()
        .ok()?
        .resize(MAX_WIDTH, MAX_HEIGHT, FilterType::Triangle)
        .to_rgba8();
    Surface::from_rgba(scaled.width(), scaled.height(), scaled.into_raw())
}

/// Whether every address of the link's host is on the public internet,
/// because anyone in a channel can post a link and we fetch it unasked.
fn host_is_public(url: &str) -> Option<bool> {
    let authority = url.split_once("://")?.1.split(['/', '?', '#']).next()?;
    let host_and_port = authority.rsplit('@').next()?;
    let port = if url.starts_with("https://") { 443 } else { 80 };
    let target = if host_and_port.rsplit(':').next()?.parse::<u16>().is_ok() {
        host_and_port.to_owned()
    } else {
        format!("{host_and_port}:{port}")
    };
    let addresses: Vec<_> = target.to_socket_addrs().ok()?.collect();
    Some(!addresses.is_empty() && addresses.iter().all(|address| is_public(address.ip())))
}

fn is_public(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]))
        }
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || v6
                    .to_ipv4_mapped()
                    .is_some_and(|mapped| !is_public(IpAddr::V4(mapped))))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use super::{image_url, is_public};

    #[test]
    fn links_to_private_addresses_are_never_fetched() {
        for address in [
            "127.0.0.1",
            "10.1.2.3",
            "192.168.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "::1",
            "fe80::1",
            "fd00::1",
            "::ffff:127.0.0.1",
        ] {
            let parsed: IpAddr = address.parse().expect("the test addresses are valid");
            assert!(!is_public(parsed), "{address} must stay unreachable");
        }
        let public: IpAddr = "93.184.216.34".parse().expect("the test address is valid");
        assert!(is_public(public));
    }

    #[test]
    fn only_direct_image_links_preview() {
        assert_eq!(
            image_url("look https://x.io/a.PNG?size=2 nice"),
            Some("https://x.io/a.PNG?size=2")
        );
        assert_eq!(image_url("https://x.io/page.html"), None);
        assert_eq!(image_url("file:///etc/a.png"), None);
    }
}
