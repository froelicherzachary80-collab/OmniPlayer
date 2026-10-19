//! Client-side world state reconstructed from chunk/block packets.

use crate::data::GAME;
use crate::protocol::{Nbt, Reader};
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::sync::Arc;

pub const SECTION_VOLUME: usize = 4096;

#[derive(Clone, Debug)]
pub enum Blocks {
    Single(u32),
    Full(Box<[u16; SECTION_VOLUME]>),
}

#[derive(Clone, Debug)]
pub struct Section {
    pub blocks: Blocks,
    /// 4x4x4 biome ids (index into the biome registry).
    pub biomes: [u16; 64],
    pub non_air: i16,
}

impl Section {
    pub fn get(&self, x: usize, y: usize, z: usize) -> u32 {
        match &self.blocks {
            Blocks::Single(v) => *v,
            Blocks::Full(a) => a[(y << 8) | (z << 4) | x] as u32,
        }
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, state: u32) {
        if let Blocks::Single(v) = self.blocks {
            if v == state {
                return;
            }
            self.blocks = Blocks::Full(Box::new([v as u16; SECTION_VOLUME]));
        }
        if let Blocks::Full(a) = &mut self.blocks {
            a[(y << 8) | (z << 4) | x] = state as u16;
        }
    }
}

#[derive(Clone, Debug)]
pub struct Chunk {
    pub sections: Vec<Section>,
    pub block_entities: Vec<(i32, i32, i32, i32)>,
}

#[derive(Clone, Debug)]
pub struct Dimension {
    pub name: String,
    pub min_y: i32,
    pub height: i32,
}

impl Default for Dimension {
    fn default() -> Self {
        Self { name: "minecraft:overworld".into(), min_y: -64, height: 384 }
    }
}

/// Chunks are reference counted and copy-on-write so renderers can take
/// cheap snapshots while the network thread keeps applying updates.
#[derive(Default, Clone)]
pub struct World {
    pub chunks: HashMap<(i32, i32), Arc<Chunk>>,
    pub dimension: Dimension,
    pub biome_names: Vec<String>,
    pub max_chunks: usize,
}

/// Bits needed to index `n` distinct values (Mth.ceillog2).
pub fn ceil_log2(n: usize) -> u32 {
    if n <= 1 {
        0
    } else {
        usize::BITS - (n - 1).leading_zeros()
    }
}

/// Reads one paletted container (blocks: 4096 entries, biomes: 64 entries).
fn read_paletted(r: &mut Reader, entries: usize, is_blocks: bool, direct_bits: u32, out: &mut Vec<u32>) -> Result<()> {
    out.clear();
    let bpe = r.u8()? as u32;
    if bpe == 0 {
        let v = r.varint()? as u32;
        out.resize(entries, v);
        return Ok(());
    }
    let (bits, palette): (u32, Option<Vec<u32>>) = if is_blocks {
        if bpe <= 8 {
            let n = r.varint()? as usize;
            if n == 0 || n > 256 {
                bail!("bad block palette length {n}");
            }
            let p = (0..n).map(|_| r.varint().map(|v| v as u32)).collect::<Result<Vec<_>>>()?;
            (bpe.max(4), Some(p))
        } else {
            (direct_bits, None)
        }
    } else if bpe <= 3 {
        let n = r.varint()? as usize;
        if n == 0 || n > 8 {
            bail!("bad biome palette length {n}");
        }
        let p = (0..n).map(|_| r.varint().map(|v| v as u32)).collect::<Result<Vec<_>>>()?;
        (bpe, Some(p))
    } else {
        (direct_bits, None)
    };
    let per_long = (64 / bits) as usize;
    let longs = entries.div_ceil(per_long);
    let mask = (1u64 << bits) - 1;
    out.reserve(entries);
    'outer: for _ in 0..longs {
        let l = r.i64()? as u64;
        for j in 0..per_long {
            if out.len() == entries {
                continue 'outer;
            }
            let idx = ((l >> (j as u32 * bits)) & mask) as u32;
            let v = match &palette {
                Some(p) => *p.get(idx as usize).unwrap_or(&0),
                None => idx,
            };
            out.push(v);
        }
    }
    Ok(())
}

impl World {
    pub fn new() -> Self {
        Self { max_chunks: 1024, ..Default::default() }
    }

    fn section_count(&self) -> usize {
        (self.dimension.height / 16) as usize
    }

    pub fn biome_bits(&self) -> u32 {
        // Vanilla 26.3 ships 65 biomes; use the registry the server sent when known.
        let n = if self.biome_names.is_empty() { 65 } else { self.biome_names.len() };
        ceil_log2(n)
    }

    /// Parses the body of a "Chunk Data and Update Light" packet after the coordinates.
    pub fn load_chunk(&mut self, cx: i32, cz: i32, r: &mut Reader) -> Result<()> {
        // Heightmaps: map of VarInt type -> long array.
        let hm = r.varint()?;
        for _ in 0..hm {
            r.varint()?;
            r.long_array()?;
        }
        let data = r.byte_array()?;
        let mut d = Reader::new(&data);
        let block_bits = ceil_log2(GAME.states.len());
        let biome_bits = self.biome_bits();
        let mut sections = Vec::with_capacity(self.section_count());
        let mut tmp = Vec::with_capacity(SECTION_VOLUME);
        for _ in 0..self.section_count() {
            if d.remaining() == 0 {
                break;
            }
            let non_air = d.i16()?;
            let _fluids = d.i16()?;
            read_paletted(&mut d, SECTION_VOLUME, true, block_bits, &mut tmp)?;
            let blocks = if tmp.iter().all(|v| *v == tmp[0]) {
                Blocks::Single(tmp[0])
            } else {
                let mut a = Box::new([0u16; SECTION_VOLUME]);
                for (i, v) in tmp.iter().enumerate() {
                    a[i] = *v as u16;
                }
                Blocks::Full(a)
            };
            read_paletted(&mut d, 64, false, biome_bits, &mut tmp)?;
            let mut biomes = [0u16; 64];
            for (i, v) in tmp.iter().enumerate().take(64) {
                biomes[i] = *v as u16;
            }
            sections.push(Section { blocks, biomes, non_air });
        }
        while sections.len() < self.section_count() {
            sections.push(Section { blocks: Blocks::Single(0), biomes: [0; 64], non_air: 0 });
        }
        let mut block_entities = Vec::new();
        let n = r.varint()?;
        for _ in 0..n {
            let xz = r.u8()?;
            let y = r.i16()? as i32;
            let ty = r.varint()?;
            let _nbt = Nbt::read_network(r)?;
            block_entities.push((cx * 16 + (xz >> 4) as i32, y, cz * 16 + (xz & 15) as i32, ty));
        }
        // Light data follows; OmniView computes its own shading so it is not needed.
        self.chunks.insert((cx, cz), Arc::new(Chunk { sections, block_entities }));
        Ok(())
    }

    pub fn unload_chunk(&mut self, cx: i32, cz: i32) {
        self.chunks.remove(&(cx, cz));
    }

    /// Drops chunks farther than `radius` from the given chunk (cache limit).
    pub fn prune(&mut self, cx: i32, cz: i32, radius: i32) {
        if self.chunks.len() <= self.max_chunks {
            return;
        }
        self.chunks.retain(|(x, z), _| (x - cx).abs() <= radius && (z - cz).abs() <= radius);
    }

    /// Cheap copy of the chunks within `radius` chunks of (cx, cz).
    pub fn snapshot(&self, cx: i32, cz: i32, radius: i32) -> World {
        World {
            chunks: self
                .chunks
                .iter()
                .filter(|((x, z), _)| (x - cx).abs() <= radius && (z - cz).abs() <= radius)
                .map(|(k, v)| (*k, v.clone()))
                .collect(),
            dimension: self.dimension.clone(),
            biome_names: self.biome_names.clone(),
            max_chunks: self.max_chunks,
        }
    }

    pub fn is_loaded(&self, x: i32, z: i32) -> bool {
        self.chunks.contains_key(&(x >> 4, z >> 4))
    }

    /// Block state at a world position; `None` when the chunk is not loaded.
    pub fn get(&self, x: i32, y: i32, z: i32) -> Option<u32> {
        let ch = self.chunks.get(&(x >> 4, z >> 4))?;
        let rel = y - self.dimension.min_y;
        if rel < 0 || rel >= self.dimension.height {
            return Some(0);
        }
        let s = ch.sections.get((rel >> 4) as usize)?;
        Some(s.get((x & 15) as usize, (rel & 15) as usize, (z & 15) as usize))
    }

    pub fn get_or_air(&self, x: i32, y: i32, z: i32) -> u32 {
        self.get(x, y, z).unwrap_or(0)
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, state: u32) {
        let min_y = self.dimension.min_y;
        let h = self.dimension.height;
        if let Some(ch) = self.chunks.get_mut(&(x >> 4, z >> 4)) {
            let ch = Arc::make_mut(ch);
            let rel = y - min_y;
            if rel < 0 || rel >= h {
                return;
            }
            if let Some(s) = ch.sections.get_mut((rel >> 4) as usize) {
                s.set((x & 15) as usize, (rel & 15) as usize, (z & 15) as usize, state);
            }
        }
    }

    pub fn biome_at(&self, x: i32, y: i32, z: i32) -> Option<&str> {
        let ch = self.chunks.get(&(x >> 4, z >> 4))?;
        let rel = (y - self.dimension.min_y).clamp(0, self.dimension.height - 1);
        let s = ch.sections.get((rel >> 4) as usize)?;
        let idx = (((rel & 15) >> 2) << 4) | ((((z & 15) >> 2) as i32) << 2) | ((x & 15) >> 2);
        let id = s.biomes[idx as usize] as usize;
        self.biome_names.get(id).map(|s| s.as_str())
    }

    pub fn block_name(&self, x: i32, y: i32, z: i32) -> &'static str {
        GAME.block_name(self.get_or_air(x, y, z))
    }

    /// Finds the nearest blocks matching a predicate within a cube radius.
    pub fn find_blocks(
        &self,
        origin: (i32, i32, i32),
        radius: i32,
        limit: usize,
        pred: impl Fn(u32) -> bool,
    ) -> Vec<(i32, i32, i32)> {
        let mut found = Vec::new();
        let (ox, oy, oz) = origin;
        let ymin = (oy - radius).max(self.dimension.min_y);
        let ymax = (oy + radius).min(self.dimension.min_y + self.dimension.height - 1);
        for x in ox - radius..=ox + radius {
            for z in oz - radius..=oz + radius {
                let Some(ch) = self.chunks.get(&(x >> 4, z >> 4)) else { continue };
                for y in ymin..=ymax {
                    let rel = y - self.dimension.min_y;
                    let s = &ch.sections[(rel >> 4) as usize];
                    if let Blocks::Single(v) = s.blocks {
                        if !pred(v) {
                            continue;
                        }
                    }
                    let st = s.get((x & 15) as usize, (rel & 15) as usize, (z & 15) as usize);
                    if pred(st) {
                        found.push((x, y, z));
                    }
                }
            }
        }
        found.sort_by_key(|(x, y, z)| (x - ox).pow(2) + (y - oy).pow(2) + (z - oz).pow(2));
        found.truncate(limit);
        found
    }

    /// Highest non-air block at (x,z), if the chunk is loaded.
    pub fn surface_y(&self, x: i32, z: i32) -> Option<i32> {
        let top = self.dimension.min_y + self.dimension.height - 1;
        for y in (self.dimension.min_y..=top).rev() {
            let s = self.get(x, y, z)?;
            if !GAME.is_air(s) {
                return Some(y);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Writer;

    fn encode_section(w: &mut Writer, states: &[u32]) {
        w.i16(4096).i16(0);
        // indirect palette, 4 bits
        let mut palette: Vec<u32> = vec![];
        for s in states {
            if !palette.contains(s) {
                palette.push(*s);
            }
        }
        w.u8(4).varint(palette.len() as i32);
        for p in &palette {
            w.varint(*p as i32);
        }
        for chunk in states.chunks(16) {
            let mut l: u64 = 0;
            for (j, s) in chunk.iter().enumerate() {
                let idx = palette.iter().position(|p| p == s).unwrap() as u64;
                l |= idx << (4 * j);
            }
            w.i64(l as i64);
        }
        // biomes single value
        w.u8(0).varint(0);
    }

    #[test]
    fn decodes_chunk_sections() {
        let mut world = World::new();
        world.biome_names = (0..65).map(|i| format!("b{i}")).collect();
        let mut data = Writer::new();
        let mut states = vec![1u32; 4096];
        states[0] = 9; // grass at (0,0,0)
        states[(3 << 8) | (2 << 4) | 1] = 0; // air at x=1,y=3,z=2
        encode_section(&mut data, &states);
        for _ in 1..24 {
            data.i16(0).i16(0).u8(0).varint(0).u8(0).varint(0);
        }
        let mut pkt = Writer::new();
        pkt.varint(0); // no heightmaps
        pkt.byte_array(&data.buf);
        pkt.varint(0); // no block entities
        world.load_chunk(2, -1, &mut Reader::new(&pkt.buf)).unwrap();
        assert_eq!(world.get(32, -64, -16), Some(9));
        assert_eq!(world.get(33, -61, -14), Some(0));
        assert_eq!(world.get(34, -64, -16), Some(1));
        assert_eq!(world.get(32, 0, -16), Some(0));
        assert_eq!(world.get(0, 0, 0), None);
        world.set(34, -64, -16, 0);
        assert_eq!(world.get(34, -64, -16), Some(0));
        assert_eq!(world.biome_at(32, -64, -16), Some("b0"));
    }

    #[test]
    fn log2() {
        assert_eq!(ceil_log2(35723), 16);
        assert_eq!(ceil_log2(65), 7);
        assert_eq!(ceil_log2(64), 6);
    }
}
