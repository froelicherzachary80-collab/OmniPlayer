//! Named Binary Tag reader/writer (network variant: nameless root since 1.20.2).

use super::buf::{Reader, Writer};
use anyhow::{bail, Result};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Nbt {
    End,
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(Vec<Nbt>),
    Compound(BTreeMap<String, Nbt>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Nbt {
    pub fn tag_id(&self) -> u8 {
        match self {
            Nbt::End => 0,
            Nbt::Byte(_) => 1,
            Nbt::Short(_) => 2,
            Nbt::Int(_) => 3,
            Nbt::Long(_) => 4,
            Nbt::Float(_) => 5,
            Nbt::Double(_) => 6,
            Nbt::ByteArray(_) => 7,
            Nbt::String(_) => 8,
            Nbt::List(_) => 9,
            Nbt::Compound(_) => 10,
            Nbt::IntArray(_) => 11,
            Nbt::LongArray(_) => 12,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Nbt> {
        match self {
            Nbt::Compound(m) => m.get(key),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Nbt::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Nbt::Byte(v) => Some(v as i64),
            Nbt::Short(v) => Some(v as i64),
            Nbt::Int(v) => Some(v as i64),
            Nbt::Long(v) => Some(v),
            _ => None,
        }
    }

    /// Read a network NBT value (type byte followed by payload, no root name).
    pub fn read_network(r: &mut Reader) -> Result<Nbt> {
        let id = r.u8()?;
        if id == 0 {
            return Ok(Nbt::End);
        }
        Self::read_payload(r, id, 0)
    }

    /// Read a classic named-root NBT value (used by some legacy files).
    pub fn read_named(r: &mut Reader) -> Result<(String, Nbt)> {
        let id = r.u8()?;
        if id == 0 {
            return Ok((String::new(), Nbt::End));
        }
        let name = read_nbt_string(r)?;
        Ok((name, Self::read_payload(r, id, 0)?))
    }

    fn read_payload(r: &mut Reader, id: u8, depth: usize) -> Result<Nbt> {
        if depth > 512 {
            bail!("NBT nested too deeply");
        }
        Ok(match id {
            1 => Nbt::Byte(r.i8()?),
            2 => Nbt::Short(r.i16()?),
            3 => Nbt::Int(r.i32()?),
            4 => Nbt::Long(r.i64()?),
            5 => Nbt::Float(r.f32()?),
            6 => Nbt::Double(r.f64()?),
            7 => {
                let n = checked_len(r.i32()?, r.remaining())?;
                Nbt::ByteArray(r.bytes(n)?.iter().map(|b| *b as i8).collect())
            }
            8 => Nbt::String(read_nbt_string(r)?),
            9 => {
                let elem = r.u8()?;
                let n = r.i32()?;
                if n <= 0 {
                    return Ok(Nbt::List(vec![]));
                }
                let n = checked_len(n, r.remaining())?;
                let mut v = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    v.push(Self::read_payload(r, elem, depth + 1)?);
                }
                Nbt::List(v)
            }
            10 => {
                let mut m = BTreeMap::new();
                loop {
                    let t = r.u8()?;
                    if t == 0 {
                        break;
                    }
                    let name = read_nbt_string(r)?;
                    m.insert(name, Self::read_payload(r, t, depth + 1)?);
                }
                Nbt::Compound(m)
            }
            11 => {
                let n = checked_len(r.i32()?, r.remaining() / 4)?;
                Nbt::IntArray((0..n).map(|_| r.i32()).collect::<Result<_>>()?)
            }
            12 => {
                let n = checked_len(r.i32()?, r.remaining() / 8)?;
                Nbt::LongArray((0..n).map(|_| r.i64()).collect::<Result<_>>()?)
            }
            _ => bail!("unknown NBT tag {id}"),
        })
    }

    pub fn write_network(&self, w: &mut Writer) {
        w.u8(self.tag_id());
        if !matches!(self, Nbt::End) {
            self.write_payload(w);
        }
    }

    fn write_payload(&self, w: &mut Writer) {
        match self {
            Nbt::End => {}
            Nbt::Byte(v) => {
                w.i8(*v);
            }
            Nbt::Short(v) => {
                w.i16(*v);
            }
            Nbt::Int(v) => {
                w.i32(*v);
            }
            Nbt::Long(v) => {
                w.i64(*v);
            }
            Nbt::Float(v) => {
                w.f32(*v);
            }
            Nbt::Double(v) => {
                w.f64(*v);
            }
            Nbt::ByteArray(v) => {
                w.i32(v.len() as i32);
                for b in v {
                    w.i8(*b);
                }
            }
            Nbt::String(s) => write_nbt_string(w, s),
            Nbt::List(v) => {
                w.u8(v.first().map(|e| e.tag_id()).unwrap_or(0));
                w.i32(v.len() as i32);
                for e in v {
                    e.write_payload(w);
                }
            }
            Nbt::Compound(m) => {
                for (k, v) in m {
                    w.u8(v.tag_id());
                    write_nbt_string(w, k);
                    v.write_payload(w);
                }
                w.u8(0);
            }
            Nbt::IntArray(v) => {
                w.i32(v.len() as i32);
                for i in v {
                    w.i32(*i);
                }
            }
            Nbt::LongArray(v) => {
                w.i32(v.len() as i32);
                for i in v {
                    w.i64(*i);
                }
            }
        }
    }
}

fn checked_len(n: i32, max: usize) -> Result<usize> {
    if n < 0 || n as usize > max {
        bail!("invalid NBT length {n}");
    }
    Ok(n as usize)
}

/// NBT strings are Java "modified UTF-8" with a u16 length prefix.
fn read_nbt_string(r: &mut Reader) -> Result<String> {
    let len = r.u16()? as usize;
    let bytes = r.bytes(len)?;
    Ok(match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => decode_modified_utf8(bytes),
    })
}

fn decode_modified_utf8(bytes: &[u8]) -> String {
    // Lossy fallback that handles the 0xC0 0x80 NUL encoding and surrogate pairs.
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b & 0x80 == 0 {
            units.push(b as u16);
            i += 1;
        } else if b & 0xE0 == 0xC0 && i + 1 < bytes.len() {
            units.push((((b & 0x1F) as u16) << 6) | (bytes[i + 1] & 0x3F) as u16);
            i += 2;
        } else if b & 0xF0 == 0xE0 && i + 2 < bytes.len() {
            units.push((((b & 0x0F) as u16) << 12) | (((bytes[i + 1] & 0x3F) as u16) << 6) | (bytes[i + 2] & 0x3F) as u16);
            i += 3;
        } else {
            units.push(0xFFFD);
            i += 1;
        }
    }
    String::from_utf16_lossy(&units)
}

fn write_nbt_string(w: &mut Writer, s: &str) {
    w.u16(s.len() as u16);
    w.bytes(s.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_compound() {
        let mut m = BTreeMap::new();
        m.insert("text".to_string(), Nbt::String("hi".into()));
        m.insert("bold".to_string(), Nbt::Byte(1));
        m.insert("extra".to_string(), Nbt::List(vec![Nbt::String("a".into()), Nbt::String("b".into())]));
        m.insert("longs".to_string(), Nbt::LongArray(vec![1, -2, 3]));
        let v = Nbt::Compound(m);
        let mut w = Writer::new();
        v.write_network(&mut w);
        let back = Nbt::read_network(&mut Reader::new(&w.buf)).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn plain_string_component() {
        // A text component sent as a bare NBT string tag.
        let bytes = [8u8, 0, 5, b'h', b'e', b'l', b'l', b'o'];
        assert_eq!(Nbt::read_network(&mut Reader::new(&bytes)).unwrap(), Nbt::String("hello".into()));
    }
}
