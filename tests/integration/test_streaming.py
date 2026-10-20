"""Streaming acceptance tests: the MineLive video is the bot's real POV.

Pipeline under test: server → OmniCore → OmniView → FFmpeg → MediaMTX → viewer.
Requires: ffmpeg, runtime/bin/mediamtx, and a release build of the bot
(OMNI_BOT_BIN defaults to target/release/omniplayer-bot here).
"""
import os
import shutil
import subprocess
import tempfile
import time
import unittest
import urllib.request

import harness
from harness import ROOT, Bot, Server

MEDIAMTX = os.environ.get("MEDIAMTX_BIN", os.path.join(ROOT, "runtime", "bin", "mediamtx"))
harness.BOT_BIN = os.environ.get("OMNI_BOT_BIN", os.path.join(ROOT, "target", "release", "omniplayer-bot"))


def grab_frame(url, path):
    subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-rtsp_transport", "tcp", "-i", url, "-frames:v", "1",
                    "-y", path], check=True, timeout=30)


def frame_diff(a, b):
    """Mean absolute difference between two PNG frames (0..255), via ffmpeg raw output."""
    def raw(p):
        return subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-i", p, "-f", "rawvideo", "-pix_fmt", "gray", "-"],
                              capture_output=True, check=True).stdout
    x, y = raw(a), raw(b)
    n = min(len(x), len(y))
    return sum(abs(x[i] - y[i]) for i in range(0, n, 7)) / (n / 7)


@unittest.skipUnless(os.path.exists(MEDIAMTX) and shutil.which("ffmpeg"), "needs mediamtx + ffmpeg")
class StreamingTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix="omni-stream-")
        shutil.copy(os.path.join(ROOT, "infra", "mediamtx", "mediamtx.yml"), cls.tmp)
        cls.mtx = subprocess.Popen([MEDIAMTX, "mediamtx.yml"], cwd=cls.tmp, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(1.5)
        cls.server = Server()
        cls.bot = Bot("StreamBot", cls.server.port, mode="idle")
        cls.bot.wait_event("event", lambda d: d.get("type") == "spawned", timeout=40)
        cls.bot.wait_status(lambda s: s.get("chunks_loaded", 0) > 20, 30)
        cls.bot.send("stream_start", config={"output": "rtsp://127.0.0.1:8554/live/streambot", "fps": 30, "title": "StreamBot"})
        cls.bot.wait_event("stream_started", timeout=20)
        time.sleep(4)

    @classmethod
    def tearDownClass(cls):
        cls.bot.send("stream_stop")
        cls.bot.stop()
        cls.server.stop()
        cls.mtx.terminate()
        cls.mtx.wait(timeout=10)
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def test_01_frames_arrive_at_viewer_at_720p30(self):
        out = subprocess.run(["ffprobe", "-hide_banner", "-loglevel", "error", "-rtsp_transport", "tcp", "-select_streams", "v:0",
                              "-show_entries", "stream=codec_name,width,height,avg_frame_rate", "-of", "csv=p=0",
                              "rtsp://127.0.0.1:8554/live/streambot"], capture_output=True, text=True, timeout=30).stdout.strip()
        codec, w, h, rate = out.split(",")[:4]
        self.assertEqual((codec, w, h), ("h264", "1280", "720"))
        # The stream adapts its render size to hold 30 fps; wait for a steady window.
        stats = self.bot.wait_event("stream_stats", lambda d: d["fps"] >= 28.0, timeout=60)
        print(f"\n  bot-side stream window: {stats}")
        # Measure the decoded frame rate at the viewer over a steady window
        # (frames before the first keyframe are discarded by the decoder).
        r = subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-rtsp_transport", "tcp", "-i", "rtsp://127.0.0.1:8554/live/streambot",
                            "-t", "10", "-progress", "pipe:1", "-f", "null", "-"], capture_output=True, text=True, timeout=60)
        samples, frame = [], None
        for line in r.stdout.splitlines():
            if line.startswith("frame="):
                frame = int(line.split("=")[1])
            elif line.startswith("out_time_us=") and frame is not None and line.split("=")[1].lstrip("-").isdigit():
                samples.append((int(line.split("=")[1]) / 1e6, frame))
        steady = [s for s in samples if s[0] >= 3.0]
        self.assertGreater(len(steady), 2, f"progress samples: {samples}")
        (t1, f1), (t2, f2) = steady[0], steady[-1]
        viewer_fps = (f2 - f1) / (t2 - t1)
        print(f"  viewer decoded {f2} frames; steady rate {viewer_fps:.1f} fps")
        self.assertGreaterEqual(viewer_fps, 28.0)

    def test_02_hls_fallback_is_served(self):
        deadline = time.time() + 20
        while True:
            try:
                with urllib.request.urlopen("http://127.0.0.1:8888/live/streambot/index.m3u8", timeout=10) as r:
                    body = r.read().decode()
                    self.assertIn("#EXTM3U", body)
                    return
            except Exception:
                if time.time() > deadline:
                    raise
                time.sleep(1)

    def test_03_walking_changes_the_view(self):
        a = os.path.join(self.tmp, "a.png")
        b = os.path.join(self.tmp, "b.png")
        grab_frame("rtsp://127.0.0.1:8554/live/streambot", a)
        self.bot.send("walk", ticks=40, yaw=180)
        time.sleep(3)
        grab_frame("rtsp://127.0.0.1:8554/live/streambot", b)
        self.assertGreater(frame_diff(a, b), 3.0)

    def test_04_mining_updates_the_view(self):
        self.bot.send("look", yaw=0, pitch=60)
        time.sleep(2)
        a = os.path.join(self.tmp, "m1.png")
        b = os.path.join(self.tmp, "m2.png")
        grab_frame("rtsp://127.0.0.1:8554/live/streambot", a)
        st = self.bot.status
        x, y, z = (int(v // 1) for v in st["pos"])
        self.bot.send("dig", x=x, y=y - 1, z=z + 1)
        self.bot.wait_event("goal_done", lambda d: d.get("goal") == "dig a block", timeout=30)
        self.bot.send("look", yaw=0, pitch=60)
        time.sleep(2)
        grab_frame("rtsp://127.0.0.1:8554/live/streambot", b)
        self.assertGreater(frame_diff(a, b), 2.0)

    def test_05_independent_channels(self):
        other = Bot("StreamBot2", self.server.port, mode="idle")
        try:
            other.wait_event("event", lambda d: d.get("type") == "spawned", timeout=40)
            other.wait_status(lambda s: s.get("chunks_loaded", 0) > 20, 30)
            other.send("look", yaw=90, pitch=-20)
            other.send("stream_start", config={"output": "rtsp://127.0.0.1:8554/live/streambot2", "fps": 30, "title": "StreamBot2", "threads": 1})
            other.wait_event("stream_started", timeout=20)
            self.bot.send("look", yaw=270, pitch=40)
            time.sleep(4)
            a = os.path.join(self.tmp, "c1.png")
            b = os.path.join(self.tmp, "c2.png")
            grab_frame("rtsp://127.0.0.1:8554/live/streambot", a)
            grab_frame("rtsp://127.0.0.1:8554/live/streambot2", b)
            self.assertGreater(frame_diff(a, b), 5.0)
        finally:
            other.send("stream_stop")
            other.stop()


if __name__ == "__main__":
    unittest.main(verbosity=2)
