"""Exercise the real JNI transport on a headless desktop JVM, without an Android UI.

This supplements core tests. It does not claim Android ART, IME host, device timing,
screen-reader, or injected-clock device acceptance. Uses the local JDK and DLL only.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def run(args):
    repo = Path(__file__).resolve().parents[2]
    cases_path = repo / "docs/validation/special-input/cases.json"
    cases = {case["id"]: case for case in json.loads(cases_path.read_text(encoding="utf-8"))["cases"]}
    output = Path(args.output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    classes = output.parent / "jni-classes"
    classes.mkdir(exist_ok=True)
    subprocess.run([str(Path(args.jdk) / "bin/javac.exe"), "-encoding", "UTF-8", "-d", str(classes),
                    str(repo / "scripts/validation/jni/org/mywubi/NativeBridge.java")], check=True)
    checks = []
    requests = 0
    with tempfile.TemporaryDirectory(prefix="mywubi-si-jni-") as directory, \
            (output.parent / "jni.stderr.txt").open("w", encoding="utf-8") as stderr:
        root = Path(directory)
        process = subprocess.Popen([str(Path(args.jdk) / "bin/java.exe"), "-cp", str(classes),
                                    "org.mywubi.NativeBridge", str(Path(args.library).resolve())],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr,
                                   text=True, encoding="utf-8")
        handles = []
        ownership = {}

        def line(request):
            nonlocal requests
            requests += 1
            process.stdin.write(request + "\n")
            process.stdin.flush()
            reply = process.stdout.readline()
            if not reply:
                raise RuntimeError("JNI process ended; inspect jni.stderr.txt")
            return json.loads(reply)

        def raw(op, **fields):
            return line(json.dumps({"version": 1, "request": {"op": op, **fields}}, ensure_ascii=False))

        def call(op, **fields):
            reply = raw(op, **fields)
            assert reply["ok"], (op, reply)
            return reply.get("value")

        def keep(handle):
            handles.append(handle)
            return handle

        def rules(rows):
            return [{"code": row.get("code", "jjad"), "position": row["position"], "kind": "date_time",
                     "format": row.get("format", "'" + row.get("text", "日期").replace("'", "''") + "'"),
                     "enabled": row.get("enabled", True)} for row in rows]

        def setup(name, words, capacity, rows, special=None, auto=False):
            folder = root / name
            folder.mkdir()
            path = folder / "config.toml"
            path.write_text("", encoding="utf-8")
            (folder / "main.dict").write_text("".join(f"jjad\t{word}\t{100-i}\n" for i, word in enumerate(words)), encoding="utf-8")
            read = call("ReadConfig", path=str(path)); keep(read["version"])
            config = read["config"]
            config["dictionary"].update(system_table="main.dict", user_table="user.dict")
            config["basic"].update(candidate_count=capacity, auto_commit_unique=auto)
            config["shortcuts"] = {"rules": rules(rows)}
            if special:
                document = {"kind": "symbols", "name": "SI fixture", "version": "1",
                            "sources": [{"id": "test", "url": "local:SI", "revision": "1", "license": "MIT", "notice": "Synthetic fixture"}],
                            "entries": [{"text": special, "name": "样例", "triggers": ["甲"], "aliases": [], "tags": [], "order": 0, "sources": ["test"]}]}
                (folder / "symbols.mwe").write_text("# MyWubi Expressions 1\n" + json.dumps(document, ensure_ascii=False), encoding="utf-8")
                config["expressions"]["symbols"].update(append=True, resource="symbols.mwe")
            saved = call("SaveConfig", path=str(path), config=config, expected=read["version"]); keep(saved["version"])
            return path, saved["config"]

        def session(path):
            runtime = keep(call("BuildRuntime", path=str(path))["handle"])
            context = dict(editor=1, epoch=1, supports_preedit=True, supports_insert=True, owns_preedit=False,
                           purpose="Text", selection_available=True, surrounding_available=False, enter_action="Newline")
            result = keep(call("CreateSession", runtime=runtime, context=context)["handle"])
            ownership[result] = False
            return result

        def ack(s, plan, outcome="Applied"):
            op = plan["op"]
            if outcome == "Applied" and op != "NoOp":
                ownership[s] = "CompositionUpdate" in op or "CommitThenUpdate" in op
            return call("Ack", session=s, token=plan["token"], outcome=outcome, owns_preedit=ownership[s])

        def action(s, value):
            plan = call("Prepare", session=s, action=value)
            ack(s, plan)
            return plan

        def enter(s, code="jjad"):
            for character in code:
                plan = action(s, {"InputCode": character})
                assert "CommitAndReplace" not in plan["op"] and "CommitThenUpdate" not in plan["op"], plan

        def window(s):
            return call("Candidates", session=s, offset=0, count=256)

        def texts(s):
            return [row["text"] for row in window(s)["candidates"]]

        def pages(s, expected_count):
            result = []
            for index in range(expected_count):
                if index:
                    action(s, {"MoveCandidatePage": True})
                result.append([item["text"] for item in call("State", session=s)["view"]["items"]])
            return result

        try:
            line("@locale=th-TH"); line("@zone=Pacific/Kiritimati")
            c01 = cases["SI-C01"]
            base = ["甲", "乙", "丙", "丁", "戊", "己"]
            for hit in [False, True]:
                path, _ = setup(f"c01-{hit}", base, c01["capacity"], c01["rules"] if hit else [], "表")
                s = session(path); enter(s)
                expected = c01["expected_pages"] if hit else c01["without_match_pages"]
                assert pages(s, len(expected)) == expected
            checks.append("SI-C01: matched and unmatched page boundaries")
            for variant in cases["SI-C02"]["variants"]:
                path, _ = setup(f"c02-{variant['position']}", base, 3, [variant], "表")
                s = session(path); enter(s)
                assert pages(s, len(variant["expected_pages"])) == variant["expected_pages"]
            checks.append("SI-C02: insertion boundaries")
            for index, variant in enumerate(cases["SI-C03"]["variants"]):
                rows = [{"position": position, "text": text} for position, text in zip(variant["positions"], ["日期甲", "日期乙"])]
                path, _ = setup(f"c03-{index}", variant["base"], 3, rows)
                s = session(path); enter(s)
                assert texts(s) == variant["expected"]
            checks.append("SI-C03: sparse positions")
            for name in ["SI-C04", "SI-C05"]:
                case = cases[name]
                ordinary = [row["text"] for row in case["base"] if row["source"] == "ordinary"]
                special = next((row["text"] for row in case["base"] if row["source"] == "special"), None)
                path, _ = setup(name, ordinary, 3, case["rules"], special)
                s = session(path); enter(s)
                candidates = window(s)["candidates"]
                assert [row["text"] for row in candidates] == case["expected_texts"]
                sources = []
                for row in candidates:
                    dynamic = [hint["DateTime"] for hint in row["hints"] if isinstance(hint, dict) and "DateTime" in hint]
                    sources.append("dynamic:" + case["rules"][dynamic[0]]["id"] if dynamic else "ordinary")
                assert sources == case["expected_sources"]
                assert len({row["id"] for row in candidates}) == len(candidates)
                for index, source in enumerate(sources):
                    selected = session(path); enter(selected); view = window(selected)
                    plan = action(selected, {"SelectCandidate": [view["query_revision"], view["candidates"][index]["id"]]})
                    assert plan["learnable"] == source.startswith("ordinary")
                checks.append(name + ": same-text identities and learning flags")
            c06 = cases["SI-C06"]
            for capacity, expected in c06["expected_page_slot"].items():
                for position, (page, slot) in zip(c06["global_positions"], expected):
                    path, _ = setup(f"c06-{capacity}-{position}", [f"普通{i}" for i in range(12)], int(capacity), [{"position": position}])
                    for click in [False, True]:
                        s = session(path); enter(s)
                        zero_session = session(path); enter(zero_session)
                        zero = action(zero_session, {"InputText": ["0", "Normal"]})
                        # Buffered punctuation policy commits the first ordinary word + raw 0;
                        # it must not interpret 0 as candidate ten in this dynamic query.
                        assert zero["op"]["CommitAndReplace"]["text"] == "普通00", zero
                        assert zero["learnable"]
                        for _ in range(page - 1): action(s, {"MoveCandidatePage": True})
                        view = window(s)
                        selection = {"SelectCandidate": [view["query_revision"], view["candidates"][position-1]["id"]]} if click else {"InputText": [str(slot), "Normal"]}
                        plan = action(s, selection)
                        assert plan["op"]["CommitAndReplace"]["text"] == "日期"
                        assert not plan["learnable"]
            checks.append("SI-C06: 1/9/10 capacity, global 9/10/11 numeric/click identity")
            path, config = setup("c07", [], 3, [{"code": "JJAD", "position": 2}])
            assert config["shortcuts"]["rules"][0]["code"] == cases["SI-C07"]["normalize"]["JJAD"]
            read = call("ReadConfig", path=str(path)); keep(read["version"])
            original = path.read_bytes()
            for code in cases["SI-C07"]["invalid_codes"]:
                config["shortcuts"]["rules"][0]["code"] = code
                assert not raw("SaveConfig", path=str(path), config=config, expected=read["version"])["ok"]
                assert path.read_bytes() == original
            table = call("ReadTableConfig", path=str(path.parent / "main.dict"))
            rows = rules([{"code": "JJAD", "position": 2}, {"code": "abcd", "position": 2}])
            for key in cases["SI-C07"]["conflict_keys"]:
                table["wildcard_key"] = key
                inspected = call("InspectShortcuts", rules=rows, table=table)
                assert not inspected["states"][0]["effective"] and inspected["states"][1]["effective"]
                assert inspected["rules"][0]["enabled"]
            table["wildcard_key"] = ""
            assert call("InspectShortcuts", rules=rows, table=table)["states"][0]["effective"]
            for enabled in [True, False]:
                duplicate = rules([{"code": "JJAD", "position": 2}, {"code": "jjad", "position": 2, "enabled": enabled}])
                assert not raw("InspectShortcuts", rules=duplicate, table=table)["ok"]
            checks.append("SI-C07: normalized/disabled duplicates, invalid save, pause and restore intent")
            # Actual Java GregorianCalendar under Thai locale, not a fixed test clock.
            path, _ = setup("clock", [], 3, [{"position": 1, "format": "yyyy-MM-dd HH:mm:ss"}])
            line("@zone=Etc/GMT+12")
            s = session(path); enter(s, "jja")
            line("@zone=Pacific/Kiritimati")
            now = line("@time"); enter(s, "d"); frozen = texts(s)[0]
            assert frozen.startswith(f"{now['year']:04d}-{now['month']:02d}-{now['day']:02d}")
            view = window(s); choose = {"SelectCandidate": [view["query_revision"], view["candidates"][0]["id"]]}
            first = call("Prepare", session=s, action=choose); ack(s, first, "RejectedBeforeWrite")
            line("@zone=Etc/GMT+12")
            retry = call("Prepare", session=s, action=choose)
            assert retry["op"] == first["op"]; ack(s, retry, "RejectedBeforeWrite")
            assert texts(s)[0] == frozen
            action(s, "Backspace"); enter(s, "d")
            assert texts(s)[0][:10] != frozen[:10]
            assert not raw("Prepare", session=s, action=choose)["ok"]
            new_view = window(s)
            plan = call("Prepare", session=s, action={"SelectCandidate": [new_view["query_revision"], new_view["candidates"][0]["id"]]})
            ack(s, plan, "OutcomeUnknown")
            cleanup = call("Recover", session=s)
            assert "CommitAndReplace" not in cleanup["op"] and "CommitThenUpdate" not in cleanup["op"]
            ack(s, cleanup)
            enter(s)
            before_reload = window(s)
            runtime = keep(call("BuildRuntime", path=str(path))["handle"])
            cleanup = call("ApplyRuntime", session=s, runtime=runtime)
            if cleanup: ack(s, cleanup)
            enter(s)
            assert not raw("Prepare", session=s, action={"SelectCandidate": [before_reload["query_revision"], before_reload["candidates"][0]["id"]]})["ok"]
            checks.append("SI-C08/C09 supplemental: actual Gregorian JNI clock, frozen zone/retry and old ID; exact injected-time contract remains in core tests")
            path, config = setup("c10", [], 3, [])
            config["expressions"]["kana"].update(search=True, resource=str(repo / "expressions/kana.mwe"))
            read = call("ReadConfig", path=str(path)); keep(read["version"])
            keep(call("SaveConfig", path=str(path), config=config, expected=read["version"])["version"])
            for query in cases["SI-C10"]["queries"]:
                s = session(path)
                action(s, {"BeginTemporary": "Kana"})
                action(s, {"SetExpressionInput": "Literal"})
                for character in query:
                    assert action(s, {"InputText": [character, "Normal"]})["op"] == "NoOp"
                assert action(s, {"InputText": ["1", "Normal"]})["op"] == "NoOp"
                assert call("State", session=s)["view"]["expression"]["text"] == query + "1"
                assert action(s, "Backspace")["op"] == "NoOp"
                assert not raw("Prepare", session=s, action={"SetExpressionInput": "Words"})["ok"]
                assert action(s, "FindExpressions")["op"] == "NoOp"
                assert texts(s)[:2] == cases["SI-C10"]["expected_contains"]
                plan = action(s, {"InputText": ["1", "Normal"]})
                assert plan["op"]["CommitAndReplace"]["text"] == cases["SI-C10"]["host_commit"]
            checks.append("SI-C10: literal shi/SHI through JNI, no query host edits")
            path, config = setup("c11", [], 3, [{"code": cases["SI-C11"]["shortcut"], "position": 1}], auto=True)
            (path.parent / "main.dict").write_text("jjaa\t普通\n", encoding="utf-8")
            s = session(path); enter(s, cases["SI-C11"]["shortcut"])
            assert texts(s)[0] == "日期"
            path, _ = setup("c11-overlap", ["普通"], 3,
                            [{"code": code, "position": 1} for code in cases["SI-C11"]["overlap"]], auto=True)
            s = session(path); enter(s, cases["SI-C11"]["overlap"][1])
            assert texts(s)[0] == "日期"
            checks.append("SI-C11: reachable long prefix prevents automatic commit")
        finally:
            try:
                for handle in reversed(handles): call("Destroy", handle=handle)
            finally:
                process.stdin.close()
                process.wait(timeout=20)
                process.stdout.close()
        assert process.returncode == 0
    output.write_text(json.dumps({"scope": "Desktop Windows JVM JNI only; no Android device or actual host", "passed": checks,
                                 "requests": requests, "cases_sha256": hashlib.sha256(cases_path.read_bytes()).hexdigest(),
                                 "library_sha256": hashlib.sha256(Path(args.library).read_bytes()).hexdigest()}, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"Passed {len(checks)} JNI groups; {requests} protocol/control requests; {output}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--jdk", required=True)
    parser.add_argument("--library", required=True)
    parser.add_argument("--output", required=True)
    run(parser.parse_args())
