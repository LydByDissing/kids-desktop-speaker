#!/usr/bin/env python3
"""Tests for dsp-import-rew.py (DSP_REQ_002, DEC-054)."""
import importlib.util
import io
import os
import shutil
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
spec = importlib.util.spec_from_file_location("dsp_import_rew", os.path.join(HERE, "dsp-import-rew.py"))
rew = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rew)

APO = os.path.join(HERE, "testdata/rew-apo-example.txt")
GENERIC = os.path.join(HERE, "testdata/rew-generic-example.txt")


def read(path):
    with open(path) as f:
        return f.read()


def limits(**changes):
    args = rew.arguments(["x"])
    for k, v in changes.items():
        setattr(args, k, v)
    return args


def pk(freq, gain, q=2.0):
    return {"type": "Peaking", "freq": freq, "gain": gain, "q": q}


class Parse(unittest.TestCase):
    def test_equalizer_apo_export(self):
        filters, preamp = rew.parse(read(APO))
        self.assertEqual(preamp, -3.4)
        self.assertEqual([f["freq"] for f in filters], [120, 450, 2600, 8000], "the OFF filter is skipped")
        self.assertEqual(filters[0], pk(120, -6.5, 3.2))
        self.assertEqual(filters[3], {"type": "Highshelf", "freq": 8000, "gain": -2.0, "q": 0.7})

    def test_rew_generic_export(self):
        filters, preamp = rew.parse(read(GENERIC))
        self.assertIsNone(preamp)
        self.assertEqual(len(filters), 3, "None filters are skipped")
        self.assertEqual(filters[1], {"type": "Lowshelf", "freq": 180, "gain": 2.0, "q": rew.SHELF_Q},
                         "REW's LS has a fixed Q")

    def test_filters_that_cant_go_before_the_split(self):
        for line in ("Filter 1: ON HP Fc 80 Hz", "Filter 1: ON NO Fc 100 Hz", "Filter 1: ON LS 12dB Fc 100 Hz Gain 3 dB",
                     "Filter 1: ON PK Fc 100 Hz Gain -3 dB", "Filter 1: ON PK Fc 100 Hz Q 2"):
            with self.subTest(line), self.assertRaises(rew.Refused):
                rew.parse(line)


class Limits(unittest.TestCase):
    def test_the_examples_fit(self):
        for path in (APO, GENERIC):
            rew.check(rew.parse(read(path))[0], limits())

    def test_each_limit(self):
        cases = {
            "no filters": ([], {}),
            "too many": ([pk(100 * i, -1) for i in range(1, 12)], {}),
            "below the band": ([pk(40, -3)], {}),
            "above the band": ([pk(15000, -3)], {}),
            "too narrow": ([pk(200, -3, 20)], {}),
            "too wide": ([pk(200, -3, 0.1)], {}),
            "too much boost": ([pk(200, 7)], {}),
            "too deep a cut": ([pk(200, -20)], {}),
            "a stricter count": ([pk(200, -1), pk(400, -1)], {"max_filters": 1}),
        }
        for name, (filters, change) in cases.items():
            with self.subTest(name), self.assertRaises(rew.Refused):
                rew.check(filters, limits(**change))

    def test_boosts_that_add_up(self):
        # Each +4 dB is allowed, but on top of each other they make +8.
        with self.assertRaisesRegex(rew.Refused, "together the filters boost 8.0 dB"):
            rew.check([pk(300, 4, 1), pk(300, 4, 1)], limits())
        rew.check([pk(300, 4, 4), pk(3000, 4, 4)], limits())

    def test_the_response_is_the_design(self):
        self.assertAlmostEqual(rew.response_db([pk(1000, -6, 2)], 1000), -6, places=6)
        self.assertAlmostEqual(rew.response_db([pk(1000, -6, 2)], 50), 0, places=1)
        low = {"type": "Lowshelf", "freq": 200, "gain": 4, "q": 0.71}
        high = {"type": "Highshelf", "freq": 2000, "gain": -3, "q": 0.71}
        self.assertAlmostEqual(rew.response_db([low], 10), 4, places=1)
        self.assertAlmostEqual(rew.response_db([low], 10000), 0, places=1)
        self.assertAlmostEqual(rew.response_db([high], 20000), -3, places=1)
        self.assertAlmostEqual(rew.response_db([high], 50), 0, places=1)


class Import(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.config = os.path.join(self.dir, "camilladsp.yml")
        self.keep = os.path.join(self.dir, "eq/rew-export.txt")
        shutil.copyfile(os.path.join(REPO, "ci/device/camilladsp.yml"), self.config)

    def tearDown(self):
        shutil.rmtree(self.dir)

    def run_import(self, *args):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = rew.main([*args, "--config", self.config, "--keep", self.keep])
        return code, out.getvalue() + err.getvalue()

    def pipeline(self):
        import yaml
        with open(self.config) as f:
            return yaml.safe_load(f)

    def test_import_places_the_eq_before_the_split(self):
        code, out = self.run_import(APO)
        self.assertEqual(code, 0, out)
        self.assertIn("preamp of -3.4 dB is ignored", out)
        conf = self.pipeline()
        self.assertEqual(conf["pipeline"][0], {"type": "Mixer", "name": "to_mono"})
        self.assertEqual(conf["pipeline"][1], {"type": "Filter", "channels": [0], "names": ["eq_1", "eq_2", "eq_3", "eq_4"]})
        self.assertEqual(conf["pipeline"][2], {"type": "Mixer", "name": "split_bass"})
        self.assertEqual(conf["filters"]["eq_1"], {"type": "Biquad", "parameters": {"type": "Peaking", "freq": 120, "gain": -6.5, "q": 3.2}})
        self.assertEqual(read(self.keep), read(APO), "the export is kept")
        self.assertEqual(rew.imported(read(self.config)), rew.parse(read(APO))[0])

    def test_a_new_import_replaces_the_last(self):
        self.run_import(APO)
        self.run_import(GENERIC)
        conf = self.pipeline()
        self.assertEqual(conf["pipeline"][1]["names"], ["eq_1", "eq_2", "eq_3"])
        self.assertNotIn("eq_4", conf["filters"])
        self.assertEqual(read(self.config).count(rew.BEGIN), 2)

    def test_clear_leaves_the_markers_and_no_step(self):
        empty = rew.rewrite(read(self.config), [])
        self.run_import(APO)
        self.assertEqual(self.run_import("--clear")[0], 0)
        self.assertEqual(read(self.config), empty)
        self.assertEqual(rew.rewrite(empty, []), empty, "clearing twice changes nothing")
        self.assertEqual(self.pipeline()["pipeline"][1], {"type": "Mixer", "name": "split_bass"})

    def test_a_refused_export_changes_nothing(self):
        bad = os.path.join(self.dir, "bad.txt")
        with open(bad, "w") as f:
            f.write("Filter 1: ON PK Fc 300 Hz Gain 9 dB Q 2\n")
        before = read(self.config)
        code, out = self.run_import(bad)
        self.assertEqual(code, 1)
        self.assertIn("boosts 9 dB; at most 6", out)
        self.assertEqual(read(self.config), before)
        self.assertFalse(os.path.exists(self.keep))

    def test_check_only(self):
        before = read(self.config)
        code, out = self.run_import("--check", GENERIC)
        self.assertEqual(code, 0, out)
        self.assertIn("Within the limits", out)
        self.assertEqual(read(self.config), before)

    def test_a_path_from_the_repo_root(self):
        # make -C ci runs the import in ci/, with FILE as typed at the root.
        cwd = os.getcwd()
        os.chdir(os.path.join(REPO, "ci"))
        try:
            code, out = self.run_import("--relative-to", "..", "ci/scripts/testdata/rew-generic-example.txt")
        finally:
            os.chdir(cwd)
        self.assertEqual(code, 0, out)
        self.assertEqual(read(self.keep), read(GENERIC))

    def test_the_limits_are_options(self):
        code, out = self.run_import("--max-filters", "2", APO)
        self.assertEqual(code, 1)
        self.assertIn("4 filters; at most 2", out)


if __name__ == "__main__":
    unittest.main()
