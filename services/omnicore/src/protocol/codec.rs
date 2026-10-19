//! Packet framing: length prefix, zlib compression and AES/CFB8 encryption.

use super::buf::{write_varint, Reader};
use aes::cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};
use aes::Aes128;
use anyhow::{bail, Context, Result};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::{Read, Write};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const MAX_PACKET: usize = 8 * 1024 * 1024;

/// AES-128 in CFB8 mode as used by Minecraft (IV = shared secret).
pub struct Cfb8 {
    cipher: Aes128,
    iv: [u8; 16],
}

impl Cfb8 {
    pub fn new(key: &[u8; 16]) -> Self {
        Self { cipher: Aes128::new(GenericArray::from_slice(key)), iv: *key }
    }

    fn keystream_byte(&self) -> u8 {
        let mut block = GenericArray::clone_from_slice(&self.iv);
        self.cipher.encrypt_block(&mut block);
        block[0]
    }

    pub fn encrypt(&mut self, data: &mut [u8]) {
        for b in data.iter_mut() {
            let c = *b ^ self.keystream_byte();
            self.iv.copy_within(1.., 0);
            self.iv[15] = c;
            *b = c;
        }
    }

    pub fn decrypt(&mut self, data: &mut [u8]) {
        for b in data.iter_mut() {
            let c = *b;
            *b = c ^ self.keystream_byte();
            self.iv.copy_within(1.., 0);
            self.iv[15] = c;
        }
    }
}

/// Reading half of a connection.
pub struct PacketReader<R> {
    inner: R,
    pub compression: Option<usize>,
    cipher: Option<Cfb8>,
    buf: Vec<u8>,
}

impl<R: AsyncRead + Unpin> PacketReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner, compression: None, cipher: None, buf: Vec::with_capacity(64 * 1024) }
    }

    pub fn enable_encryption(&mut self, key: &[u8; 16]) {
        self.cipher = Some(Cfb8::new(key));
    }

    async fn fill(&mut self, need: usize) -> Result<()> {
        while self.buf.len() < need {
            let mut tmp = [0u8; 16 * 1024];
            let n = self.inner.read(&mut tmp).await?;
            if n == 0 {
                bail!("connection closed by server");
            }
            let chunk = &mut tmp[..n];
            if let Some(c) = self.cipher.as_mut() {
                c.decrypt(chunk);
            }
            self.buf.extend_from_slice(chunk);
        }
        Ok(())
    }

    /// Returns the decoded packet (id + body) as raw bytes beginning with the packet id VarInt.
    pub async fn read_packet(&mut self) -> Result<Vec<u8>> {
        // Decode frame length VarInt.
        let mut len: u32 = 0;
        let mut header = 0usize;
        loop {
            self.fill(header + 1).await?;
            let b = self.buf[header];
            len |= ((b & 0x7F) as u32) << (7 * header);
            header += 1;
            if b & 0x80 == 0 {
                break;
            }
            if header >= 3 {
                bail!("frame length VarInt too long");
            }
        }
        let len = len as usize;
        if len > MAX_PACKET {
            bail!("packet too large: {len}");
        }
        self.fill(header + len).await?;
        let frame: Vec<u8> = self.buf[header..header + len].to_vec();
        self.buf.drain(..header + len);
        match self.compression {
            None => Ok(frame),
            Some(_) => {
                let mut r = Reader::new(&frame);
                let data_len = r.varint()? as usize;
                let rest = r.rest();
                if data_len == 0 {
                    Ok(rest.to_vec())
                } else {
                    if data_len > MAX_PACKET {
                        bail!("decompressed packet too large: {data_len}");
                    }
                    let mut out = Vec::with_capacity(data_len);
                    ZlibDecoder::new(rest).read_to_end(&mut out).context("zlib inflate")?;
                    if out.len() != data_len {
                        bail!("decompressed size mismatch {} != {}", out.len(), data_len);
                    }
                    Ok(out)
                }
            }
        }
    }
}

/// Encodes a packet (id + body) into a wire frame given the compression threshold.
pub fn encode_frame(packet: &[u8], compression: Option<usize>) -> Result<Vec<u8>> {
    let mut body = Vec::with_capacity(packet.len() + 8);
    match compression {
        None => body.extend_from_slice(packet),
        Some(threshold) => {
            if packet.len() >= threshold {
                write_varint(&mut body, packet.len() as i32);
                let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
                enc.write_all(packet)?;
                body.extend_from_slice(&enc.finish()?);
            } else {
                write_varint(&mut body, 0);
                body.extend_from_slice(packet);
            }
        }
    }
    let mut frame = Vec::with_capacity(body.len() + 5);
    write_varint(&mut frame, body.len() as i32);
    frame.extend_from_slice(&body);
    Ok(frame)
}

/// Writing half of a connection.
pub struct PacketWriter<W> {
    inner: W,
    pub compression: Option<usize>,
    cipher: Option<Cfb8>,
}

impl<W: AsyncWrite + Unpin> PacketWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner, compression: None, cipher: None }
    }

    pub fn enable_encryption(&mut self, key: &[u8; 16]) {
        self.cipher = Some(Cfb8::new(key));
    }

    pub async fn write_packet(&mut self, packet: &[u8]) -> Result<()> {
        let mut frame = encode_frame(packet, self.compression)?;
        if let Some(c) = self.cipher.as_mut() {
            c.encrypt(&mut frame);
        }
        self.inner.write_all(&frame).await?;
        self.inner.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfb8_roundtrip() {
        let key = *b"0123456789abcdef";
        let mut data = b"the quick brown fox jumps over the lazy dog".to_vec();
        let orig = data.clone();
        Cfb8::new(&key).encrypt(&mut data);
        assert_ne!(data, orig);
        Cfb8::new(&key).decrypt(&mut data);
        assert_eq!(data, orig);
    }

    #[test]
    fn cfb8_streaming_matches_oneshot() {
        let key = [7u8; 16];
        let msg: Vec<u8> = (0..200u8).collect();
        let mut a = msg.clone();
        Cfb8::new(&key).encrypt(&mut a);
        let mut b = msg.clone();
        let mut c = Cfb8::new(&key);
        c.encrypt(&mut b[..50]);
        c.encrypt(&mut b[50..]);
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn frame_roundtrip_compressed() {
        let big: Vec<u8> = (0..5000).map(|i| (i % 7) as u8).collect();
        for comp in [None, Some(256)] {
            for pkt in [vec![0x01, 0x02, 0x03], big.clone()] {
                let mut frame = encode_frame(&pkt, comp).unwrap();
                let key = [3u8; 16];
                Cfb8::new(&key).encrypt(&mut frame);
                let mut r = PacketReader::new(&frame[..]);
                r.compression = comp;
                r.enable_encryption(&key);
                assert_eq!(r.read_packet().await.unwrap(), pkt);
            }
        }
    }
}
