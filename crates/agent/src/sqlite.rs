//! Minimal read-only SQLite reader (pure Rust), enough to read Options+ settings databases:
//! table B-trees, overflow pages and the write-ahead log. No indexes, no SQL.
//!
//! File format reference: <https://www.sqlite.org/fileformat2.html>.

use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("not an SQLite database")]
    NotSqlite,
    #[error("corrupt database: {0}")]
    Corrupt(&'static str),
    #[error("no table named {0}")]
    NoTable(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

/// One table row: its rowid and column values (an `INTEGER PRIMARY KEY` column reads as NULL; use
/// `rowid`).
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub rowid: i64,
    pub values: Vec<Value>,
}

pub struct Database {
    main: Vec<u8>,
    /// Latest committed WAL copy of each page.
    wal: HashMap<u32, Vec<u8>>,
    page_size: usize,
    usable: usize,
}

impl Database {
    /// Opens `path` and, if present, its `-wal` file (both are read into memory once).
    pub fn open(path: &Path) -> Result<Self> {
        let main = std::fs::read(path)?;
        let mut wal_path = path.as_os_str().to_owned();
        wal_path.push("-wal");
        let wal = std::fs::read(&wal_path).ok();
        Self::from_bytes(main, wal.as_deref())
    }

    pub fn from_bytes(main: Vec<u8>, wal: Option<&[u8]>) -> Result<Self> {
        if main.len() < 100 || &main[..16] != b"SQLite format 3\0" {
            return Err(Error::NotSqlite);
        }
        let page_size = match u16::from_be_bytes([main[16], main[17]]) {
            1 => 65536,
            n => n as usize,
        };
        let usable = page_size - main[20] as usize;
        let mut db = Self { main, wal: HashMap::new(), page_size, usable };
        if let Some(w) = wal {
            db.wal = read_wal(w, page_size);
        }
        Ok(db)
    }

    fn page(&self, n: u32) -> Result<&[u8]> {
        if let Some(p) = self.wal.get(&n) {
            return Ok(p);
        }
        let start = (n as usize).checked_sub(1).ok_or(Error::Corrupt("page 0"))? * self.page_size;
        self.main.get(start..start + self.page_size).ok_or(Error::Corrupt("page beyond end of file"))
    }

    /// All rows of `table`, in rowid order.
    pub fn table(&self, table: &str) -> Result<Vec<Row>> {
        let root = self
            .rows(1)?
            .into_iter()
            .find(|r| {
                matches!((&r.values.first(), &r.values.get(1)), (Some(Value::Text(t)), Some(Value::Text(n)))
                    if t == "table" && n.eq_ignore_ascii_case(table))
            })
            .and_then(|r| match r.values.get(3) {
                Some(Value::Integer(p)) => Some(*p as u32),
                _ => None,
            })
            .ok_or_else(|| Error::NoTable(table.to_string()))?;
        self.rows(root)
    }

    fn rows(&self, root: u32) -> Result<Vec<Row>> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        let mut visited = 0usize;
        while let Some(n) = stack.pop() {
            visited += 1;
            if visited > 1_000_000 {
                return Err(Error::Corrupt("b-tree loop"));
            }
            let page = self.page(n)?;
            let h = if n == 1 { 100 } else { 0 };
            let kind = page[h];
            let cells = u16::from_be_bytes([page[h + 3], page[h + 4]]) as usize;
            match kind {
                0x0d => {
                    for i in 0..cells {
                        let off = cell_offset(page, h + 8, i)?;
                        out.push(self.leaf_cell(page, off)?);
                    }
                }
                0x05 => {
                    // Children are pushed right-most first so rows come out in rowid order.
                    stack.push(u32::from_be_bytes(page[h + 8..h + 12].try_into().unwrap()));
                    for i in (0..cells).rev() {
                        let off = cell_offset(page, h + 12, i)?;
                        let child = page.get(off..off + 4).ok_or(Error::Corrupt("interior cell"))?;
                        stack.push(u32::from_be_bytes(child.try_into().unwrap()));
                    }
                }
                _ => return Err(Error::Corrupt("unexpected page type in table b-tree")),
            }
        }
        Ok(out)
    }

    fn leaf_cell(&self, page: &[u8], off: usize) -> Result<Row> {
        let (size, a) = varint(page, off)?;
        let (rowid, b) = varint(page, off + a)?;
        let start = off + a + b;
        let size = size as usize;
        let u = self.usable;
        let x = u - 35;
        let local = if size <= x {
            size
        } else {
            let m = (u - 12) * 32 / 255 - 23;
            let k = m + (size - m) % (u - 4);
            if k <= x { k } else { m }
        };
        let mut payload = page.get(start..start + local).ok_or(Error::Corrupt("cell payload"))?.to_vec();
        if local < size {
            let mut next = u32::from_be_bytes(
                page.get(start + local..start + local + 4)
                    .ok_or(Error::Corrupt("overflow pointer"))?
                    .try_into()
                    .unwrap(),
            );
            while payload.len() < size {
                if next == 0 {
                    return Err(Error::Corrupt("overflow chain ends early"));
                }
                let p = self.page(next)?;
                let take = (size - payload.len()).min(u - 4);
                payload.extend_from_slice(&p[4..4 + take]);
                next = u32::from_be_bytes(p[..4].try_into().unwrap());
            }
        }
        Ok(Row { rowid: rowid as i64, values: record(&payload)? })
    }
}

fn cell_offset(page: &[u8], array: usize, i: usize) -> Result<usize> {
    let p = page.get(array + 2 * i..array + 2 * i + 2).ok_or(Error::Corrupt("cell pointer"))?;
    Ok(u16::from_be_bytes([p[0], p[1]]) as usize)
}

/// SQLite varint: big-endian, 7 bits per byte, the 9th byte contributes 8 bits.
fn varint(b: &[u8], at: usize) -> Result<(u64, usize)> {
    let mut v = 0u64;
    for i in 0..9 {
        let byte = *b.get(at + i).ok_or(Error::Corrupt("varint"))?;
        if i == 8 {
            return Ok((v << 8 | byte as u64, 9));
        }
        v = v << 7 | (byte & 0x7f) as u64;
        if byte & 0x80 == 0 {
            return Ok((v, i + 1));
        }
    }
    unreachable!()
}

fn record(p: &[u8]) -> Result<Vec<Value>> {
    let (header_len, mut pos) = varint(p, 0)?;
    let header_len = header_len as usize;
    let mut types = Vec::new();
    while pos < header_len {
        let (t, n) = varint(p, pos)?;
        types.push(t);
        pos += n;
    }
    let mut data = header_len;
    let mut take = |n: usize| -> Result<&[u8]> {
        let s = p.get(data..data + n).ok_or(Error::Corrupt("record body"))?;
        data += n;
        Ok(s)
    };
    let int = |s: &[u8]| s.iter().fold(if s[0] & 0x80 != 0 { -1i64 } else { 0 }, |a, &b| a << 8 | b as i64);
    types
        .into_iter()
        .map(|t| {
            Ok(match t {
                0 => Value::Null,
                1..=4 => Value::Integer(int(take(t as usize)?)),
                5 => Value::Integer(int(take(6)?)),
                6 => Value::Integer(int(take(8)?)),
                7 => Value::Real(f64::from_be_bytes(take(8)?.try_into().unwrap())),
                8 => Value::Integer(0),
                9 => Value::Integer(1),
                t if t >= 12 && t % 2 == 0 => Value::Blob(take(((t - 12) / 2) as usize)?.to_vec()),
                t if t >= 13 => Value::Text(String::from_utf8_lossy(take(((t - 13) / 2) as usize)?).into_owned()),
                _ => return Err(Error::Corrupt("reserved serial type")),
            })
        })
        .collect()
}

/// Pages from the write-ahead log, up to its last commit frame. Frames from an older WAL generation
/// (different salts) are ignored. Frame checksums are not verified.
fn read_wal(w: &[u8], page_size: usize) -> HashMap<u32, Vec<u8>> {
    let mut committed = HashMap::new();
    if w.len() < 32 || !matches!(u32::from_be_bytes(w[..4].try_into().unwrap()), 0x377f_0682 | 0x377f_0683) {
        return committed;
    }
    if u32::from_be_bytes(w[8..12].try_into().unwrap()) as usize != page_size {
        return committed;
    }
    let salts = &w[16..24];
    let mut pending: HashMap<u32, Vec<u8>> = HashMap::new();
    let mut at = 32;
    while at + 24 + page_size <= w.len() {
        let f = &w[at..at + 24];
        if &f[8..16] != salts {
            break;
        }
        let page = u32::from_be_bytes(f[..4].try_into().unwrap());
        pending.insert(page, w[at + 24..at + 24 + page_size].to_vec());
        if u32::from_be_bytes(f[4..8].try_into().unwrap()) != 0 {
            committed.extend(pending.drain());
        }
        at += 24 + page_size;
    }
    committed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints() {
        assert_eq!(varint(&[0x7f], 0).unwrap(), (127, 1));
        assert_eq!(varint(&[0x81, 0x00], 0).unwrap(), (128, 2));
        assert_eq!(varint(&[0xff; 9], 0).unwrap(), (u64::MAX, 9));
    }

    #[test]
    fn records() {
        // header: len 4, types: int8 (1), text len 2 (17), blob len 1 (14)
        let p = [4, 1, 17, 14, 0xff, b'h', b'i', 0xab];
        assert_eq!(record(&p).unwrap(), vec![Value::Integer(-1), Value::Text("hi".into()), Value::Blob(vec![0xab])]);
    }

    /// Reads the local Options+ settings database when present (developer machines only).
    #[test]
    fn reads_options_plus_settings_if_installed() {
        let Some(dir) = std::env::var_os("LOCALAPPDATA") else { return };
        let path = std::path::PathBuf::from(dir).join("LogiOptionsPlus").join("settings.db");
        if !path.exists() {
            return;
        }
        let db = Database::open(&path).unwrap();
        let rows = db.table("data").unwrap();
        let last = rows.last().expect("settings rows");
        assert!(last.values.iter().any(|v| matches!(v, Value::Blob(b) if b.first() == Some(&b'{'))
            || matches!(v, Value::Text(t) if t.starts_with('{'))));
    }
}
