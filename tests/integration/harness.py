"""Integration-test harness: real server process + real omniplayer-bot processes.

Assertions are made on what the *server* reports (its log/console), not on
what the bot claims about itself.

Server selection (env):
  MC_SERVER_JAR=/path/server.jar  -> official vanilla/Paper jar (needs MC_ACCEPT_EULA=1)
  otherwise                       -> tests/test-server (Minestom 26.3 test counterpart)
"""
import json
import os
import queue
import re
import shutil
import socket
import subprocess
import tempfile
import threading
import time

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
BOT_BIN = os.environ.get("OMNI_BOT_BIN", os.path.join(ROOT, "target", "debug", "omniplayer-bot"))
TEST_SERVER = os.path.join(ROOT, "tests", "test-server", "build", "install", "omniplayer-test-server", "bin", "omniplayer-test-server")
JAVA_HOME = os.environ.get("JAVA_HOME_25", "/usr/lib/jvm/java-25-openjdk-amd64")


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


class Server:
    """A running Minecraft server whose console output is captured."""

    def __init__(self, port=None):
        self.port = port or free_port()
        self.lines = []
        self.cond = threading.Condition()
        self.vanilla = bool(os.environ.get("MC_SERVER_JAR"))
        self.dir = tempfile.mkdtemp(prefix="omni-mc-")
        env = dict(os.environ, JAVA_HOME=JAVA_HOME)
        if self.vanilla:
            if os.environ.get("MC_ACCEPT_EULA") != "1":
                raise RuntimeError("Set MC_ACCEPT_EULA=1 to confirm you accept the Minecraft EULA")
            with open(os.path.join(self.dir, "eula.txt"), "w") as f:
                f.write("eula=true\n")
            with open(os.path.join(self.dir, "server.properties"), "w") as f:
                f.write(f"server-port={self.port}\nonline-mode=false\nlevel-type=minecraft\\:flat\nspawn-protection=0\n"
                        "enforce-secure-profile=false\ngamemode=survival\ndifficulty=peaceful\nview-distance=6\n")
            cmd = [os.path.join(JAVA_HOME, "bin", "java"), "-Xmx2G", "-jar", os.environ["MC_SERVER_JAR"], "nogui"]
        else:
            if not os.path.exists(TEST_SERVER):
                raise RuntimeError("build the test server first: (cd tests/test-server && ./gradlew installDist)")
            cmd = [TEST_SERVER, str(self.port)]
        self.proc = subprocess.Popen(cmd, cwd=self.dir, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.STDOUT, text=True, bufsize=1)
        threading.Thread(target=self._pump, daemon=True).start()
        if self.vanilla:
            self.wait_for(r"Done \(", 240)
        else:
            self.wait_for(r"\[TEST\] READY", 120)

    def _pump(self):
        for line in self.proc.stdout:
            with self.cond:
                self.lines.append(line.rstrip("\n"))
                self.cond.notify_all()

    def send(self, cmd):
        self.proc.stdin.write(cmd + "\n")
        self.proc.stdin.flush()

    def wait_for(self, pattern, timeout=30, start=0):
        rx = re.compile(pattern)
        deadline = time.time() + timeout
        with self.cond:
            while True:
                for i in range(start, len(self.lines)):
                    m = rx.search(self.lines[i])
                    if m:
                        return m, i
                left = deadline - time.time()
                if left <= 0:
                    tail = "\n".join(self.lines[-25:])
                    raise AssertionError(f"server never logged /{pattern}/ within {timeout}s. Last lines:\n{tail}")
                self.cond.wait(left)

    def mark(self):
        with self.cond:
            return len(self.lines)

    # Normalised queries (Minestom test server and vanilla)
    def online_players(self):
        m0 = self.mark()
        self.send("list")
        if self.vanilla:
            m, _ = self.wait_for(r"players online:(.*)$", 10, m0)
            return [p.strip() for p in m.group(1).split(",") if p.strip()]
        m, _ = self.wait_for(r"\[TEST\] LIST (.*)$", 10, m0)
        return [p for p in m.group(1).split(",") if p]

    def join_pattern(self, name):
        return rf"{name} joined the game" if self.vanilla else rf"\[TEST\] JOIN {name} "

    def leave_pattern(self, name):
        return rf"{name} left the game" if self.vanilla else rf"\[TEST\] LEAVE {name}"

    def chat_pattern(self, name, text):
        return rf"<{name}> {re.escape(text)}" if self.vanilla else rf"\[TEST\] CHAT {name} {re.escape(text)}"

    def stop(self):
        try:
            self.send("stop")
            self.proc.wait(timeout=20)
        except Exception:
            self.proc.kill()
        shutil.rmtree(self.dir, ignore_errors=True)


class Bot:
    """A real omniplayer-bot process controlled over its JSON stdin/stdout interface."""

    def __init__(self, username, port, host="127.0.0.1", mode="idle", extra=None):
        self.username = username
        cfg = {
            "bot_id": username, "username": username, "host": host, "port": port, "version": "26.3",
            "account": {"type": "offline"}, "mode": mode, "reconnect": True, "view_distance": 4,
            "brain": {"think_interval_ticks": 10, "allow_pvp": True},
            "persona": {"streamer_name": username, "chattiness": 0.5, "energy": 0.6},
        }
        if extra:
            for k, v in extra.items():
                cfg[k] = v
        fd, self.cfg_path = tempfile.mkstemp(suffix=".json")
        with os.fdopen(fd, "w") as f:
            json.dump(cfg, f)
        self.events = []
        self.cond = threading.Condition()
        self.status = {}
        self.proc = subprocess.Popen([BOT_BIN, "--config", self.cfg_path], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE, text=True, bufsize=1)
        threading.Thread(target=self._pump, daemon=True).start()

    def _pump(self):
        for line in self.proc.stdout:
            try:
                ev = json.loads(line)
            except json.JSONDecodeError:
                continue
            with self.cond:
                if ev.get("type") == "status":
                    self.status = ev["data"]
                self.events.append(ev)
                self.cond.notify_all()

    def send(self, cmd, **kw):
        kw["cmd"] = cmd
        self.proc.stdin.write(json.dumps(kw) + "\n")
        self.proc.stdin.flush()

    def mark(self):
        with self.cond:
            return len(self.events)

    def wait_event(self, etype, pred=lambda d: True, timeout=30, start=0):
        deadline = time.time() + timeout
        with self.cond:
            while True:
                for ev in self.events[start:]:
                    if ev.get("type") == etype and pred(ev.get("data") or {}):
                        return ev.get("data") or {}
                left = deadline - time.time()
                if left <= 0:
                    kinds = [e.get("type") for e in self.events[-30:]]
                    raise AssertionError(f"{self.username} never emitted {etype} within {timeout}s; recent: {kinds}")
                self.cond.wait(left)

    def wait_status(self, pred, timeout=30):
        deadline = time.time() + timeout
        with self.cond:
            while True:
                if self.status and pred(self.status):
                    return self.status
                left = deadline - time.time()
                if left <= 0:
                    raise AssertionError(f"status condition not met; last status: {json.dumps(self.status)[:600]}")
                self.cond.wait(left)

    def stop(self):
        try:
            self.send("disconnect")
            self.proc.wait(timeout=10)
        except Exception:
            self.proc.kill()
        os.unlink(self.cfg_path)
