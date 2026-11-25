import unittest
from release_contract import baselines, check_next, marker, platforms, required


def release(tag, names):
    return dict(tag_name=tag, draft=False, prerelease=False, assets=[{"name": n} for n in names])


class ReleaseContractTests(unittest.TestCase):
    def test_interleaved_history(self):
        history = [release("V1.0.0", ["releases.win.json", "android-update.json"])]
        with self.assertRaises(ValueError):
            check_next("V2.0.0", "windows", history)
        history[0]["assets"].append({"name": marker("both")})
        for n, scope, expected in [(2, "windows", (2, 1)), (3, "android", (2, 3)), (4, "both", (4, 4))]:
            tag = f"V{n}.0.0"
            check_next(tag, scope, history, 3, 2)
            history.append(release(tag, [marker(scope)]))
            self.assertEqual(baselines(history), dict(zip(("windows", "android"), [f"V{i}.0.0" for i in expected])))
        for tag, scope, code in [("V4.0.0", "both", 8), ("V1.1.0", "both", 8),
                                 ("V5.0.0", "other", 8), ("V5.0.0", "android", 2)]:
            with self.assertRaises(ValueError):
                check_next(tag, scope, history, code, 2)

    def test_asset_contract(self):
        win = required("windows", "V1.2.3")
        android = required("android", "V1.2.3")
        self.assertFalse(win & android)
        self.assertEqual(win | android, required("both", "V1.2.3"))
        self.assertEqual(platforms(win | android), ("windows", "android"))
        self.assertEqual(platforms([marker("android")]), ("android",))
        self.assertIn("android-update-full.json", android)
        with self.assertRaises(ValueError):
            platforms([marker("both"), marker("android")])
