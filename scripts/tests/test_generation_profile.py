"""Protect benchmark attribution: invalid evidence must fail, not look faster."""
import importlib.util
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

SPEC = importlib.util.spec_from_file_location(
    'profile_summary', Path(__file__).resolve().parents[1] / 'summarize-generation-profile.py')
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class GenerationProfileTests(unittest.TestCase):
    def setUp(self):
        self.directory = TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        root = Path(self.directory.name)
        self.csv = root / 'runs.csv'
        self.log = root / 'stages.log'
        self.csv.write_text(
            'seed,run,workers,milliseconds,vertices,triangles,rivers,geometry_hash\n'
            '17,1,2,100,3,1,1,42\n17,2,2,200,3,1,1,42\n')
        self.valid_log = (
            'profile,warmup_only,999\nprofile,island.generate,1000\n'
            'profile,child,10\nprofile,child,20\nprofile,parent,80\n'
            'profile,island.generate,100\nprofile,parent,160\n'
            'profile,island.generate,200\n')
        self.log.write_text(self.valid_log)

    def summarize(self):
        return MODULE.summarize(self.csv, self.log)

    def test_excludes_warmup_and_keeps_inclusive_repeated_and_absent_stages(self):
        fixture = self.summarize()['fixtures'][0]
        self.assertEqual(fixture['median_ms'], 150)
        stages = fixture['ranked_stages']
        self.assertEqual(len(stages), 2)
        self.assertEqual(stages[0]['median_inclusive_ms'], 120)
        self.assertEqual(stages[1]['median_inclusive_ms'], 15)
        self.assertEqual(stages[1]['calls_per_run'], [2, 0])

    def test_rejects_incomplete_or_mismatched_logs(self):
        for broken in [
            self.valid_log.rsplit('profile,island.generate,200', 1)[0],
            self.valid_log + 'profile,unfinished,10\n',
            self.valid_log.replace('generate,200', 'generate,500'),
        ]:
            with self.subTest(log=broken):
                self.log.write_text(broken)
                with self.assertRaises(ValueError):
                    self.summarize()

    def test_rejects_invalid_durations(self):
        for duration in ['nan', 'inf', '-1']:
            with self.subTest(duration=duration):
                self.log.write_text(self.valid_log.replace('parent,160', f'parent,{duration}'))
                with self.assertRaises(ValueError):
                    self.summarize()

    def test_rejects_unstable_geometry(self):
        self.csv.write_text(self.csv.read_text().replace('17,2,2,200,3,1,1,42', '17,2,2,200,3,1,1,43'))
        with self.assertRaises(ValueError):
            self.summarize()


if __name__ == '__main__':
    unittest.main()
