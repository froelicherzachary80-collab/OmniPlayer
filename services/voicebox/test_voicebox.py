import os
import shutil
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))
import voicebox  # noqa: E402


@unittest.skipUnless(shutil.which("espeak-ng") and shutil.which("ffmpeg"), "needs espeak-ng + ffmpeg")
class VoiceBoxTest(unittest.TestCase):
    def test_produces_24k_pcm_with_pauses(self):
        one = voicebox.synthesize({"text": "WAIT, NO WAY! I actually found diamonds down here!", "emotion": "excited"})
        self.assertGreater(len(one), voicebox.RATE)  # > 0.5 s of 16-bit audio
        self.assertEqual(len(one) % 2, 0)
        two = voicebox.synthesize({"text": "Okay chat. I think we should add a second floor."})
        # The inter-sentence pause is real silence.
        self.assertIn(b"\x00\x00" * int(voicebox.RATE * 0.2), two)

    def test_rate_changes_length(self):
        slow = voicebox.synthesize({"text": "Let's build a house together today", "rate": 0.7})
        fast = voicebox.synthesize({"text": "Let's build a house together today", "rate": 1.6})
        self.assertGreater(len(slow), len(fast))

    def test_rejects_empty(self):
        with self.assertRaises(ValueError):
            voicebox.synthesize({"text": "  "})


if __name__ == "__main__":
    unittest.main()
