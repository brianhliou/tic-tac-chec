//! Versioned, checksummed wave-boundary checkpoints.

use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: [u8; 8] = *b"TTCCP001";
const VERSION: u32 = 1;
const FIXED_BYTES: u64 = 60;
const BUFFER_BYTES: usize = 8 * 1024 * 1024;
const FRONTIER_CHUNK_IDS: usize = 16 * 1024;
const CRC_TABLE: [u64; 256] = crc_table();

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checkpoint {
    pub rules_tag: u32,
    pub wave: u64,
    pub values: Vec<u8>,
    pub remaining: Vec<u8>,
    pub frontier: Vec<u32>,
}

impl Checkpoint {
    pub fn new(
        rules_tag: u32,
        wave: u64,
        values: Vec<u8>,
        remaining: Vec<u8>,
        frontier: Vec<u32>,
    ) -> Result<Self, CheckpointError> {
        let checkpoint = Self {
            rules_tag,
            wave,
            values,
            remaining,
            frontier,
        };
        checkpoint.validate()?;
        Ok(checkpoint)
    }

    pub fn node_count(&self) -> u32 {
        self.values
            .len()
            .try_into()
            .expect("validated checkpoint node count fits u32")
    }

    pub fn save_atomic(&self, path: impl AsRef<Path>) -> Result<(), CheckpointError> {
        save_atomic(
            path,
            self.rules_tag,
            self.wave,
            &self.values,
            &self.remaining,
            &self.frontier,
        )
    }

    pub fn load(
        path: impl AsRef<Path>,
        expected_node_count: u32,
        expected_rules_tag: u32,
    ) -> Result<Self, CheckpointError> {
        let file = File::open(path)?;
        let file_len = file.metadata()?.len();
        let mut reader = BufReader::with_capacity(BUFFER_BYTES, file);
        let mut crc = Crc64::new();

        let magic = read_array_crc::<8>(&mut reader, &mut crc)?;
        if magic != MAGIC {
            return Err(CheckpointError::BadMagic(magic));
        }
        let version = u32::from_le_bytes(read_array_crc(&mut reader, &mut crc)?);
        if version != VERSION {
            return Err(CheckpointError::UnsupportedVersion(version));
        }
        let node_count = u32::from_le_bytes(read_array_crc(&mut reader, &mut crc)?);
        if node_count != expected_node_count {
            return Err(CheckpointError::NodeCountMismatch {
                expected: expected_node_count,
                actual: node_count,
            });
        }
        let rules_tag = u32::from_le_bytes(read_array_crc(&mut reader, &mut crc)?);
        if rules_tag != expected_rules_tag {
            return Err(CheckpointError::RulesMismatch {
                expected: expected_rules_tag,
                actual: rules_tag,
            });
        }
        let wave = u64::from_le_bytes(read_array_crc(&mut reader, &mut crc)?);
        let values_len = u64::from_le_bytes(read_array_crc(&mut reader, &mut crc)?);
        let remaining_len = u64::from_le_bytes(read_array_crc(&mut reader, &mut crc)?);
        let frontier_len = u64::from_le_bytes(read_array_crc(&mut reader, &mut crc)?);
        if values_len != node_count as u64 || remaining_len != node_count as u64 {
            return Err(CheckpointError::InvalidTableLengths {
                node_count,
                values: values_len,
                remaining: remaining_len,
            });
        }
        if frontier_len > node_count as u64 {
            return Err(CheckpointError::FrontierTooLarge {
                node_count,
                frontier: frontier_len,
            });
        }
        let expected_len = FIXED_BYTES
            .checked_add(values_len)
            .and_then(|length| length.checked_add(remaining_len))
            .and_then(|length| length.checked_add(frontier_len.checked_mul(4)?))
            .ok_or(CheckpointError::LengthOverflow)?;
        if file_len != expected_len {
            return Err(CheckpointError::FileLengthMismatch {
                expected: expected_len,
                actual: file_len,
            });
        }

        let values_size =
            usize::try_from(values_len).map_err(|_| CheckpointError::LengthOverflow)?;
        let remaining_size =
            usize::try_from(remaining_len).map_err(|_| CheckpointError::LengthOverflow)?;
        let frontier_size =
            usize::try_from(frontier_len).map_err(|_| CheckpointError::LengthOverflow)?;
        let mut values = vec![0; values_size];
        let mut remaining = vec![0; remaining_size];
        read_crc(&mut reader, &mut crc, &mut values)?;
        read_crc(&mut reader, &mut crc, &mut remaining)?;
        let frontier = read_frontier(&mut reader, &mut crc, frontier_size, node_count)?;
        let expected_crc = u64::from_le_bytes(read_array::<8>(&mut reader)?);
        let actual_crc = crc.finish();
        if actual_crc != expected_crc {
            return Err(CheckpointError::ChecksumMismatch {
                expected: expected_crc,
                actual: actual_crc,
            });
        }
        if let Some((node, &value)) = values.iter().enumerate().find(|(_, value)| **value > 3) {
            return Err(CheckpointError::InvalidValue {
                node: node as u32,
                value,
            });
        }

        Self::new(rules_tag, wave, values, remaining, frontier)
    }

    fn validate(&self) -> Result<(), CheckpointError> {
        validate_parts(&self.values, &self.remaining, &self.frontier).map(|_| ())
    }
}

pub fn save_atomic(
    path: impl AsRef<Path>,
    rules_tag: u32,
    wave: u64,
    values: &[u8],
    remaining: &[u8],
    frontier: &[u32],
) -> Result<(), CheckpointError> {
    let node_count = validate_parts(values, remaining, frontier)?;
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
    write_crc(&mut writer, &mut crc, &node_count.to_le_bytes())?;
    write_crc(&mut writer, &mut crc, &rules_tag.to_le_bytes())?;
    write_crc(&mut writer, &mut crc, &wave.to_le_bytes())?;
    write_crc(&mut writer, &mut crc, &(values.len() as u64).to_le_bytes())?;
    write_crc(
        &mut writer,
        &mut crc,
        &(remaining.len() as u64).to_le_bytes(),
    )?;
    write_crc(
        &mut writer,
        &mut crc,
        &(frontier.len() as u64).to_le_bytes(),
    )?;
    write_crc(&mut writer, &mut crc, values)?;
    write_crc(&mut writer, &mut crc, remaining)?;
    write_frontier(&mut writer, &mut crc, frontier)?;
    writer.write_all(&crc.finish().to_le_bytes())?;
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
    Ok(())
}

fn validate_parts(
    values: &[u8],
    remaining: &[u8],
    frontier: &[u32],
) -> Result<u32, CheckpointError> {
    let node_count = u32::try_from(values.len()).map_err(|_| CheckpointError::LengthOverflow)?;
    if remaining.len() != values.len() {
        return Err(CheckpointError::InvalidTableLengths {
            node_count,
            values: values.len() as u64,
            remaining: remaining.len() as u64,
        });
    }
    if frontier.len() > values.len() {
        return Err(CheckpointError::FrontierTooLarge {
            node_count,
            frontier: frontier.len() as u64,
        });
    }
    if let Some((node, &value)) = values.iter().enumerate().find(|(_, value)| **value > 3) {
        return Err(CheckpointError::InvalidValue {
            node: node as u32,
            value,
        });
    }
    if let Some(&node) = frontier.iter().find(|&&node| node >= node_count) {
        return Err(CheckpointError::FrontierNodeOutOfRange { node, node_count });
    }
    Ok(node_count)
}

#[derive(Debug)]
pub enum CheckpointError {
    Io(io::Error),
    BadMagic([u8; 8]),
    UnsupportedVersion(u32),
    NodeCountMismatch {
        expected: u32,
        actual: u32,
    },
    RulesMismatch {
        expected: u32,
        actual: u32,
    },
    InvalidTableLengths {
        node_count: u32,
        values: u64,
        remaining: u64,
    },
    FrontierTooLarge {
        node_count: u32,
        frontier: u64,
    },
    FrontierNodeOutOfRange {
        node: u32,
        node_count: u32,
    },
    InvalidValue {
        node: u32,
        value: u8,
    },
    FileLengthMismatch {
        expected: u64,
        actual: u64,
    },
    LengthOverflow,
    ChecksumMismatch {
        expected: u64,
        actual: u64,
    },
}

impl fmt::Display for CheckpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "checkpoint I/O error: {error}"),
            other => write!(formatter, "invalid checkpoint: {other:?}"),
        }
    }
}

impl std::error::Error for CheckpointError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for CheckpointError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

fn write_frontier(writer: &mut impl Write, crc: &mut Crc64, frontier: &[u32]) -> io::Result<()> {
    let mut bytes = vec![0_u8; FRONTIER_CHUNK_IDS * 4];
    for chunk in frontier.chunks(FRONTIER_CHUNK_IDS) {
        for (index, node) in chunk.iter().enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&node.to_le_bytes());
        }
        write_crc(writer, crc, &bytes[..chunk.len() * 4])?;
    }
    Ok(())
}

fn read_frontier(
    reader: &mut impl Read,
    crc: &mut Crc64,
    frontier_len: usize,
    node_count: u32,
) -> Result<Vec<u32>, CheckpointError> {
    let mut frontier = Vec::with_capacity(frontier_len);
    let mut bytes = vec![0_u8; FRONTIER_CHUNK_IDS * 4];
    while frontier.len() < frontier_len {
        let count = (frontier_len - frontier.len()).min(FRONTIER_CHUNK_IDS);
        read_crc(reader, crc, &mut bytes[..count * 4])?;
        for encoded in bytes[..count * 4].chunks_exact(4) {
            let node = u32::from_le_bytes(encoded.try_into().expect("four-byte chunk"));
            if node >= node_count {
                return Err(CheckpointError::FrontierNodeOutOfRange { node, node_count });
            }
            frontier.push(node);
        }
    }
    Ok(frontier)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn crc64_xz_check_value_is_stable() {
        let mut crc = Crc64::new();
        crc.update(b"123456789");
        assert_eq!(crc.finish(), 0x995d_c9bb_df19_39fa);
    }

    #[test]
    fn checkpoint_round_trips() {
        let path = test_path("round-trip");
        let checkpoint = sample_checkpoint();
        checkpoint.save_atomic(&path).unwrap();
        let loaded = Checkpoint::load(&path, 6, 17).unwrap();
        assert_eq!(loaded, checkpoint);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn checksum_detects_corruption() {
        let path = test_path("corrupt");
        sample_checkpoint().save_atomic(&path).unwrap();
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
            Checkpoint::load(&path, 6, 17),
            Err(CheckpointError::ChecksumMismatch { .. })
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn truncation_is_rejected_before_allocation() {
        let path = test_path("truncated");
        sample_checkpoint().save_atomic(&path).unwrap();
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(40)
            .unwrap();
        assert!(Checkpoint::load(&path, 6, 17).is_err());
        fs::remove_file(path).unwrap();
    }

    fn sample_checkpoint() -> Checkpoint {
        Checkpoint::new(
            17,
            9,
            vec![0, 1, 2, 3, 0, 1],
            vec![4, 3, 2, 1, 0, 8],
            vec![5, 1, 4],
        )
        .unwrap()
    }

    fn test_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "tic-tac-chec-{label}-{}-{nonce}.ctb",
            std::process::id()
        ))
    }
}
