"""End-to-end verification of the 1.3 test build.

Starts the packaged runtime, drives it over the settings pipe, exercises the new
1.3 contract (collapsed two-line settings, todo store, chime settings) and runs
the settings window's own self-check. Writes a JSON report and exits non-zero on
the first failed assertion.

Usage:
    python tests/verify13.py <runtime-dir> [--report <path>] [--artifacts <dir>]
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import time
import uuid

PIPE_PREFIX = "LyricIsland.Settings.test.verify13."


def request(pipe, command, value=None, timeout=15.0):
    """One request/response round trip. The server accepts a fresh connection per call."""
    payload = {"version": 1, "command": command}
    if value is not None:
        payload["value"] = value
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with open("\\\\.\\pipe\\" + pipe, "r+b", buffering=0) as handle:
                handle.write((json.dumps(payload) + "\n").encode("utf8"))
                response = b""
                while not response.endswith(b"\n"):
                    chunk = handle.read(1)
                    if not chunk:
                        raise OSError("pipe closed")
                    response += chunk
                return json.loads(response)
        except OSError:
            time.sleep(0.15)
    raise RuntimeError("no response for " + command)


class Runtime:
    """A packaged runtime, started against a throwaway data directory."""

    def __init__(self, directory, data, run_ms):
        self.directory = directory
        self.data = data
        self.run_ms = run_ms
        self.pipe = PIPE_PREFIX + uuid.uuid4().hex
        self.process = None

    def environment(self):
        return dict(os.environ, LYRICISLAND_TEST_PIPE=self.pipe)

    def start(self):
        self.process = subprocess.Popen(
            [str(self.directory / "LyricIsland.exe"), "--data-dir", str(self.data), "--run-ms", str(self.run_ms)],
            env=self.environment(),
        )
        state = request(self.pipe, "state")
        assert state["success"] is True, state
        return state

    def stop(self):
        if self.process is None:
            return
        if self.process.poll() is None:
            try:
                request(self.pipe, "exit", timeout=3.0)
            except Exception:
                pass
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.terminate()
                self.process.wait(timeout=10)
        self.process = None


def check_defaults(state):
    """Every 1.3 default, including the ones that must stay exactly as 1.2.2 left them."""
    settings = state["settings"]
    expected = {
        # New in 1.3.
        "AutoCompactWidth": False,
        "AutoCompactMaxWidth": 420.0,
        "SwipeActivities": False,
        "DeviceRemindersEnabled": False,
        "DeviceVolume": 55.0,
        "TodoIslandEnabled": True,
        "CompactTranslation": True,
        "ChineseTwoLine": False,
        "NotificationSoundEnabled": True,
        "TodoSoundEnabled": True,
        "NotificationSound": "Soft",
        "TodoSound": "Bell",
        "NotificationVolume": 35.0,
        "TodoVolume": 45.0,
        "NotificationSoundPath": "",
        "TodoSoundPath": "",
        # Carried over from 1.2.2 and must not have moved.
        "CoverBackground": True,
        "NotificationClickOpenApp": True,
        "MusicClickOpenApp": False,
        "NotificationsEnabled": False,
        "PauseNotifications": False,
        "NotificationWhitelistEnabled": False,
        "ClickAnywhereExpand": True,
        "OnlineSongInfo": False, # isolated verification opts out of network metadata
        "Translation": True,
        "Animation": True,
        "CompactWidth": 260.0,
        "FontSize": 12.5,
        "Floating": False,
    }
    for key, value in expected.items():
        assert settings[key] == value, (key, settings[key], value)
    return expected


def check_todos(runtime, data):
    """Create, validate, complete and delete a todo; then prove it persisted."""
    today = request(runtime.pipe, "state")["today"]

    saved = request(runtime.pipe, "todoSave", {"title": "喝水", "content": "休息一下", "date": today, "time": "09:00", "daily": True})
    assert saved["success"] is True, saved
    todo_id = saved["id"]

    listed = request(runtime.pipe, "todos")
    assert listed["success"] is True and len(listed["todos"]) == 1, listed
    assert listed["todos"][0]["title"] == "喝水" and listed["todos"][0]["daily"] is True
    assert listed["todos"][0]["completed"] is False

    # The store is the source of truth for validation, not the UI.
    for payload, why in [
        ({"title": "x", "date": "2026-02-29", "time": "09:00"}, "non-leap date"),
        ({"title": "x", "date": today, "time": "24:00"}, "out-of-range time"),
        ({"title": "  ", "date": today, "time": "09:00"}, "empty title"),
        ({"title": "x", "date": today, "time": "9:00"}, "unpadded time"),
    ]:
        rejected = request(runtime.pipe, "todoSave", payload)
        assert rejected["success"] is False, (why, rejected)

    assert request(runtime.pipe, "todoComplete", {"id": todo_id, "completed": True})["success"] is True
    listed = request(runtime.pipe, "todos")
    assert listed["todos"][0]["completed"] is True, listed
    assert request(runtime.pipe, "todoComplete", {"id": todo_id, "completed": False})["success"] is True
    assert request(runtime.pipe, "todos")["todos"][0]["completed"] is False

    assert request(runtime.pipe, "todoComplete", {"id": "missing", "completed": True})["success"] is False
    assert request(runtime.pipe, "todoDelete", {"id": "missing"})["success"] is False

    stored = json.loads((data / "todos.json").read_text(encoding="utf8"))
    assert isinstance(stored, list) and stored[0]["id"] == todo_id and stored[0]["daily"] is True, stored

    assert request(runtime.pipe, "todoDelete", {"id": todo_id})["success"] is True
    assert request(runtime.pipe, "todos")["todos"] == []
    return todo_id


def check_persistence(runtime, data):
    """A todo saved before a restart must still be there afterwards."""
    today = request(runtime.pipe, "state")["today"]
    request(runtime.pipe, "todoSave", {"title": "交稿", "content": "", "date": today, "time": "20:00", "daily": False})
    runtime.stop()
    restarted = Runtime(runtime.directory, data, runtime.run_ms)
    restarted.start()
    listed = request(restarted.pipe, "todos")
    assert len(listed["todos"]) == 1 and listed["todos"][0]["title"] == "交稿", listed
    return restarted


def check_settings_updates(runtime):
    """The new keys must go through the same validator as everything else."""
    accepted = request(runtime.pipe, "update", {"key": "AutoCompactWidth", "value": True})
    assert accepted["success"] is True, accepted
    assert request(runtime.pipe, "state")["settings"]["AutoCompactWidth"] is True

    for key, value in [("AutoCompactWidth", "yes"), ("NotificationSound", "Chime"), ("TodoVolume", 120.0), ("ChineseTwoLine", 1)]:
        rejected = request(runtime.pipe, "update", {"key": key, "value": value})
        assert rejected["success"] is False, (key, value, rejected)

    # A custom chime must be a real, readable PCM WAV.
    rejected = request(runtime.pipe, "update", {"key": "NotificationSoundPath", "value": str(runtime.directory / "CHANGELOG.md")})
    assert rejected["success"] is False, rejected

    preview = request(runtime.pipe, "soundPreview", {"todo": False})
    assert preview["success"] is True, preview
    assert request(runtime.pipe, "soundPreview", {"todo": True})["success"] is True

    # Put the toggle back so the report describes the shipped defaults.
    request(runtime.pipe, "update", {"key": "AutoCompactWidth", "value": False})


def check_adaptive_width(runtime):
    """Auto width must actually shrink the collapsed capsule, and off must restore the stored width.

    With no player running the capsule shows the placeholder line, which is short
    enough to collapse below the stored width — so the measured value proves the
    real glyph measurement path ran, not just that the setting was stored.
    """
    stored = request(runtime.pipe, "state")["settings"]["CompactWidth"]

    def settle():
        time.sleep(1.2)
        return request(runtime.pipe, "state")

    request(runtime.pipe, "update", {"key": "AutoCompactWidth", "value": False})
    state = settle()
    assert state["compactWidth"] == stored, state["compactWidth"]
    assert state["compactHeight"] == 40.0, state["compactHeight"]
    assert state["compactSecondary"] == "", state["compactSecondary"]

    request(runtime.pipe, "update", {"key": "AutoCompactWidth", "value": True})
    state = settle()
    assert 140.0 <= state["compactWidth"] < stored, state["compactWidth"]

    request(runtime.pipe, "update", {"key": "AutoCompactWidth", "value": False})
    state = settle()
    assert state["compactWidth"] == stored, state["compactWidth"]
    return {"stored": stored, "adaptive": True}


def check_settings_selfcheck(runtime, workspace):
    """The settings window walks all eleven pages and reports its own regressions."""
    directory = workspace / "settings-smoke"
    directory.mkdir(parents=True, exist_ok=True)
    environment = dict(os.environ, LYRICISLAND_TEST_PIPE=runtime.pipe, LYRICISLAND_NATIVE_DATA=str(runtime.data), LYRICISLAND_SMOKE_REGRESSIONS="1")
    completed = subprocess.run(
        [str(runtime.directory / "LyricIsland.Settings.exe"), "--smoke-ui", str(directory)],
        env=environment,
        timeout=120,
    )
    report = json.loads((directory / "report.json").read_text(encoding="utf8"))
    assert completed.returncode == 0 and report["success"] is True, (completed.returncode, report)
    assert report["pages"] == 11, report
    assert report["todoRegression"] is True, report
    assert report["whitelistExpandCollapse"] is True, report
    return report


def check_tools(runtime):
    def call(command, value=None):
        response=request(runtime.pipe,command,value)
        assert response["success"] is True, response
        return response
    for key,value in [("NotificationVolume",0),("TodoVolume",0),("DeviceVolume",0),("TodoRemindersEnabled",False)]:
        call("update",{"key":key,"value":value})
    call("update",{"key":"AutoCompactMaxWidth","value":180})
    assert request(runtime.pipe,"update",{"key":"AutoCompactMaxWidth","value":90})["success"] is False
    call("update",{"key":"AutoCompactWidth","value":True})
    time.sleep(1)
    assert call("state")["compactWidth"]<=180.1
    call("update",{"key":"AutoCompactWidth","value":False})
    ids=[]
    for item in [{"mode":"countdown","duration":60},{"mode":"stopwatch"},{"mode":"pomodoro","focus":60,"rest":5,"rounds":2}]:
        ids.append(call("timerCreate",item)["id"])
    time.sleep(.2)
    state=call("state")
    assert len(state["timers"])==3 and state["activeComponent"]=="timer:"+ids[2]
    call("timerAction",{"id":ids[0],"action":"toggle"})
    paused=next(t for t in call("state")["timers"] if t["id"]==ids[0])["elapsedMs"]
    time.sleep(.3)
    assert next(t for t in call("state")["timers"] if t["id"]==ids[0])["elapsedMs"]==paused
    call("timerAction",{"id":ids[0],"action":"reset"})
    assert next(t for t in call("state")["timers"] if t["id"]==ids[0])["clock"]=="01:00"
    call("timerAction",{"id":ids[0],"action":"toggle"})
    call("islandSelect","timer:"+ids[1])
    call("islandCycle",1)
    assert call("state")["activeComponent"]=="timer:"+ids[2]
    # An internal reminder survives unrelated settings updates, even with app notifications off.
    call("devicePreview")
    generation=call("state")["rendererGeneration"]
    for volume in range(0,50,5):call("update",{"key":"DeviceVolume","value":volume})
    assert call("state")["rendererGeneration"]==generation
    call("update",{"key":"DeviceVolume","value":0})
    assert call("state")["notificationVisible"] is True
    for invalid in [{"mode":"bad"},{"duration":0},{"mode":"pomodoro","rounds":17}]:
        assert request(runtime.pipe,"timerCreate",invalid)["success"] is False
    for timer_id in ids:call("timerAction",{"id":timer_id,"action":"delete"})
    assert call("state")["timers"]==[]
    todo=call("todoSave",{"title":"配色测试","date":"2099-01-01","time":"09:00","color":"#B7DBCB"})
    assert next(t for t in call("state")["todos"] if t["id"]==todo["id"])["color"]=="#B7DBCB"
    assert request(runtime.pipe,"todoSave",{"title":"无效颜色","date":"2099-01-01","time":"09:00","color":"white"})["success"] is False
    call("todoDelete",{"id":todo["id"]})
    # Compare elapsed time to a monotonic wall clock to detect per-frame rounding drift.
    timer_id=call("timerCreate",{"mode":"stopwatch"})["id"]
    before=call("state")["timers"][0]["elapsedMs"]; started=time.monotonic()
    time.sleep(5)
    after=call("state")["timers"][0]["elapsedMs"]; wall=(time.monotonic()-started)*1000
    assert abs(after-before-wall)<100, (after-before,wall)
    # A live session is persisted but resumes paused in the new process.
    runtime.stop();runtime.start()
    restored=next(t for t in call("state")["timers"] if t["id"]==timer_id)
    assert restored["running"] is False and restored["elapsedMs"]>=200
    call("timerAction",{"id":timer_id,"action":"delete"})
    return {"concurrentTimers":True,"pauseResetResume":True,"cycleWithSwipeDisabled":True,"adaptiveMaximum":True,"pastelColor":True,"pausedSessionRestore":True,"devicePreview":True,"realHardwareTested":False}


def check_bootstrap(share, workspace, version="1.3"):
    """The single-file launcher must extract the payload and report 1.3."""
    directory = workspace / "bootstrap"
    directory.mkdir(parents=True, exist_ok=True)
    completed = subprocess.run([str(share), "--verify-package", str(directory)], timeout=60)
    report = json.loads((directory / "verification.json").read_text(encoding="utf-8-sig"))
    assert completed.returncode == 0 and report["version"] == version, (completed.returncode, report)
    return report


RELEASE_FILES = [
    "DotIsle-1.3-Portable.zip",
    "DotIsle-1.3.exe",
    "DotIsle-1.3-Source.zip",
    "DotIsle-1.3-SHA256.txt",
]


def digests(directory, version):
    rows = {}
    for original in RELEASE_FILES:
        name = original.replace("1.3", version)
        path = directory / name
        if path.is_file():
            rows[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    return rows


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("runtime", type=Path, help="the packaged runtime directory, e.g. publish/LyricIsland-1.3")
    parser.add_argument("--report", type=Path, default=None)
    parser.add_argument("--artifacts", type=Path, default=None, help="directory holding the packaged release files")
    parser.add_argument("--version",default="1.3")
    options = parser.parse_args()

    runtime_directory = options.runtime.resolve()
    assert (runtime_directory / "LyricIsland.exe").is_file(), runtime_directory
    workspace = Path(tempfile.mkdtemp(prefix="LyricIsland.Verify13."))
    data = workspace / "Data"
    data.mkdir(parents=True, exist_ok=True)

    report = {"success": False, "version": options.version, "runtime": str(runtime_directory), "testOverrides":{"AllowedSources":["LyricIsland.Verify13"],"OnlineSongInfo":False}}
    runtime = Runtime(runtime_directory, data, 60000)
    try:
        (data / "settings.json").write_text(json.dumps({"AllowedSources":["LyricIsland.Verify13"],"OnlineSongInfo":False}),encoding="utf8")
        state = runtime.start()
        assert state["version"] == options.version, state["version"]
        report["nativeReportsVersion"] = True
        report["defaults"] = check_defaults(state)
        assert isinstance(state["todos"], list) and state["pendingPage"] is None
        report["todosInState"] = True

        check_todos(runtime, data)
        report["todoContract"] = True
        check_settings_updates(runtime)
        report["settingsValidation"] = True
        report["adaptiveWidth"] = check_adaptive_width(runtime)
        report["tools"] = check_tools(runtime)
        report["settingsSelfCheck"] = check_settings_selfcheck(runtime, workspace)

        runtime = check_persistence(runtime, data)
        report["todoPersistence"] = True
        runtime.stop()

        if options.artifacts:
            artifacts = options.artifacts.resolve()
            report["digests"] = digests(artifacts, options.version)
            share = artifacts / ("DotIsle-"+options.version+".exe")
            if share.is_file():
                report["bootstrap"] = check_bootstrap(share, workspace, options.version)
        report["success"] = True
    finally:
        runtime.stop()

    if options.report:
        options.report.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf8")
    print(json.dumps(report, ensure_ascii=False))
    return 0 if report["success"] else 1


if __name__ == "__main__":
    sys.exit(main())
