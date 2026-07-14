//! Draw-aware compact tablebase artifacts for low-memory probing.
//!
//! A one-bit bitmap marks decisive positions. Their result-plus-remoteness
//! codes are stored densely at six bits each; draw positions store no code.
//! A prefix count every 512 positions keeps lookup constant-time.

use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use crate::remoteness::DRAW_CODE;
use crate::tablebase::TableSection;

const MAGIC: [u8; 8] = *b"TTCCMP01";
const VERSION: u32 = 1;
const ENCODING_DECISIVE_BITMAP_SIX_BIT_DISTANCE: u32 = 1;
const HEADER_BYTES: u64 = 56;
const TRAILER_BYTES: u64 = 8;
const BUFFER_BYTES: usize = 8 * 1024 * 1024;
const UNRESOLVED_CODE: u8 = 254;
const MAX_DISTANCE: u8 = 63;
const CRC_TABLE: [u64; 256] = crc_table();

pub const RANK_BLOCK_STATES: u32 = 512;

pub struct CompactTablebaseArtifact {
    rules_tag: u32,
    post: CompactSection,
    opening: CompactSection,
    checksum: u64,
}

impl CompactTablebaseArtifact {
    pub fn load(
        path: impl AsRef<Path>,
        expected_rules_tag: u32,
        expected_post_nodes: u64,
        expected_opening_nodes: u64,
    ) -> Result<Self, CompactTablebaseError> {
        let file = File::open(path)?;
        let file_len = file.metadata()?.len();
        let mut reader = BufReader::with_capacity(BUFFER_BYTES, file);
        let mut crc = Crc64::new();

        let magic = read_array_crc::<8>(&mut reader, &mut crc)?;
        if magic != MAGIC {
            return Err(CompactTablebaseError::BadMagic(magic));
        }
        let version = read_u32_crc(&mut reader, &mut crc)?;
        if version != VERSION {
            return Err(CompactTablebaseError::UnsupportedVersion(version));
        }
        let rules_tag = read_u32_crc(&mut reader, &mut crc)?;
        if rules_tag != expected_rules_tag {
            return Err(CompactTablebaseError::RulesMismatch {
                expected: expected_rules_tag,
                actual: rules_tag,
            });
        }
        let encoding = read_u32_crc(&mut reader, &mut crc)?;
        if encoding != ENCODING_DECISIVE_BITMAP_SIX_BIT_DISTANCE {
            return Err(CompactTablebaseError::UnsupportedEncoding(encoding));
        }
        let rank_block_states = read_u32_crc(&mut reader, &mut crc)?;
        if rank_block_states != RANK_BLOCK_STATES {
            return Err(CompactTablebaseError::UnsupportedRankBlock(
                rank_block_states,
            ));
        }
        let post_nodes = read_u64_crc(&mut reader, &mut crc)?;
        let opening_nodes = read_u64_crc(&mut reader, &mut crc)?;
        if post_nodes != expected_post_nodes || opening_nodes != expected_opening_nodes {
            return Err(CompactTablebaseError::DimensionMismatch {
                expected_post: expected_post_nodes,
                actual_post: post_nodes,
                expected_opening: expected_opening_nodes,
                actual_opening: opening_nodes,
            });
        }
        let post_decisive = read_u64_crc(&mut reader, &mut crc)?;
        let opening_decisive = read_u64_crc(&mut reader, &mut crc)?;
        let post_layout = SectionLayout::new(post_nodes, post_decisive)?;
        let opening_layout = SectionLayout::new(opening_nodes, opening_decisive)?;
        let expected_len = HEADER_BYTES
            .checked_add(post_layout.bytes()?)
            .and_then(|length| length.checked_add(opening_layout.bytes().ok()?))
            .and_then(|length| length.checked_add(TRAILER_BYTES))
            .ok_or(CompactTablebaseError::LengthOverflow)?;
        if file_len != expected_len {
            return Err(CompactTablebaseError::FileLengthMismatch {
                expected: expected_len,
                actual: file_len,
            });
        }

        let post = CompactSection::read(
            &mut reader,
            &mut crc,
            TableSection::PostOpening,
            post_layout,
        )?;
        let opening =
            CompactSection::read(&mut reader, &mut crc, TableSection::Opening, opening_layout)?;
        let expected_crc = u64::from_le_bytes(read_array::<8>(&mut reader)?);
        let actual_crc = crc.finish();
        if actual_crc != expected_crc {
            return Err(CompactTablebaseError::ChecksumMismatch {
                expected: expected_crc,
                actual: actual_crc,
            });
        }

        Ok(Self {
            rules_tag,
            post,
            opening,
            checksum: actual_crc,
        })
    }

    pub const fn rules_tag(&self) -> u32 {
        self.rules_tag
    }

    pub const fn checksum(&self) -> u64 {
        self.checksum
    }

    pub const fn post_nodes(&self) -> u64 {
        self.post.nodes
    }

    pub const fn opening_nodes(&self) -> u64 {
        self.opening.nodes
    }

    pub const fn post_decisive(&self) -> u64 {
        self.post.decisive
    }

    pub const fn opening_decisive(&self) -> u64 {
        self.opening.decisive
    }

    pub fn post_code(&self, index: u64) -> u8 {
        self.post.code(index)
    }

    pub fn opening_code(&self, index: u64) -> u8 {
        self.opening.code(index)
    }
}

pub fn save_atomic(
    path: impl AsRef<Path>,
    rules_tag: u32,
    post_codes: &[u8],
    opening_codes: &[u8],
) -> Result<u64, CompactTablebaseError> {
    let post = CompactSection::encode(post_codes, TableSection::PostOpening)?;
    let opening = CompactSection::encode(opening_codes, TableSection::Opening)?;
    let path = path.as_ref();
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let temporary = temporary_path(path);
    let file = File::create(&temporary)?;
    let mut writer = BufWriter::with_capacity(BUFFER_BYTES, file);
    let mut crc = Crc64::new();

    write_crc(&mut writer, &mut crc, &MAGIC)?;
    write_crc(&mut writer, &mut crc, &VERSION.to_le_bytes())?;
    write_crc(&mut writer, &mut crc, &rules_tag.to_le_bytes())?;
    write_crc(
        &mut writer,
        &mut crc,
        &ENCODING_DECISIVE_BITMAP_SIX_BIT_DISTANCE.to_le_bytes(),
    )?;
    write_crc(&mut writer, &mut crc, &RANK_BLOCK_STATES.to_le_bytes())?;
    write_crc(&mut writer, &mut crc, &post.nodes.to_le_bytes())?;
    write_crc(&mut writer, &mut crc, &opening.nodes.to_le_bytes())?;
    write_crc(&mut writer, &mut crc, &post.decisive.to_le_bytes())?;
    write_crc(&mut writer, &mut crc, &opening.decisive.to_le_bytes())?;
    post.write(&mut writer, &mut crc)?;
    opening.write(&mut writer, &mut crc)?;
    let checksum = crc.finish();
    writer.write_all(&checksum.to_le_bytes())?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    drop(writer);

    fs::rename(&temporary, path)?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        File::open(parent)?.sync_all()?;
    }
    Ok(checksum)
}

struct CompactSection {
    nodes: u64,
    decisive: u64,
    bitmap: Vec<u8>,
    ranks: Vec<u32>,
    distances: Vec<u8>,
}

impl CompactSection {
    fn encode(codes: &[u8], section: TableSection) -> Result<Self, CompactTablebaseError> {
        let nodes = codes.len() as u64;
        let decisive = codes.iter().filter(|&&code| code != DRAW_CODE).count() as u64;
        let layout = SectionLayout::new(nodes, decisive)?;
        let mut bitmap = vec![0; layout.bitmap_bytes];
        let mut ranks = Vec::with_capacity(layout.rank_entries);
        let mut distances = vec![0; layout.distance_bytes];
        let mut decisive_before = 0_u64;

        for (index, &code) in codes.iter().enumerate() {
            if index % RANK_BLOCK_STATES as usize == 0 {
                ranks.push(
                    u32::try_from(decisive_before)
                        .map_err(|_| CompactTablebaseError::LengthOverflow)?,
                );
            }
            if code == DRAW_CODE {
                continue;
            }
            if code == UNRESOLVED_CODE {
                return Err(CompactTablebaseError::UnresolvedCode {
                    section,
                    node: index as u64,
                });
            }
            if code > MAX_DISTANCE {
                return Err(CompactTablebaseError::DistanceTooLarge {
                    section,
                    node: index as u64,
                    distance: code,
                });
            }
            set_bit(&mut bitmap, index as u64);
            set_six_bits(&mut distances, decisive_before, code);
            decisive_before += 1;
        }
        debug_assert_eq!(decisive_before, decisive);
        debug_assert_eq!(ranks.len(), layout.rank_entries);
        Ok(Self {
            nodes,
            decisive,
            bitmap,
            ranks,
            distances,
        })
    }

    fn read(
        reader: &mut impl Read,
        crc: &mut Crc64,
        section: TableSection,
        layout: SectionLayout,
    ) -> Result<Self, CompactTablebaseError> {
        let mut bitmap = vec![0; layout.bitmap_bytes];
        read_crc(reader, crc, &mut bitmap)?;
        let mut ranks = Vec::with_capacity(layout.rank_entries);
        for _ in 0..layout.rank_entries {
            ranks.push(read_u32_crc(reader, crc)?);
        }
        let mut distances = vec![0; layout.distance_bytes];
        read_crc(reader, crc, &mut distances)?;
        let result = Self {
            nodes: layout.nodes,
            decisive: layout.decisive,
            bitmap,
            ranks,
            distances,
        };
        result.validate(section)?;
        Ok(result)
    }

    fn write(&self, writer: &mut impl Write, crc: &mut Crc64) -> io::Result<()> {
        write_crc(writer, crc, &self.bitmap)?;
        for &rank in &self.ranks {
            write_crc(writer, crc, &rank.to_le_bytes())?;
        }
        write_crc(writer, crc, &self.distances)
    }

    fn validate(&self, section: TableSection) -> Result<(), CompactTablebaseError> {
        if trailing_bits_nonzero(&self.bitmap, self.nodes) {
            return Err(CompactTablebaseError::NonzeroPadding { section });
        }
        let mut decisive_before = 0_u64;
        for (block, &rank) in self.ranks.iter().enumerate() {
            if u64::from(rank) != decisive_before {
                return Err(CompactTablebaseError::InvalidRank {
                    section,
                    block: block as u64,
                    expected: decisive_before,
                    actual: u64::from(rank),
                });
            }
            let start = block * RANK_BLOCK_STATES as usize;
            let end = ((block + 1) * RANK_BLOCK_STATES as usize).min(self.nodes as usize);
            decisive_before += count_bits(&self.bitmap, start as u64, end as u64);
        }
        if decisive_before != self.decisive {
            return Err(CompactTablebaseError::DecisiveCountMismatch {
                section,
                expected: self.decisive,
                actual: decisive_before,
            });
        }
        if trailing_bits_nonzero(&self.distances, self.decisive * 6) {
            return Err(CompactTablebaseError::NonzeroPadding { section });
        }
        Ok(())
    }

    fn code(&self, index: u64) -> u8 {
        assert!(index < self.nodes, "compact tablebase index out of range");
        if !bit_is_set(&self.bitmap, index) {
            return DRAW_CODE;
        }
        let block = index / u64::from(RANK_BLOCK_STATES);
        let block_start = block * u64::from(RANK_BLOCK_STATES);
        let rank =
            u64::from(self.ranks[block as usize]) + count_bits(&self.bitmap, block_start, index);
        get_six_bits(&self.distances, rank)
    }
}

#[derive(Clone, Copy)]
struct SectionLayout {
    nodes: u64,
    decisive: u64,
    bitmap_bytes: usize,
    rank_entries: usize,
    distance_bytes: usize,
}

impl SectionLayout {
    fn new(nodes: u64, decisive: u64) -> Result<Self, CompactTablebaseError> {
        if decisive > nodes || decisive > u64::from(u32::MAX) {
            return Err(CompactTablebaseError::LengthOverflow);
        }
        Ok(Self {
            nodes,
            decisive,
            bitmap_bytes: usize_from_bits(nodes)?,
            rank_entries: usize::try_from(nodes.div_ceil(u64::from(RANK_BLOCK_STATES)))
                .map_err(|_| CompactTablebaseError::LengthOverflow)?,
            distance_bytes: usize_from_bits(
                decisive
                    .checked_mul(6)
                    .ok_or(CompactTablebaseError::LengthOverflow)?,
            )?,
        })
    }

    fn bytes(self) -> Result<u64, CompactTablebaseError> {
        let rank_bytes = self
            .rank_entries
            .checked_mul(std::mem::size_of::<u32>())
            .ok_or(CompactTablebaseError::LengthOverflow)?;
        self.bitmap_bytes
            .checked_add(rank_bytes)
            .and_then(|bytes| bytes.checked_add(self.distance_bytes))
            .and_then(|bytes| u64::try_from(bytes).ok())
            .ok_or(CompactTablebaseError::LengthOverflow)
    }
}

fn usize_from_bits(bits: u64) -> Result<usize, CompactTablebaseError> {
    usize::try_from(bits.div_ceil(8)).map_err(|_| CompactTablebaseError::LengthOverflow)
}

fn set_bit(bytes: &mut [u8], index: u64) {
    bytes[(index / 8) as usize] |= 1 << (index % 8);
}

fn bit_is_set(bytes: &[u8], index: u64) -> bool {
    bytes[(index / 8) as usize] & (1 << (index % 8)) != 0
}

fn count_bits(bytes: &[u8], start: u64, end: u64) -> u64 {
    debug_assert!(start <= end);
    let mut count = 0;
    let mut index = start;
    while index < end && !index.is_multiple_of(8) {
        count += u64::from(bit_is_set(bytes, index));
        index += 1;
    }
    while index + 8 <= end {
        count += u64::from(bytes[(index / 8) as usize].count_ones());
        index += 8;
    }
    while index < end {
        count += u64::from(bit_is_set(bytes, index));
        index += 1;
    }
    count
}

fn set_six_bits(bytes: &mut [u8], index: u64, value: u8) {
    let bit = index * 6;
    let byte = (bit / 8) as usize;
    let shift = (bit % 8) as u32;
    let shifted = u16::from(value) << shift;
    bytes[byte] |= shifted as u8;
    if shift > 2 {
        bytes[byte + 1] |= (shifted >> 8) as u8;
    }
}

fn get_six_bits(bytes: &[u8], index: u64) -> u8 {
    let bit = index * 6;
    let byte = (bit / 8) as usize;
    let shift = (bit % 8) as u32;
    let mut value = u16::from(bytes[byte]) >> shift;
    if shift > 2 {
        value |= u16::from(bytes[byte + 1]) << (8 - shift);
    }
    (value & 0x3f) as u8
}

fn trailing_bits_nonzero(bytes: &[u8], used_bits: u64) -> bool {
    if bytes.is_empty() || used_bits.is_multiple_of(8) {
        return false;
    }
    let mask = !((1_u8 << (used_bits % 8)) - 1);
    bytes[bytes.len() - 1] & mask != 0
}

fn write_crc(writer: &mut impl Write, crc: &mut Crc64, bytes: &[u8]) -> io::Result<()> {
    writer.write_all(bytes)?;
    crc.update(bytes);
    Ok(())
}

fn read_crc(reader: &mut impl Read, crc: &mut Crc64, bytes: &mut [u8]) -> io::Result<()> {
    reader.read_exact(bytes)?;
    crc.update(bytes);
    Ok(())
}

fn read_u32_crc(reader: &mut impl Read, crc: &mut Crc64) -> io::Result<u32> {
    Ok(u32::from_le_bytes(read_array_crc(reader, crc)?))
}

fn read_u64_crc(reader: &mut impl Read, crc: &mut Crc64) -> io::Result<u64> {
    Ok(u64::from_le_bytes(read_array_crc(reader, crc)?))
}

fn read_array_crc<const N: usize>(reader: &mut impl Read, crc: &mut Crc64) -> io::Result<[u8; N]> {
    let bytes = read_array(reader)?;
    crc.update(&bytes);
    Ok(bytes)
}

fn read_array<const N: usize>(reader: &mut impl Read) -> io::Result<[u8; N]> {
    let mut bytes = [0; N];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut temporary = OsString::from(path.as_os_str());
    temporary.push(".tmp");
    PathBuf::from(temporary)
}

struct Crc64(u64);

impl Crc64 {
    const fn new() -> Self {
        Self(u64::MAX)
    }

    fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            let index = (self.0 as u8 ^ byte) as usize;
            self.0 = CRC_TABLE[index] ^ (self.0 >> 8);
        }
    }

    const fn finish(self) -> u64 {
        !self.0
    }
}

const fn crc_table() -> [u64; 256] {
    let mut table = [0; 256];
    let mut index = 0;
    while index < 256 {
        let mut value = index as u64;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 != 0 {
                0xc96c_5795_d787_0f42 ^ (value >> 1)
            } else {
                value >> 1
            };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
}

#[derive(Debug)]
pub enum CompactTablebaseError {
    Io(io::Error),
    BadMagic([u8; 8]),
    UnsupportedVersion(u32),
    RulesMismatch {
        expected: u32,
        actual: u32,
    },
    UnsupportedEncoding(u32),
    UnsupportedRankBlock(u32),
    DimensionMismatch {
        expected_post: u64,
        actual_post: u64,
        expected_opening: u64,
        actual_opening: u64,
    },
    FileLengthMismatch {
        expected: u64,
        actual: u64,
    },
    LengthOverflow,
    UnresolvedCode {
        section: TableSection,
        node: u64,
    },
    DistanceTooLarge {
        section: TableSection,
        node: u64,
        distance: u8,
    },
    InvalidRank {
        section: TableSection,
        block: u64,
        expected: u64,
        actual: u64,
    },
    DecisiveCountMismatch {
        section: TableSection,
        expected: u64,
        actual: u64,
    },
    NonzeroPadding {
        section: TableSection,
    },
    ChecksumMismatch {
        expected: u64,
        actual: u64,
    },
}

impl fmt::Display for CompactTablebaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "compact tablebase I/O error: {error}"),
            other => write!(formatter, "invalid compact tablebase: {other:?}"),
        }
    }
}

impl std::error::Error for CompactTablebaseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for CompactTablebaseError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn compact_artifact_round_trips_across_rank_blocks() {
        let path = test_path("round-trip");
        let mut post = vec![DRAW_CODE; 1_027];
        for (index, code) in [(0, 0), (1, 1), (511, 2), (512, 3), (1_026, 63)] {
            post[index] = code;
        }
        let opening = [DRAW_CODE, 4, DRAW_CODE, 5];
        let checksum = save_atomic(&path, 17, &post, &opening).unwrap();
        let artifact = CompactTablebaseArtifact::load(&path, 17, 1_027, 4).unwrap();
        assert_eq!(artifact.rules_tag(), 17);
        assert_eq!(artifact.post_decisive(), 5);
        assert_eq!(artifact.opening_decisive(), 2);
        assert_eq!(artifact.checksum(), checksum);
        for (index, &expected) in post.iter().enumerate() {
            assert_eq!(artifact.post_code(index as u64), expected);
        }
        for (index, &expected) in opening.iter().enumerate() {
            assert_eq!(artifact.opening_code(index as u64), expected);
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn compact_checksum_detects_corruption() {
        let path = test_path("corrupt");
        save_atomic(&path, 17, &[0, DRAW_CODE, 1], &[2, 3]).unwrap();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.seek(SeekFrom::End(-1)).unwrap();
        let mut byte = [0];
        file.read_exact(&mut byte).unwrap();
        file.seek(SeekFrom::End(-1)).unwrap();
        file.write_all(&[byte[0] ^ 0x80]).unwrap();
        file.sync_all().unwrap();
        assert!(matches!(
            CompactTablebaseArtifact::load(&path, 17, 3, 2),
            Err(CompactTablebaseError::ChecksumMismatch { .. })
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn compact_encoding_rejects_unrepresentable_distance() {
        let path = test_path("distance");
        assert!(matches!(
            save_atomic(&path, 17, &[64], &[]),
            Err(CompactTablebaseError::DistanceTooLarge { .. })
        ));
    }

    fn test_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "tic-tac-chec-compact-{label}-{}-{nonce}.ttb",
            std::process::id()
        ))
    }
}
