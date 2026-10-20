//! Render-speed benchmark on a synthetic hilly world.
//! cargo run --release -p omniview --example bench -- 640 360 4
use omnicore::data::GAME;
use omnicore::world::{Blocks, Chunk, Section, World};
use omniview::*;
use std::time::Instant;

fn main() {
    let a: Vec<usize> = std::env::args().skip(1).filter_map(|s| s.parse().ok()).collect();
    let (w, h, threads) = (a.first().copied().unwrap_or(640), a.get(1).copied().unwrap_or(360), a.get(2).copied().unwrap_or(4));
    let grass = GAME.block_by_name("grass_block").unwrap().default_state as u16;
    let log = GAME.block_by_name("oak_log").unwrap().default_state as u16;
    let leaves = GAME.block_by_name("oak_leaves").unwrap().default_state as u16;
    let mut world = World::new();
    for cx in -5..5 {
        for cz in -5..5 {
            let mut sections: Vec<Section> = (0..24).map(|_| Section { blocks: Blocks::Single(0), biomes: [0; 64], non_air: 0 }).collect();
            for s in 0..9 {
                sections[s].blocks = Blocks::Full(Box::new([0u16; 4096]));
            }
            world.chunks.insert((cx, cz), std::sync::Arc::new(Chunk { sections, block_entities: vec![] }));
            for lx in 0..16 {
                for lz in 0..16 {
                    let x = cx * 16 + lx;
                    let z = cz * 16 + lz;
                    let top = 64 + ((x as f64 * 0.15).sin() * 3.0 + (z as f64 * 0.1).cos() * 3.0) as i32;
                    for y in -64..top {
                        world.set(x, y, z, if y == top - 1 { grass as u32 } else { 1 });
                    }
                    if (x * 7 + z * 13).rem_euclid(97) == 0 {
                        for k in 0..5 {
                            world.set(x, top + k, z, log as u32);
                        }
                        for dx in -2..=2 {
                            for dz in -2..=2 {
                                world.set(x + dx, top + 4, z + dz, leaves as u32);
                            }
                        }
                    }
                }
            }
        }
    }
    let r = Renderer::new(Settings { width: w, height: h, threads, ..Default::default() });
    let mut buf = vec![0u8; r.frame_bytes()];
    let n = 60;
    let t = Instant::now();
    for i in 0..n {
        let scene = Scene { world: world.snapshot(0, 0, 4), eye: [0.5, 70.0, 0.5 + i as f64 * 0.1], yaw: i as f32 * 2.0, pitch: 15.0, entities: vec![], day_time: 6000, raining: false, digging: None, hud: HudState::default(), frame: i };
        r.render(&scene, &mut buf);
    }
    let el = t.elapsed().as_secs_f64();
    println!("{w}x{h} threads={threads}: {:.1} ms/frame ({:.1} fps)", el * 1000.0 / n as f64, n as f64 / el);
}
