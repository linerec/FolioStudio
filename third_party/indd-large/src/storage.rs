//! Bounded read-only storage. A direct-mapped 8 MiB page cache never grows.
use crate::Error;
use std::{
    borrow::Cow,
    cell::RefCell,
    fs::File,
    io::{Read, Seek, SeekFrom},
    ops::Range,
};
const PAGE: usize = 4096;
const SLOTS: usize = 2048;
pub(crate) enum Storage<'a> {
    Slice(&'a [u8]),
    File(RefCell<FileCache>),
}
pub(crate) struct FileCache {
    file: File,
    len: usize,
    slots: Vec<Option<(usize, Box<[u8; PAGE]>)>>,
    read_bytes: u64,
    misses: u64,
}
impl<'a> Storage<'a> {
    pub fn file(file: File) -> Result<Self, Error> {
        let len = usize::try_from(file.metadata()?.len())
            .map_err(|_| Error::Corrupt("file length outside address range".into()))?;
        if !(2 * PAGE..=2 * 1024 * 1024 * 1024).contains(&len) {
            return Err(Error::Corrupt(
                "file size outside 8 KiB..2 GiB bound".into(),
            ));
        }
        let mut slots = Vec::with_capacity(SLOTS);
        slots.resize_with(SLOTS, || None);
        Ok(Self::File(RefCell::new(FileCache {
            file,
            len,
            slots,
            read_bytes: 0,
            misses: 0,
        })))
    }
    pub fn len(&self) -> usize {
        match self {
            Self::Slice(b) => b.len(),
            Self::File(f) => f.borrow().len,
        }
    }
    pub fn stats(&self) -> (u64, u64) {
        match self {
            Self::Slice(_) => (0, 0),
            Self::File(f) => {
                let f = f.borrow();
                (f.read_bytes, f.misses)
            }
        }
    }
    pub fn read(&self, range: Range<usize>) -> Result<Cow<'_, [u8]>, Error> {
        if range.end < range.start || range.end > self.len() {
            return Err(Error::Corrupt(format!(
                "read outside file at {}..{}",
                range.start, range.end
            )));
        }
        match self {
            Self::Slice(bytes) => Ok(Cow::Borrowed(&bytes[range])),
            Self::File(cache) => {
                let mut cache = cache.borrow_mut();
                let mut out = Vec::with_capacity(range.len());
                let mut pos = range.start;
                while pos < range.end {
                    let page = pos / PAGE;
                    let slot = page % SLOTS;
                    if cache.slots[slot].as_ref().map(|s| s.0) != Some(page) {
                        let mut bytes = Box::new([0u8; PAGE]);
                        let count = (cache.len - page * PAGE).min(PAGE);
                        cache.file.seek(SeekFrom::Start((page * PAGE) as u64))?;
                        cache.file.read_exact(&mut bytes[..count])?;
                        cache.read_bytes += count as u64;
                        cache.misses += 1;
                        cache.slots[slot] = Some((page, bytes));
                    }
                    let start = pos % PAGE;
                    let count = (PAGE - start).min(range.end - pos);
                    let bytes = &cache.slots[slot]
                        .as_ref()
                        .ok_or_else(|| Error::Corrupt("cache ownership lost".into()))?
                        .1;
                    out.extend_from_slice(&bytes[start..start + count]);
                    pos += count;
                }
                Ok(Cow::Owned(out))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_cache_preserves_cross_page_reads_and_bounds_its_storage() {
        let path = std::env::temp_dir().join(format!(
            "folio-cache-test-{}-{}.bin",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bytes: Vec<u8> = (0..PAGE * (SLOTS + 2)).map(|i| (i % 251) as u8).collect();
        {
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .unwrap();
            std::io::Write::write_all(&mut file, &bytes).unwrap();
        }
        let source = Storage::file(File::open(&path).unwrap()).unwrap();
        assert_eq!(
            &*source.read(PAGE - 3..PAGE + 7).unwrap(),
            &bytes[PAGE - 3..PAGE + 7]
        );
        source.read(0..4).unwrap();
        source.read(SLOTS * PAGE..SLOTS * PAGE + 4).unwrap();
        assert_eq!(&*source.read(0..4).unwrap(), &bytes[..4]);
        assert!(source.read(bytes.len() - 2..bytes.len() + 1).is_err());
        if let Storage::File(cache) = &source {
            assert_eq!(cache.borrow().slots.len(), SLOTS);
            assert!(cache.borrow().slots.iter().filter(|s| s.is_some()).count() <= SLOTS);
        }
        drop(source);
        std::fs::remove_file(path).unwrap();
    }
}
