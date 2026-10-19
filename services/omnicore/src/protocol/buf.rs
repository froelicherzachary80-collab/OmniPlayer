//! Primitive Minecraft wire types: VarInt/VarLong, strings, positions, UUIDs.
//! Everything here is hand-written against the Java Edition protocol.

use anyhow::{bail, Result};
use uuid::Uuid;

/// Cursor over an incoming packet body.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn rest(&mut self) -> &'a [u8] {
        let r = &self.data[self.pos..];
        self.pos = self.data.len();
        r
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.remaining() < n {
            bail!("unexpected end of packet: wanted {n} bytes, {} left", self.remaining());
        }
        let r = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(r)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }
    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }
    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.bytes(2)?.try_into()?))
    }
    pub fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.bytes(2)?.try_into()?))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.bytes(4)?.try_into()?))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.bytes(4)?.try_into()?))
    }
    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.bytes(8)?.try_into()?))
    }
    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_be_bytes(self.bytes(4)?.try_into()?))
    }
    pub fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_be_bytes(self.bytes(8)?.try_into()?))
    }

    pub fn varint(&mut self) -> Result<i32> {
        let mut value: u32 = 0;
        for i in 0..5 {
            let b = self.u8()?;
            value |= ((b & 0x7F) as u32) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(value as i32);
            }
        }
        bail!("VarInt too long")
    }

    pub fn varlong(&mut self) -> Result<i64> {
        let mut value: u64 = 0;
        for i in 0..10 {
            let b = self.u8()?;
            value |= ((b & 0x7F) as u64) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(value as i64);
            }
        }
        bail!("VarLong too long")
    }

    pub fn string(&mut self) -> Result<String> {
        let len = self.varint()?;
        if len < 0 || len > 32767 * 4 {
            bail!("invalid string length {len}");
        }
        Ok(String::from_utf8(self.bytes(len as usize)?.to_vec())?)
    }

    pub fn byte_array(&mut self) -> Result<Vec<u8>> {
        let len = self.varint()?;
        if len < 0 || len as usize > self.remaining() {
            bail!("invalid byte array length {len}");
        }
        Ok(self.bytes(len as usize)?.to_vec())
    }

    pub fn uuid(&mut self) -> Result<Uuid> {
        Ok(Uuid::from_bytes(self.bytes(16)?.try_into()?))
    }

    /// Packed block position (x:26, z:26, y:12).
    pub fn position_packed(&mut self) -> Result<(i32, i32, i32)> {
        Ok(unpack_position(self.i64()?))
    }

    pub fn long_array(&mut self) -> Result<Vec<i64>> {
        let len = self.varint()?;
        if len < 0 || len as usize * 8 > self.remaining() {
            bail!("invalid long array length {len}");
        }
        (0..len).map(|_| self.i64()).collect()
    }

    /// Vanilla BitSet: VarInt count of longs followed by the longs.
    pub fn bitset(&mut self) -> Result<Vec<i64>> {
        self.long_array()
    }

    /// Low-precision vector used for velocities since 1.21.9.
    pub fn lp_vec3(&mut self) -> Result<(f64, f64, f64)> {
        let flags = self.u8()? as u64;
        if flags == 0 {
            return Ok((0.0, 0.0, 0.0));
        }
        let p2 = self.u8()? as u64;
        let p3 = self.u32()? as u64;
        let value = (p3 << 16) | (p2 << 8) | flags;
        let mut scale = flags & 0b11;
        if flags & 4 == 4 {
            scale |= ((self.varint()? as u32) as u64) << 2;
        }
        let unpack = |v: u64| -> f64 { ((v & 0x7FFF) as f64).min(32766.0) * 2.0 / 32766.0 - 1.0 };
        let s = scale as f64;
        Ok((unpack(value >> 3) * s, unpack(value >> 18) * s, unpack(value >> 33) * s))
    }

    pub fn optional<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<Option<T>> {
        if self.bool()? {
            Ok(Some(f(self)?))
        } else {
            Ok(None)
        }
    }
}

pub fn unpack_position(v: i64) -> (i32, i32, i32) {
    let x = (v >> 38) as i32;
    let y = ((v << 52) >> 52) as i32;
    let z = ((v << 26) >> 38) as i32;
    (x, y, z)
}

pub fn pack_position(x: i32, y: i32, z: i32) -> i64 {
    ((x as i64 & 0x3FFFFFF) << 38) | ((z as i64 & 0x3FFFFFF) << 12) | (y as i64 & 0xFFF)
}

/// Builder for an outgoing packet body.
#[derive(Default, Clone)]
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_id(id: i32) -> Self {
        let mut w = Self::new();
        w.varint(id);
        w
    }
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }
    pub fn i8(&mut self, v: i8) -> &mut Self {
        self.u8(v as u8)
    }
    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.u8(v as u8)
    }
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn i16(&mut self, v: i16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn i64(&mut self, v: i64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn f64(&mut self, v: f64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn varint(&mut self, v: i32) -> &mut Self {
        write_varint(&mut self.buf, v);
        self
    }
    pub fn varlong(&mut self, v: i64) -> &mut Self {
        let mut v = v as u64;
        loop {
            if v & !0x7F == 0 {
                self.buf.push(v as u8);
                return self;
            }
            self.buf.push(((v & 0x7F) | 0x80) as u8);
            v >>= 7;
        }
    }
    pub fn string(&mut self, s: &str) -> &mut Self {
        self.varint(s.len() as i32);
        self.buf.extend_from_slice(s.as_bytes());
        self
    }
    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(b);
        self
    }
    pub fn byte_array(&mut self, b: &[u8]) -> &mut Self {
        self.varint(b.len() as i32);
        self.bytes(b)
    }
    pub fn uuid(&mut self, u: &Uuid) -> &mut Self {
        self.bytes(u.as_bytes())
    }
    pub fn position(&mut self, x: i32, y: i32, z: i32) -> &mut Self {
        self.i64(pack_position(x, y, z))
    }
    pub fn lp_vec3(&mut self, x: f64, y: f64, z: f64) -> &mut Self {
        let san = |v: f64| if v.is_nan() { 0.0 } else { v.clamp(-1.7179869183E10, 1.7179869183E10) };
        let (x, y, z) = (san(x), san(y), san(z));
        let max = x.abs().max(y.abs()).max(z.abs());
        if max < 3.051944088384301E-5 {
            return self.u8(0);
        }
        let i = max.ceil() as i64;
        let cont = (i & 3) != i;
        let flags = if cont { (i & 3) | 4 } else { i } as u64;
        let pack = |v: f64| -> u64 { ((v * 0.5 + 0.5) * 32766.0).round() as u64 };
        let packed = flags | (pack(x / i as f64) << 3) | (pack(y / i as f64) << 18) | (pack(z / i as f64) << 33);
        self.u8(packed as u8);
        self.u8((packed >> 8) as u8);
        self.i32((packed >> 16) as i32);
        if cont {
            self.varint((i >> 2) as i32);
        }
        self
    }
}

pub fn write_varint(buf: &mut Vec<u8>, v: i32) {
    let mut v = v as u32;
    loop {
        if v & !0x7F == 0 {
            buf.push(v as u8);
            return;
        }
        buf.push(((v & 0x7F) | 0x80) as u8);
        v >>= 7;
    }
}

pub fn varint_len(v: i32) -> usize {
    let mut b = Vec::with_capacity(5);
    write_varint(&mut b, v);
    b.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_known_vectors() {
        // Vectors from the protocol documentation.
        let cases: &[(i32, &[u8])] = &[
            (0, &[0x00]),
            (1, &[0x01]),
            (127, &[0x7f]),
            (128, &[0x80, 0x01]),
            (255, &[0xff, 0x01]),
            (25565, &[0xdd, 0xc7, 0x01]),
            (2097151, &[0xff, 0xff, 0x7f]),
            (2147483647, &[0xff, 0xff, 0xff, 0xff, 0x07]),
            (-1, &[0xff, 0xff, 0xff, 0xff, 0x0f]),
            (-2147483648, &[0x80, 0x80, 0x80, 0x80, 0x08]),
        ];
        for (v, bytes) in cases {
            let mut w = Writer::new();
            w.varint(*v);
            assert_eq!(&w.buf, bytes, "encode {v}");
            assert_eq!(Reader::new(bytes).varint().unwrap(), *v, "decode {v}");
        }
    }

    #[test]
    fn varlong_roundtrip() {
        for v in [0i64, 1, 127, 128, 9223372036854775807, -1, -9223372036854775808] {
            let mut w = Writer::new();
            w.varlong(v);
            assert_eq!(Reader::new(&w.buf).varlong().unwrap(), v);
        }
    }

    #[test]
    fn position_roundtrip() {
        for (x, y, z) in [(0, 0, 0), (18357644, 831, -20882616), (-1, -64, -1), (30000000, 319, -30000000)] {
            assert_eq!(unpack_position(pack_position(x, y, z)), (x, y, z));
        }
    }

    #[test]
    fn lp_vec3_roundtrip() {
        for v in [(0.0, 0.0, 0.0), (0.1, -0.0784, 0.0), (3.5, -2.0, 1.25), (12.0, 0.0, -40.5)] {
            let mut w = Writer::new();
            w.lp_vec3(v.0, v.1, v.2);
            let (x, y, z) = Reader::new(&w.buf).lp_vec3().unwrap();
            let tol = v.0.abs().max(v.1.abs()).max(v.2.abs()).ceil() * 1e-4 + 1e-4;
            assert!((x - v.0).abs() < tol && (y - v.1).abs() < tol && (z - v.2).abs() < tol, "{v:?} -> {x},{y},{z}");
        }
    }

    #[test]
    fn string_roundtrip() {
        let mut w = Writer::new();
        w.string("Hello, Минecraft ✓");
        assert_eq!(Reader::new(&w.buf).string().unwrap(), "Hello, Минecraft ✓");
    }
}
