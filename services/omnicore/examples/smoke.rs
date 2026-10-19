//! Minimal end-to-end check: connect, spawn, walk, look, chat, dig, disconnect.
//! Usage: cargo run -p omnicore --example smoke -- <host> <port> <username>

use omnicore::physics::Controls;
use omnicore::{Client, ClientConfig, Event};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let host = args.get(1).map(String::as_str).unwrap_or("127.0.0.1");
    let port: u16 = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(25565);
    let name = args.get(3).map(String::as_str).unwrap_or("OmniSmoke");

    let status = omnicore::client::ping_status(host, port, Duration::from_secs(5)).await?;
    println!("status: {status:?}");
    let (client, mut events) = Client::connect(ClientConfig::offline(host, port, name)).await?;
    let c2 = client.clone();
    tokio::spawn(async move {
        while let Some(e) = events.recv().await {
            match &e {
                Event::ChunkLoaded { .. } | Event::BlockChanged { .. } => {}
                _ => println!("event: {}", serde_json::to_string(&e).unwrap()),
            }
            if let Event::Disconnected { .. } = e {
                break;
            }
        }
        drop(c2);
    });
    let mut ticker = tokio::time::interval(Duration::from_millis(50));
    for t in 0..200u32 {
        ticker.tick().await;
        let mut ctl = Controls::default();
        if (40..80).contains(&t) {
            ctl.forward = 1.0;
        }
        if t == 90 {
            client.set_rotation(90.0, 30.0);
        }
        if t == 100 {
            client.chat("Hello from OmniCore!");
        }
        if t == 110 {
            let st = client.state.lock().unwrap();
            let p = st.body.pos;
            println!("pos {:?} chunks {} batches {}", p, st.world.chunks.len(), st.chunk_batches);
            let below = (p[0].floor() as i32, p[1].floor() as i32 - 1, p[2].floor() as i32);
            println!("block below {:?} = {}", below, st.world.block_name(below.0, below.1, below.2));
            drop(st);
            client.start_digging(below, 1);
        }
        if t == 130 {
            let st = client.state.lock().unwrap();
            let p = st.body.pos;
            drop(st);
            client.finish_digging((p[0].floor() as i32, p[1].floor() as i32 - 1, p[2].floor() as i32), 1);
        }
        client.tick(&ctl);
        if client.is_closed() {
            break;
        }
    }
    client.disconnect();
    tokio::time::sleep(Duration::from_millis(300)).await;
    Ok(())
}
