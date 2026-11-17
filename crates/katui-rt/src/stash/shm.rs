//! Pixels through POSIX shared memory a terminal on this machine reads
//! (`t=s`), so a frame skips the file system as well.

use std::collections::VecDeque;
use std::fs::File;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use katui_present::surface::Stash;
use katui_proto::kitty::Medium;
use memmap2::MmapMut;
use rustix::fs::Mode;
use rustix::shm;

use super::inline;

/// How long an object may wait for the terminal before we unlink it
/// ourselves. The terminal unlinks every object it reads, so we only
/// clean up after one it rejected.
const LINGER: Duration = Duration::from_secs(2);

/// Numbers every object in this process, so two stashes never open the
/// same name.
static NEXT_OBJECT: AtomicU64 = AtomicU64::new(0);

/// Shared memory objects, which the terminal unlinks once it has read
/// them.
#[derive(Debug)]
pub struct SharedMemoryStash {
    prefix: String,
    pending: VecDeque<(String, Instant)>,
}

impl SharedMemoryStash {
    /// A stash whose objects are named for this process.
    #[must_use]
    pub fn new() -> Self {
        Self {
            // macOS caps a name at 31 bytes, so we keep it short
            prefix: format!("/kt{}-", std::process::id()),
            pending: VecDeque::new(),
        }
    }

    /// Copy `pixels` into a fresh object, whatever their size.
    pub(super) fn store(&mut self, pixels: &[u8]) -> io::Result<Medium> {
        let now = Instant::now();
        self.unlink_older_than(now.checked_sub(LINGER).unwrap_or(now));
        let name = format!(
            "{}{}",
            self.prefix,
            NEXT_OBJECT.fetch_add(1, Ordering::Relaxed)
        );
        let object = shm::open(
            name.as_str(),
            shm::OFlags::CREATE | shm::OFlags::EXCL | shm::OFlags::RDWR,
            Mode::RUSR | Mode::WUSR,
        )?;
        self.pending.push_back((name.clone(), now));
        let file = File::from(object);
        file.set_len(pixels.len() as u64)?;
        copy_into(&file, pixels)?;
        Ok(Medium::SharedMemory {
            name,
            size: pixels.len(),
        })
    }

    fn unlink_older_than(&mut self, cutoff: Instant) {
        while let Some((name, _)) = self.pending.front().filter(|(_, at)| *at <= cutoff) {
            // the terminal has usually unlinked it already
            let _ = shm::unlink(name.as_str());
            self.pending.pop_front();
        }
    }
}

/// Write `pixels` at the start of `file` through a mapping, the only way
/// macOS fills a shared memory object.
#[expect(unsafe_code, reason = "macOS fills shared memory only through mmap")]
fn copy_into(file: &File, pixels: &[u8]) -> io::Result<()> {
    // SAFETY: we just created the object with O_EXCL under a name only
    // this process knows, so nothing else resizes or writes it while we
    // hold the mapping.
    let mut map = unsafe { MmapMut::map_mut(file)? };
    let target = map
        .get_mut(..pixels.len())
        .ok_or_else(|| io::Error::other("the shared memory object is smaller than the frame"))?;
    target.copy_from_slice(pixels);
    Ok(())
}

impl Default for SharedMemoryStash {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SharedMemoryStash {
    fn drop(&mut self) {
        for (name, _) in self.pending.drain(..) {
            let _ = shm::unlink(name.as_str());
        }
    }
}

impl Stash for SharedMemoryStash {
    fn put(&mut self, pixels: &[u8]) -> io::Result<Medium> {
        if inline(pixels) {
            return Ok(Medium::Direct);
        }
        self.store(pixels)
    }
}

#[cfg(test)]
mod tests {
    use katui_proto::kitty::Medium;

    use super::SharedMemoryStash;

    #[test]
    fn a_stored_frame_lands_in_shared_memory_and_drop_unlinks_it() {
        let mut stash = SharedMemoryStash::new();
        let Medium::SharedMemory { name, size } = stash
            .store(&[1, 2, 3, 4])
            .expect("shared memory is available")
        else {
            panic!("a shared memory stash hands out shared memory");
        };
        assert_eq!(size, 4);
        assert!(name.len() <= 31, "macOS refuses longer names");
        let object = rustix::shm::open(
            name.as_str(),
            rustix::shm::OFlags::RDONLY,
            rustix::fs::Mode::empty(),
        )
        .expect("the object exists until the terminal or the stash unlinks it");
        let map = {
            #[expect(
                unsafe_code,
                reason = "the test reads the object back as a terminal would"
            )]
            // SAFETY: the object is ours and nothing writes it any more.
            unsafe {
                memmap2::Mmap::map(&std::fs::File::from(object)).expect("the object maps")
            }
        };
        assert_eq!(map.get(..4), Some(&[1, 2, 3, 4][..]));
        drop(stash);
        assert!(
            rustix::shm::open(
                name.as_str(),
                rustix::shm::OFlags::RDONLY,
                rustix::fs::Mode::empty()
            )
            .is_err()
        );
    }
}
