//! Bounded index paging for an already leased local generation. Graph records
//! stay exact: the App retains them, so speculative graph pages duplicate work.
use std::{
    collections::{BTreeMap, VecDeque},
    io::{Read, Seek, SeekFrom},
};

const PAGE_BYTES: u64 = 4096;
const MAX_PAGES: usize = 512;

pub(super) struct LocalPc4File<R> {
    source: R,
    length: u64,
    cache_index: bool,
    pages: BTreeMap<u64, Vec<u8>>,
    insertion_order: VecDeque<u64>,
}

impl<R: Read + Seek> LocalPc4File<R> {
    pub(super) fn new(source: R, length: u64, cache_index: bool) -> Self {
        Self {
            source,
            length,
            cache_index,
            pages: BTreeMap::new(),
            insertion_order: VecDeque::new(),
        }
    }

    pub(super) fn read(&mut self, offset: u64, length: u64) -> super::super::Result<Vec<u8>> {
        if length == 0
            || length > 65536
            || offset
                .checked_add(length)
                .is_none_or(|end| end > self.length)
        {
            return Err("tablebase: invalid local slice");
        }
        if !self.cache_index {
            return self.read_exact_at(offset, length);
        }
        let mut result = vec![0; length as usize];
        let end = offset + length;
        let mut start = offset / PAGE_BYTES * PAGE_BYTES;
        while start < end {
            if !self.pages.contains_key(&start) {
                let bytes = self.read_exact_at(start, PAGE_BYTES.min(self.length - start))?;
                if self.pages.len() == MAX_PAGES {
                    let old = self
                        .insertion_order
                        .pop_front()
                        .expect("one page owner per key");
                    self.pages.remove(&old);
                }
                self.pages.insert(start, bytes);
                self.insertion_order.push_back(start);
            }
            let bytes = &self.pages[&start];
            let begin = offset.max(start);
            let finish = end.min(start + bytes.len() as u64);
            result[(begin - offset) as usize..(finish - offset) as usize]
                .copy_from_slice(&bytes[(begin - start) as usize..(finish - start) as usize]);
            start += PAGE_BYTES;
        }
        Ok(result)
    }

    fn read_exact_at(&mut self, offset: u64, length: u64) -> super::super::Result<Vec<u8>> {
        self.source
            .seek(SeekFrom::Start(offset))
            .map_err(|_| "tablebase: cannot seek local artifact")?;
        let mut bytes = vec![0; length as usize];
        self.source
            .read_exact(&mut bytes)
            .map_err(|_| "tablebase: local artifact was truncated")?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Result};
    struct Counted {
        source: Cursor<Vec<u8>>,
        reads: usize,
        bytes: usize,
    }
    impl Read for Counted {
        fn read(&mut self, output: &mut [u8]) -> Result<usize> {
            self.reads += 1;
            let count = self.source.read(output)?;
            self.bytes += count;
            Ok(count)
        }
    }
    impl Seek for Counted {
        fn seek(&mut self, position: SeekFrom) -> Result<u64> {
            self.source.seek(position)
        }
    }
    fn reader(size: usize, cache: bool) -> LocalPc4File<Counted> {
        LocalPc4File::new(
            Counted {
                source: Cursor::new((0..size).map(|i| (i % 251) as u8).collect()),
                reads: 0,
                bytes: 0,
            },
            size as u64,
            cache,
        )
    }
    #[test]
    fn tablebase_local_index_repeated_reads_are_exact_and_paged() {
        let mut reader = reader(131089, true);
        for index in 0..20000_u64 {
            let offset = index * 17 % (131089 - 34);
            assert_eq!(
                reader.read(offset, 34).unwrap(),
                (offset..offset + 34)
                    .map(|n| (n % 251) as u8)
                    .collect::<Vec<_>>()
            );
        }
        assert!(reader.source.reads <= 33);
        assert!(reader.pages.len() <= MAX_PAGES);
    }
    #[test]
    fn tablebase_local_graph_does_not_read_unrelated_records() {
        let mut reader = reader(100000, false);
        assert_eq!(reader.read(99988, 12).unwrap().len(), 12);
        assert_eq!(reader.source.bytes, 12);
        assert!(reader.pages.is_empty());
    }
    #[test]
    fn tablebase_local_short_file_and_bounds_do_not_poison_cache() {
        let mut reader = reader(16, true);
        reader.length = 4096;
        assert!(reader.read(0, 8).is_err());
        assert!(reader.pages.is_empty());
        assert!(reader.read(u64::MAX, 2).is_err());
        assert!(reader.read(0, 0).is_err());
    }
    #[test]
    fn tablebase_local_paging_retention_is_bounded_after_eviction() {
        let mut reader = reader((MAX_PAGES + 4) * PAGE_BYTES as usize, true);
        for i in 0..MAX_PAGES + 4 {
            assert_eq!(reader.read(i as u64 * PAGE_BYTES, 1).unwrap().len(), 1);
        }
        assert_eq!(reader.pages.len(), MAX_PAGES);
        assert_eq!(reader.insertion_order.len(), MAX_PAGES);
        assert!(!reader.pages.contains_key(&0));
        assert_eq!(reader.read(0, 1).unwrap(), vec![0]);
    }
}
