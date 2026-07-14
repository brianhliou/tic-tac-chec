//! Atomic, checksummed result-plus-remoteness tablebase artifacts.

use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: [u8; 8] = *b"TTCTB001";
const VERSION: u32 = 1;
const ENCODING_DISTANCE_PARITY: u32 = 1;
const HEADER_BYTES: u64 = 40;
const TRAILER_BYTES: u64 = 8;
const BUFFER_BYTES: usize = 8 * 1024 * 1024;
const CRC_TABLE: [u64; 256] = crc_table();
const UNRESOLVED_CODE: u8 = 254;

pub struct TablebaseArtifact {
    rules_tag: u32,
    post_codes: Vec<u8>,
    opening_codes: Vec<u8>,
    checksum: u64,
}

impl TablebaseArtifact {
    pub fn load(
        path: impl AsRef<Path>,
        expected_rules_tag: u32,
        expected_post_nodes: u64,
        expected_opening_nodes: u64,
    ) -> Result<Self, TablebaseError> {
        let file = File::open(path)?;
        let file_len = file.metadata()?.len();
        let mut reader = BufReader::with_capacity(BUFFER_BYTES, file);
        let mut crc = Crc64::new();

        let magic = read_array_crc::<8>(&mut reader, &mut crc)?;
        if magic != MAGIC {
            return Err(TablebaseError::BadMagic(magic));
        }
        let version = u32::from_le_bytes(read_array_crc(&mut reader, &mut crc)?);
        if version != VERSION {
            return Err(TablebaseError::UnsupportedVersion(version));
        }
        let rules_tag = u32::from_le_bytes(read_array_crc(&mut reader, &mut crc)?);
        if rules_tag != expected_rules_tag {
            return Err(TablebaseError::RulesMismatch {
                expected: expected_rules_tag,
                actual: rules_tag,
            });
        }
        let encoding = u32::from_le_bytes(read_array_crc(&mut reader, &mut crc)?);
        if encoding != ENCODING_DISTANCE_PARITY {
            return Err(TablebaseError::UnsupportedEncoding(encoding));
        }
        let reserved = u32::from_le_bytes(read_array_crc(&mut reader, &mut crc)?);
        if reserved != 0 {
            return Err(TablebaseError::ReservedHeaderNonzero(reserved));
        }
        let post_len = u64::from_le_bytes(read_array_crc(&mut reader, &mut crc)?);
        let opening_len = u64::from_le_bytes(read_array_crc(&mut reader, &mut crc)?);
        if post_len != expected_post_nodes || opening_len != expected_opening_nodes {
            return Err(TablebaseError::DimensionMismatch {
                expected_post: expected_post_nodes,
                actual_post: post_len,
                expected_opening: expected_opening_nodes,
                actual_opening: opening_len,
            });
        }
        let expected_len = HEADER_BYTES
            .checked_add(post_len)
            .and_then(|length| length.checked_add(opening_len))
            .and_then(|length| length.checked_add(TRAILER_BYTES))
            .ok_or(TablebaseError::LengthOverflow)?;
        if file_len != expected_len {
            return Err(TablebaseError::FileLengthMismatch {
                expected: expected_len,
                actual: file_len,
            });
        }

        let mut post_codes =
            vec![0; usize::try_from(post_len).map_err(|_| TablebaseError::LengthOverflow)?];
        let mut opening_codes =
            vec![0; usize::try_from(opening_len).map_err(|_| TablebaseError::LengthOverflow)?];
        read_crc(&mut reader, &mut crc, &mut post_codes)?;
        read_crc(&mut reader, &mut crc, &mut opening_codes)?;
        validate_codes(&post_codes, TableSection::PostOpening)?;
        validate_codes(&opening_codes, TableSection::Opening)?;
        let expected_crc = u64::from_le_bytes(read_array::<8>(&mut reader)?);
        let actual_crc = crc.finish();
        if actual_crc != expected_crc {
            return Err(TablebaseError::ChecksumMismatch {
                expected: expected_crc,
                actual: actual_crc,
            });
        }
        Ok(Self {
            rules_tag,
            post_codes,
            opening_codes,
            checksum: actual_crc,
        })
    }

    pub const fn rules_tag(&self) -> u32 {
        self.rules_tag
    }

    pub fn post_codes(&self) -> &[u8] {
        &self.post_codes
    }

    pub fn opening_codes(&self) -> &[u8] {
        &self.opening_codes
    }

    pub const fn checksum(&self) -> u64 {
        self.checksum
    }
}

pub fn save_atomic(
    path: impl AsRef<Path>,
    rules_tag: u32,
    post_codes: &[u8],
    opening_codes: &[u8],
) -> Result<u64, TablebaseError> {
    validate_codes(post_codes, TableSection::PostOpening)?;
    validate_codes(opening_codes, TableSection::Opening)?;
    let post_len = post_codes.len() as u64;
    let opening_len = opening_codes.len() as u64;
    post_len
        .checked_add(opening_len)
        .and_then(|length| length.checked_add(HEADER_BYTES + TRAILER_BYTES))
        .ok_or(TablebaseError::LengthOverflow)?;

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
        &ENCODING_DISTANCE_PARITY.to_le_bytes(),
    )?;
    write_crc(&mut writer, &mut crc, &0_u32.to_le_bytes())?;
    write_crc(&mut writer, &mut crc, &post_len.to_le_bytes())?;
    write_crc(&mut writer, &mut crc, &opening_len.to_le_bytes())?;
    write_crc(&mut writer, &mut crc, post_codes)?;
    write_crc(&mut writer, &mut crc, opening_codes)?;
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

fn validate_codes(codes: &[u8], section: TableSection) -> Result<(), TablebaseError> {
    if let Some((node, _)) = codes
        .iter()
        .enumerate()
        .find(|(_, code)| **code == UNRESOLVED_CODE)
    {
        return Err(TablebaseError::UnresolvedCode {
            section,
            node: node as u64,
        });
    }
    Ok(())
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableSection {
    PostOpening,
    Opening,
}

#[derive(Debug)]
pub enum TablebaseError {
    Io(io::Error),
    BadMagic([u8; 8]),
    UnsupportedVersion(u32),
    RulesMismatch {
        expected: u32,
        actual: u32,
    },
    UnsupportedEncoding(u32),
    ReservedHeaderNonzero(u32),
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
    ChecksumMismatch {
        expected: u64,
        actual: u64,
    },
}

impl fmt::Display for TablebaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "tablebase I/O error: {error}"),
            other => write!(formatter, "invalid tablebase: {other:?}"),
        }
    }
}

impl std::error::Error for TablebaseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for TablebaseError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remoteness::DRAW_CODE;
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn artifact_round_trips() {
        let path = test_path("round-trip");
        let checksum = save_atomic(&path, 17, &[0, 1, 2, DRAW_CODE], &[3, 4, 5]).unwrap();
        let artifact = TablebaseArtifact::load(&path, 17, 4, 3).unwrap();
        assert_eq!(artifact.rules_tag(), 17);
        assert_eq!(artifact.post_codes(), [0, 1, 2, DRAW_CODE]);
        assert_eq!(artifact.opening_codes(), [3, 4, 5]);
        assert_eq!(artifact.checksum(), checksum);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn checksum_detects_corruption() {
        let path = test_path("corrupt");
        save_atomic(&path, 17, &[0, 1, DRAW_CODE], &[2, 3]).unwrap();
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
            TablebaseArtifact::load(&path, 17, 3, 2),
            Err(TablebaseError::ChecksumMismatch { .. })
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn unresolved_code_is_rejected() {
        let path = test_path("unresolved");
        assert!(matches!(
            save_atomic(&path, 17, &[UNRESOLVED_CODE], &[]),
            Err(TablebaseError::UnresolvedCode { .. })
        ));
    }

    fn test_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "tic-tac-chec-tablebase-{label}-{}-{nonce}.ttb",
            std::process::id()
        ))
    }
}
