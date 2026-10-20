//! Live stream pipeline for one bot:
//! OmniCore world state → OmniView renderer → FFmpeg (H.264/Opus) → RTSP (MediaMTX).
//! Audio: a real-time PCM pipe fed with VoiceBox speech (silence otherwise).
//!
//! Rendering runs on its own threads; gameplay never waits on it.

use crate::io::Output;
use anyhow::{bail, Context, Result};
use omnicore::Client;
use omniview::{Renderer, Settings};
use serde::Deserialize;
use serde_json::json;
use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const AUDIO_RATE: usize = 24_000;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct StreamConfig {
    /// rtsp://host:8554/path, or a file path (.ts/.mp4/.mkv) for local recording/tests.
    pub output: String,
    pub width: usize,
    pub height: usize,
    pub out_width: usize,
    pub out_height: usize,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub max_distance: f64,
    pub threads: usize,
    pub title: String,
    pub font: String,
    pub ffmpeg: String,
    /// Stop automatically after N seconds (0 = run until stopped).
    pub duration_s: u64,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            output: String::new(),
            // Internal render size; 480x270 sustains 30 fps on 3 CPU cores and is
            // upscaled (nearest neighbour) to the 1280x720 output.
            width: 480,
            height: 270,
            out_width: 1280,
            out_height: 720,
            fps: 30,
            bitrate_kbps: 2500,
            max_distance: 48.0,
            threads: std::thread::available_parallelism().map(|n| n.get().saturating_sub(1).max(1)).unwrap_or(2),
            title: String::new(),
            font: "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf".into(),
            ffmpeg: "ffmpeg".into(),
            duration_s: 0,
        }
    }
}

struct Shared {
    client: Mutex<Option<Client>>,
    audio: Mutex<VecDeque<i16>>,
    caption: Mutex<(String, Instant)>,
    stop: AtomicBool,
    frames: AtomicU64,
}

pub struct Streamer {
    shared: Arc<Shared>,
    child: Option<Child>,
    threads: Vec<std::thread::JoinHandle<()>>,
    dir: PathBuf,
    out: Output,
}

fn write_atomic(path: &PathBuf, text: &str) {
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

fn escape_filter_path(p: &str) -> String {
    p.replace('\\', "\\\\").replace(':', "\\:").replace('\'', "\\'")
}

impl Streamer {
    pub fn start(cfg: StreamConfig, client: Client, out: Output) -> Result<Self> {
        if cfg.output.is_empty() {
            bail!("stream output is required (rtsp://... or a file path)");
        }
        let dir = std::env::temp_dir().join(format!("omniview-{}-{}", out.bot, std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let fifo = dir.join("audio.pcm");
        let _ = std::fs::remove_file(&fifo);
        let status = Command::new("mkfifo").arg(&fifo).status().context("mkfifo")?;
        if !status.success() {
            bail!("could not create audio FIFO");
        }
        let info_file = dir.join("info.txt");
        let cap_file = dir.join("caption.txt");
        write_atomic(&info_file, &cfg.title);
        write_atomic(&cap_file, " ");

        let fs = (cfg.out_height as f32 / 30.0).round() as u32;
        let font = escape_filter_path(&cfg.font);
        let filter = format!(
            "[0:v]scale={ow}:{oh}:flags=neighbor,\
             drawtext=fontfile='{font}':textfile='{info}':reload=1:x=14:y=14:fontsize={fs}:fontcolor=white:box=1:boxcolor=black@0.45:boxborderw=8,\
             drawtext=fontfile='{font}':textfile='{cap}':reload=1:x=(w-text_w)/2:y=h-{capy}:fontsize={fs2}:fontcolor=white:box=1:boxcolor=black@0.55:boxborderw=10[v]",
            ow = cfg.out_width,
            oh = cfg.out_height,
            info = escape_filter_path(&info_file.to_string_lossy()),
            cap = escape_filter_path(&cap_file.to_string_lossy()),
            fs = fs,
            fs2 = fs + 4,
            capy = cfg.out_height / 5 + 20,
        );
        let gop = (cfg.fps * 2).to_string();
        let br = format!("{}k", cfg.bitrate_kbps);
        let buf = format!("{}k", cfg.bitrate_kbps * 2);
        let size = format!("{}x{}", cfg.width, cfg.height);
        let mut cmd = Command::new(&cfg.ffmpeg);
        cmd.args(["-hide_banner", "-loglevel", "error", "-nostats"])
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &size, "-framerate", &cfg.fps.to_string(), "-i", "pipe:0"])
            .args(["-f", "s16le", "-ar", &AUDIO_RATE.to_string(), "-ac", "1", "-i"])
            .arg(&fifo)
            .args(["-filter_complex", &filter, "-map", "[v]", "-map", "1:a"])
            .args(["-c:v", "libx264", "-preset", "veryfast", "-tune", "zerolatency", "-pix_fmt", "yuv420p", "-profile:v", "baseline"])
            .args(["-g", &gop, "-b:v", &br, "-maxrate", &br, "-bufsize", &buf])
            .args(["-c:a", "libopus", "-b:a", "64k", "-ar", "48000"]);
        if cfg.output.starts_with("rtsp://") {
            cmd.args(["-f", "rtsp", "-rtsp_transport", "tcp"]).arg(&cfg.output);
        } else {
            cmd.arg("-y").arg(&cfg.output);
        }
        cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
        let mut child = cmd.spawn().context("starting ffmpeg (is it installed?)")?;
        let mut stdin = child.stdin.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let shared = Arc::new(Shared {
            client: Mutex::new(Some(client)),
            audio: Mutex::new(VecDeque::new()),
            caption: Mutex::new((String::new(), Instant::now())),
            stop: AtomicBool::new(false),
            frames: AtomicU64::new(0),
        });
        let mut threads = vec![];

        // ffmpeg stderr → log lines
        {
            let out = out.clone();
            threads.push(std::thread::spawn(move || {
                use std::io::BufRead;
                for l in std::io::BufReader::new(stderr).lines().map_while(Result::ok) {
                    out.log("warn", format!("ffmpeg: {l}"));
                }
            }));
        }
        // audio writer: real-time paced PCM
        {
            let sh = shared.clone();
            let fifo = fifo.clone();
            threads.push(std::thread::spawn(move || {
                let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(&fifo) else { return };
                let chunk = AUDIO_RATE / 50; // 20 ms
                let start = Instant::now();
                let mut written: u64 = 0;
                let mut buf = vec![0u8; chunk * 2];
                while !sh.stop.load(Ordering::Relaxed) {
                    {
                        let mut q = sh.audio.lock().unwrap();
                        for i in 0..chunk {
                            let s = q.pop_front().unwrap_or(0);
                            buf[i * 2..i * 2 + 2].copy_from_slice(&s.to_le_bytes());
                        }
                    }
                    if f.write_all(&buf).is_err() {
                        break;
                    }
                    written += chunk as u64;
                    let due = Duration::from_micros(written * 1_000_000 / AUDIO_RATE as u64);
                    let el = start.elapsed();
                    if due > el {
                        std::thread::sleep(due - el);
                    }
                }
            }));
        }
        // renderer
        {
            let sh = shared.clone();
            let out2 = out.clone();
            let cfg2 = cfg.clone();
            let info_file = info_file.clone();
            let cap_file = cap_file.clone();
            threads.push(std::thread::spawn(move || {
                // Adaptive quality: render at the largest preset that holds the target
                // frame rate and upscale (nearest neighbour) to the fixed encoder input.
                let (bw, bh) = (cfg2.width, cfg2.height);
                let levels: Vec<(usize, usize)> = [1.0, 0.89, 0.8, 0.67, 0.5]
                    .iter()
                    .map(|k| (((bw as f64 * k) as usize).max(64) & !1, ((bh as f64 * k) as usize).max(36) & !1))
                    .collect();
                let mut renderers: Vec<Option<Renderer>> = levels.iter().map(|_| None).collect();
                let mut level = 0usize;
                let mut small = vec![0u8; bw * bh * 3];
                let mut frame = vec![0u8; bw * bh * 3];
                let period = Duration::from_secs_f64(1.0 / cfg2.fps as f64);
                let start = Instant::now();
                let mut n: u64 = 0;
                let mut last_info = Instant::now() - Duration::from_secs(5);
                let mut window_start = Instant::now();
                let mut window_frames = 0u64;
                let mut window_render = Duration::ZERO;
                let mut fast_windows = 0u32;
                let radius = (cfg2.max_distance / 16.0).ceil() as i32 + 1;
                while !sh.stop.load(Ordering::Relaxed) {
                    let t0 = Instant::now();
                    let (lw, lh) = levels[level];
                    let renderer = renderers[level].get_or_insert_with(|| {
                        Renderer::new(Settings { width: lw, height: lh, max_distance: cfg2.max_distance, threads: cfg2.threads, ..Default::default() })
                    });
                    let scene = {
                        let c = sh.client.lock().unwrap();
                        c.as_ref().map(|c| {
                            let st = c.state.lock().unwrap();
                            let info = format!(
                                "{}  |  {:.0} {:.0} {:.0}  |  {}  |  {}",
                                if cfg2.title.is_empty() { st.username.clone() } else { cfg2.title.clone() },
                                st.body.pos[0],
                                st.body.pos[1],
                                st.body.pos[2],
                                {
                                    let p = st.body.pos;
                                    st.world.biome_at(p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32).unwrap_or("").replace('_', " ")
                                },
                                if st.spawned { "LIVE" } else { "joining..." },
                            );
                            (omniview::scene_from_state(&st, n, radius), info)
                        })
                    };
                    match scene {
                        Some((sc, info)) => {
                            if (lw, lh) == (bw, bh) {
                                renderer.render(&sc, &mut frame);
                            } else {
                                let buf = &mut small[..lw * lh * 3];
                                renderer.render(&sc, buf);
                                upscale(buf, lw, lh, &mut frame, bw, bh);
                            }
                            if last_info.elapsed() > Duration::from_millis(500) {
                                write_atomic(&info_file, &info);
                                let (cap, at) = sh.caption.lock().unwrap().clone();
                                write_atomic(&cap_file, if at.elapsed() < Duration::from_secs(7) && !cap.is_empty() { &cap } else { " " });
                                last_info = Instant::now();
                            }
                        }
                        None => {
                            // Bot is reconnecting: keep the stream alive with a status card.
                            frame.iter_mut().for_each(|b| *b = 18);
                            write_atomic(&info_file, "Reconnecting to the Minecraft server...");
                        }
                    }
                    window_render += t0.elapsed();
                    if stdin.write_all(&frame).is_err() {
                        out2.emit("stream_error", json!({ "error": "ffmpeg closed its input" }));
                        break;
                    }
                    n += 1;
                    window_frames += 1;
                    sh.frames.store(n, Ordering::Relaxed);
                    if window_start.elapsed() >= Duration::from_secs(5) {
                        let secs = window_start.elapsed().as_secs_f64();
                        let fps = window_frames as f64 / secs;
                        let avg_ms = window_render.as_secs_f64() * 1000.0 / window_frames.max(1) as f64;
                        let budget_ms = 1000.0 / cfg2.fps as f64;
                        if fps < cfg2.fps as f64 * 0.93 && level + 1 < levels.len() {
                            level += 1;
                            fast_windows = 0;
                        } else if avg_ms < budget_ms * 0.5 && level > 0 {
                            fast_windows += 1;
                            if fast_windows >= 4 {
                                level -= 1;
                                fast_windows = 0;
                            }
                        }
                        out2.emit("stream_stats", json!({
                            "frames": n, "fps": (fps * 10.0).round() / 10.0, "avg_render_ms": (avg_ms * 10.0).round() / 10.0,
                            "render": format!("{}x{}", levels[level].0, levels[level].1), "uptime_s": start.elapsed().as_secs()
                        }));
                        window_start = Instant::now();
                        window_frames = 0;
                        window_render = Duration::ZERO;
                    }
                    if cfg2.duration_s > 0 && start.elapsed().as_secs() >= cfg2.duration_s {
                        break;
                    }
                    let due = period * n as u32;
                    let el = start.elapsed();
                    if due > el {
                        std::thread::sleep(due - el);
                    } else if el - due > Duration::from_secs(1) {
                        // Too far behind: drop the backlog instead of bursting frames.
                        n = (el.as_secs_f64() * cfg2.fps as f64) as u64;
                    }
                }
                drop(stdin);
            }));
        }
        out.emit("stream_started", json!({ "output": cfg.output, "width": cfg.out_width, "height": cfg.out_height, "fps": cfg.fps, "render": format!("{}x{}", cfg.width, cfg.height) }));
        Ok(Self { shared, child: Some(child), threads, dir, out })
    }

    pub fn attach(&mut self, c: Client) {
        *self.shared.client.lock().unwrap() = Some(c);
    }

    pub fn detach(&mut self) {
        *self.shared.client.lock().unwrap() = None;
    }

    pub fn on_event(&mut self, _e: &omnicore::Event) {}

    pub fn set_caption(&mut self, text: &str) {
        *self.shared.caption.lock().unwrap() = (text.chars().take(120).collect(), Instant::now());
    }

    pub fn queue_audio_b64(&mut self, b64: &str) {
        let Some(bytes) = b64_decode(b64) else { return };
        let mut q = self.shared.audio.lock().unwrap();
        for s in bytes.chunks_exact(2) {
            q.push_back(i16::from_le_bytes([s[0], s[1]]));
        }
        // Cap the queue at ~60 s of audio.
        while q.len() > AUDIO_RATE * 60 {
            q.pop_front();
        }
    }

    pub fn frames(&self) -> u64 {
        self.shared.frames.load(Ordering::Relaxed)
    }

    pub fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(mut c) = self.child.take() {
            // Give ffmpeg a moment to flush, then make sure it exits.
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Ok(Some(_)) = c.try_wait() {
                    break;
                }
                if Instant::now() > deadline {
                    let _ = c.kill();
                    let _ = c.wait();
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
        let _ = std::fs::remove_dir_all(&self.dir);
        self.out.emit("stream_stopped", json!({ "frames": self.frames() }));
    }
}

impl Drop for Streamer {
    fn drop(&mut self) {
        if self.child.is_some() {
            self.stop();
        }
    }
}

/// Nearest-neighbour upscale of an RGB24 image.
fn upscale(src: &[u8], sw: usize, sh: usize, dst: &mut [u8], dw: usize, dh: usize) {
    for y in 0..dh {
        let sy = y * sh / dh;
        for x in 0..dw {
            let sx = x * sw / dw;
            let si = (sy * sw + sx) * 3;
            let di = (y * dw + x) * 3;
            dst[di..di + 3].copy_from_slice(&src[si..si + 3]);
        }
    }
}

fn b64_decode(s: &str) -> Option<Vec<u8>> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lut = [255u8; 256];
    for (i, c) in T.iter().enumerate() {
        lut[*c as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for b in s.bytes() {
        if b == b'=' || b == b'\n' || b == b'\r' {
            continue;
        }
        let v = lut[b as usize];
        if v == 255 {
            return None;
        }
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64() {
        assert_eq!(super::b64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(super::b64_decode("AAEC").unwrap(), vec![0, 1, 2]);
    }
}
