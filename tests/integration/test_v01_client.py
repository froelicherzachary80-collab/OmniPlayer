"""Version 0.1 acceptance test: a custom OmniCore bot joins a real server and
interacts with the world over the Minecraft 26.3 protocol.

Run:  python3 -m unittest tests/integration/test_v01_client.py -v
"""
import re
import time
import unittest

from harness import Bot, Server


class V01ClientTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = Server()
        cls.bot = Bot("NovaTest", cls.server.port, mode="idle")

    @classmethod
    def tearDownClass(cls):
        cls.bot.stop()
        cls.server.stop()

    def test_01_connects_and_spawns(self):
        data = self.bot.wait_event("connected", timeout=30)
        self.assertEqual(data["protocol"], 777)
        self.server.wait_for(self.server.join_pattern("NovaTest"), 30)
        self.bot.wait_event("event", lambda d: d.get("type") == "spawned", timeout=30)

    def test_02_appears_in_player_list(self):
        self.server.wait_for(self.server.join_pattern("NovaTest"), 30)
        self.assertIn("NovaTest", self.server.online_players())

    def test_03_receives_real_world_data(self):
        st = self.bot.wait_status(lambda s: s.get("chunks_loaded", 0) > 20 and s.get("block_below") not in (None, "air", "unknown"), 30)
        # The test server's surface is grass at y=59; vanilla flat worlds also have grass on top.
        self.assertEqual(st["block_below"], "grass_block")

    def test_04_moves(self):
        st = self.bot.wait_status(lambda s: s.get("pos") is not None, 20)
        z0 = st["pos"][2]
        m = self.server.mark()
        self.bot.send("walk", ticks=40, yaw=0)
        if not self.server.vanilla:
            # Server-side position must advance along +Z.
            deadline = time.time() + 20
            moved = False
            while time.time() < deadline and not moved:
                match, idx = self.server.wait_for(r"\[TEST\] MOVE NovaTest ([-\d.]+) ([-\d.]+) ([-\d.]+)", 20, m)
                m = idx + 1
                moved = float(match.group(3)) - z0 > 3.0
            self.assertTrue(moved, "server did not observe the bot walking 3+ blocks")
        st = self.bot.wait_status(lambda s: s["pos"][2] - z0 > 3.0, 20)

    def test_05_turns_head(self):
        m = self.server.mark()
        self.bot.send("look", yaw=90, pitch=30)
        self.bot.wait_status(lambda s: abs(s["yaw"] - 90) < 2 and abs(s["pitch"] - 30) < 2, 15)
        if not self.server.vanilla:
            # Ask the server where the bot is looking (server-side state, not the bot's claim).
            m = self.server.mark()
            self.server.send("pos NovaTest")
            match, _ = self.server.wait_for(r"\[TEST\] POS NovaTest \S+ \S+ \S+ ([-\d.]+) ([-\d.]+)", 10, m)
            self.assertAlmostEqual(float(match.group(1)) % 360, 90.0, delta=3)
            self.assertAlmostEqual(float(match.group(2)), 30.0, delta=3)

    def test_06_sends_chat(self):
        m = self.server.mark()
        self.bot.send("chat", text="Hello from OmniCore v0.1!")
        self.server.wait_for(self.server.chat_pattern("NovaTest", "Hello from OmniCore v0.1!"), 15, m)

    def test_07_breaks_block_and_inventory_changes(self):
        st = self.bot.wait_status(lambda s: s.get("pos") is not None, 10)
        x, y, z = (int(v // 1) for v in st["pos"])
        m = self.server.mark()
        self.bot.send("dig_below")
        if not self.server.vanilla:
            self.server.wait_for(rf"\[TEST\] BREAK NovaTest {x} {y - 1} {z} grass_block", 20, m)
            self.server.wait_for(r"\[TEST\] PICKUP NovaTest grass_block 1", 20, m)
            self.server.send(f"block {x} {y - 1} {z}")
            self.server.wait_for(rf"\[TEST\] BLOCK {x} {y - 1} {z} air", 10, m)
        self.bot.wait_status(lambda s: any(n == "grass_block" for n, _ in s.get("inventory", [])), 20)

    def test_08_disconnects_and_reconnects(self):
        m = self.server.mark()
        b = self.bot.mark()
        self.bot.send("reconnect")
        self.server.wait_for(self.server.leave_pattern("NovaTest"), 20, m)
        self.bot.wait_event("connected", timeout=60, start=b)
        self.server.wait_for(self.server.join_pattern("NovaTest"), 30, m)
        self.assertIn("NovaTest", self.server.online_players())


if __name__ == "__main__":
    unittest.main(verbosity=2)
