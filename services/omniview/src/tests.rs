use super::*;
use omnicore::data::GAME;
use omnicore::world::{Blocks, Chunk, Section, World};

fn flat() -> World {
    let mut w = World::new();
    let grass = GAME.block_by_name("grass_block").unwrap().default_state as u16;
    for cx in -3..3 {
        for cz in -3..3 {
            let mut sections = vec![];
            for i in 0..24 {
                let blocks = if i == 0 { Blocks::Full(Box::new([0u16; 4096])) } else { Blocks::Single(0) };
                sections.push(Section { blocks, biomes: [0; 64], non_air: 0 });
            }
            if let Blocks::Full(a) = &mut sections[0].blocks {
                for y in 0..4 {
                    for i in 0..256 {
                        a[(y << 8) | i] = if y == 3 { grass } else { 1 };
                    }
                }
            }
            w.chunks.insert((cx, cz), std::sync::Arc::new(Chunk { sections, block_entities: vec![] }));
        }
    }
    w
}

fn scene(world: World, eye: [f64; 3], yaw: f32, pitch: f32) -> Scene {
    Scene { world, eye, yaw, pitch, entities: vec![], day_time: 6000, raining: false, digging: None, hud: HudState::default(), frame: 0 }
}

fn render(sc: &Scene) -> Vec<u8> {
    let r = Renderer { settings: Settings { width: 160, height: 90, ..Default::default() }, info: palette::build_table(), textures: None };
    let mut buf = vec![0u8; r.frame_bytes()];
    r.render(sc, &mut buf);
    buf
}

fn px(buf: &[u8], x: usize, y: usize) -> [u8; 3] {
    let i = (y * 160 + x) * 3;
    [buf[i], buf[i + 1], buf[i + 2]]
}

#[test]
fn sky_above_ground_below() {
    let sc = scene(flat(), [0.5, -58.38, 0.5], 0.0, 0.0);
    let img = render(&sc);
    let top = px(&img, 80, 5);
    let bottom = px(&img, 80, 70);
    assert!(top[2] > top[0], "sky should be blue-ish: {top:?}");
    assert!(bottom[1] > bottom[2], "ground should be green grass: {bottom:?}");
}

#[test]
fn mining_a_block_changes_the_frame() {
    let mut w = flat();
    w.set(0, -60, 2, 1); // a stone block in front at eye level-ish
    w.set(0, -59, 2, 1);
    let sc = scene(w.clone(), [0.5, -58.38, 0.5], 0.0, 10.0);
    let before = render(&sc);
    w.set(0, -59, 2, 0);
    w.set(0, -60, 2, 0);
    let sc2 = scene(w, [0.5, -58.38, 0.5], 0.0, 10.0);
    let after = render(&sc2);
    let diff = before.iter().zip(&after).filter(|(a, b)| a != b).count();
    assert!(diff > 500, "removing the block should change many pixels (changed {diff})");
}

#[test]
fn walking_moves_the_camera() {
    let mut w = flat();
    for z in 6..8 {
        w.set(0, -59, z, 1);
    }
    let a = render(&scene(w.clone(), [0.5, -58.38, 0.5], 0.0, 0.0));
    let b = render(&scene(w, [0.5, -58.38, 3.5], 0.0, 0.0));
    let diff = a.iter().zip(&b).filter(|(x, y)| x != y).count();
    assert!(diff > 500, "moving closer must change the view (changed {diff})");
}

#[test]
fn entities_render_at_their_position() {
    let w = flat();
    let mut sc = scene(w, [0.5, -58.38, 0.5], 0.0, 0.0);
    let empty = render(&sc);
    sc.entities.push(EntityView { kind: "player".into(), pos: [0.5, -60.0, 5.5], width: 0.6, height: 1.8, yaw: 0.0, is_player: true, hurt: false });
    let with = render(&sc);
    // Player straight ahead → centre column changes, far left edge does not.
    assert_ne!(px(&empty, 80, 50), px(&with, 80, 50));
    assert_eq!(px(&empty, 3, 30), px(&with, 3, 30));
    // Turn 90° away: the player leaves the view.
    sc.yaw = 90.0;
    let away = render(&sc);
    sc.entities.clear();
    let away_empty = render(&sc);
    assert_eq!(away, away_empty);
}

#[test]
fn night_is_darker() {
    let mut day = scene(flat(), [0.5, -58.38, 0.5], 0.0, 30.0);
    let a = render(&day);
    day.day_time = 18000;
    let b = render(&day);
    let sum = |v: &[u8]| v.iter().map(|x| *x as u64).sum::<u64>();
    assert!(sum(&b) < sum(&a) / 2);
}
