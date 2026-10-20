"""Gameplay acceptance tests (v0.3 scope) against a live server.

Crafting is verified only on a vanilla/Paper server (MC_SERVER_JAR), because the
Minestom test counterpart does not compute crafting results.
"""
import math
import os
import time
import unittest

from harness import Bot, Server


def dist(a, b):
    return math.sqrt(sum((x - y) ** 2 for x, y in zip(a, b)))


class GameplayTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = Server()

    @classmethod
    def tearDownClass(cls):
        cls.server.stop()

    def spawn(self, name, **kw):
        b = Bot(name, self.server.port, **kw)
        self.addCleanup(b.stop)
        b.wait_event("event", lambda d: d.get("type") == "spawned", timeout=40)
        b.wait_status(lambda s: s.get("chunks_loaded", 0) > 20, 30)
        return b

    def server_pos(self, name):
        m = self.server.mark()
        self.server.send(f"pos {name}")
        match, _ = self.server.wait_for(rf"\[TEST\] POS {name} ([-\d.]+) ([-\d.]+) ([-\d.]+)", 10, m)
        return [float(match.group(i)) for i in (1, 2, 3)]

    def test_collects_wood(self):
        bot = self.spawn("WoodBot")
        m = self.server.mark()
        bot.send("gather", what="wood", count=2)
        self.server.wait_for(r"\[TEST\] BREAK WoodBot \S+ \S+ \S+ oak_log", 90, m)
        self.server.wait_for(r"\[TEST\] PICKUP WoodBot oak_log 1", 60, m)
        bot.wait_event("goal_done", lambda d: d.get("goal") == "gather wood", timeout=120)
        st = bot.wait_status(lambda s: dict(s["inventory"]).get("oak_log", 0) >= 2, 20)
        self.assertGreaterEqual(dict(st["inventory"])["oak_log"], 2)

    def test_navigates_around_obstacles(self):
        bot = self.spawn("NavBot")
        m = self.server.mark()
        # A 3-high wall between the bot and its goal.
        self.server.send("wall -6 4 6 4 60 3")
        self.server.wait_for(r"\[TEST\] WALL done", 10, m)
        time.sleep(1.0)
        bot.send("goto", x=0, y=60, z=9, tolerance=1.0)
        bot.wait_event("goal_done", lambda d: d.get("goal") == "go to a location", timeout=90)
        p = self.server_pos("NavBot")
        self.assertLess(dist([p[0], p[2]], [0.5, 9.5]), 2.0, f"server says bot is at {p}")

    def test_fights_hostile_mob(self):
        bot = self.spawn("FightBot")
        m = self.server.mark()
        self.server.send("zombie 3 60 -6")
        self.server.wait_for(r"\[TEST\] SPAWNED zombie", 10, m)
        bot.wait_status(lambda s: any(e["kind"] == "zombie" for e in s.get("nearby", [])), 20)
        bot.send("attack_nearest", kind="zombie")
        self.server.wait_for(r"\[TEST\] ATTACK FightBot zombie", 40, m)
        self.server.wait_for(r"\[TEST\] KILLED FightBot zombie", 60, m)

    def test_follows_human_on_chat_command(self):
        follower = self.spawn("FollowBot", mode="idle")
        human = self.spawn("HumanTester", mode="idle")  # a second real client playing the "human"
        m = self.server.mark()
        b = follower.mark()
        human.send("chat", text="FollowBot follow me")
        self.server.wait_for(r"\[TEST\] CHAT HumanTester FollowBot follow me", 10, m)
        follower.wait_event("intent", lambda d: d.get("intent") == "Follow", timeout=15, start=b)
        self.server.wait_for(r"\[TEST\] CHAT FollowBot Sure HumanTester", 15, m)
        human.send("goto", x=-10, y=60, z=-10, tolerance=1.0)
        human.wait_event("goal_done", lambda d: d.get("goal") == "go to a location", timeout=60)
        time.sleep(6)
        hp = self.server_pos("HumanTester")
        fp = self.server_pos("FollowBot")
        self.assertLess(dist(hp, fp), 5.0, f"human at {hp}, follower at {fp}")

    def test_answers_questions_from_real_state(self):
        bot = self.spawn("TalkBot", mode="idle")
        human = self.spawn("Asker", mode="idle")
        m = self.server.mark()
        human.send("chat", text="TalkBot where are you?")
        match, _ = self.server.wait_for(r"\[TEST\] CHAT TalkBot I'm at (-?\d+) (-?\d+) (-?\d+)", 20, m)
        p = self.server_pos("TalkBot")
        self.assertLess(dist([float(match.group(1)), float(match.group(3))], [math.floor(p[0]), math.floor(p[2])]), 1.5)

    @unittest.skipUnless(os.environ.get("MC_SERVER_JAR"), "crafting needs a vanilla/Paper server (MC_SERVER_JAR)")
    def test_crafts_table_and_tools(self):
        bot = self.spawn("CraftBot")
        bot.send("gather", what="wood", count=4)
        bot.wait_event("goal_done", lambda d: d.get("goal") == "gather wood", timeout=180)
        for item in ["planks", "planks", "crafting_table", "stick", "wooden_pickaxe"]:
            bot.send("craft", item=item)
            bot.wait_event("goal_done", lambda d, i=item: d.get("goal") == f"craft {i}", timeout=60)
        bot.wait_status(lambda s: dict(s["inventory"]).get("wooden_pickaxe", 0) == 1, 20)


if __name__ == "__main__":
    unittest.main(verbosity=2)
