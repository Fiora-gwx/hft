#!/usr/bin/python3
"""dev2 resource guard. All mutable state belongs to this process.

The sampler is read-only; the effect boundary can only stop the fixed hft
allowlist, or start the simulation instance. No legacy unit is ever controlled.
"""
import collections
import datetime
import json
import math
import os
from pathlib import Path
import subprocess
import time
import tomllib

BATCH = (
    "hft-verify.service", "hft-replay-verify.service",
    "hft-journal-archiver.service", "hft-pipeline.service", "hft-treasury.service",
)
CORE = ("hft-core@prod.service", "hft-reconciler.service", "hft-core@sim.service")
STOP_UNITS = BATCH + CORE
START_UNITS = ("hft-core@sim.service",)
MIB = 1024 * 1024


def load_config(path):
    with open(path, "rb") as stream:
        cfg = tomllib.load(stream)
    expected = {"mode", "sample_s", "level_two_s", "recover_s", "cpu_cores",
                "memory_mib", "state_dir", "legacy_cgroup", "legacy_state", "signals"}
    if set(cfg) != expected or cfg["mode"] not in {"shared", "dedicated"}:
        raise ValueError("invalid guard configuration keys/mode")
    for key in ("sample_s", "level_two_s", "recover_s", "cpu_cores", "memory_mib"):
        if isinstance(cfg[key], bool) or not isinstance(cfg[key], (int, float)) or not math.isfinite(cfg[key]) or cfg[key] <= 0:
            raise ValueError("invalid guard threshold")
    allowed = {"mem_available_mib", "legacy_memory", "legacy_cpu", "swap_in",
               "host_memory", "host_cpu", "swap_growth", "legacy_unhealthy"}
    stops = set()
    for name, signal in cfg["signals"].items():
        if name not in allowed or set(signal) - {"kind", "threshold", "seconds", "samples", "below", "level"}:
            raise ValueError("invalid guard signal")
        if signal.get("kind") not in {"stop", "alert"} or "threshold" not in signal:
            raise ValueError("invalid guard signal kind/threshold")
        for key in ("threshold", "seconds", "samples"):
            value = signal.get(key, 0)
            if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
                raise ValueError("invalid guard signal duration")
        if signal.get("kind") == "stop":
            if "level" in signal or name not in {"mem_available_mib", "legacy_memory", "legacy_cpu", "swap_in"}:
                raise ValueError("alert-only signal cannot stop hft")
            stops.add(name)
        elif signal.get("level") not in {"P1", "P2"}:
            raise ValueError("invalid alert level")
    required = {"mem_available_mib"}
    if cfg["mode"] == "shared":
        required |= {"legacy_memory", "legacy_cpu", "swap_in"}
    if stops != required:
        raise ValueError("missing or inappropriate stop signals")
    return cfg


class Effects:
    def __init__(self, state_dir, dry_run=False):
        self.root = Path(state_dir)
        self.dry_run = dry_run

    def control(self, verb, unit):
        allowed = STOP_UNITS if verb == "stop" else START_UNITS if verb == "start" else ()
        if unit not in allowed:
            raise ValueError("unit action outside guard allowlist")
        if self.dry_run:
            print(json.dumps({"verb": verb, "unit": unit}))
            return
        subprocess.run(["systemctl", verb, unit], check=True, timeout=25)

    def active(self, unit):
        if unit not in STOP_UNITS:
            raise ValueError("unit outside guard allowlist")
        result = subprocess.run(["systemctl", "show", unit, "--property=ActiveState", "--value"],
                                check=True, capture_output=True, text=True, timeout=5)
        return result.stdout.strip() in {"active", "activating", "deactivating", "reloading"}

    def sim_stamp(self):
        result = subprocess.run(["systemctl", "show", START_UNITS[0],
                                 "--property=StateChangeTimestampMonotonic", "--value"],
                                check=True, capture_output=True, text=True, timeout=5)
        return result.stdout.strip()

    def trip(self, event):
        if self.dry_run:
            return
        self.root.mkdir(parents=True, exist_ok=True, mode=0o750)
        try:
            fd = os.open(self.root / "tripped", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
        except FileExistsError:
            return
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            os.fchmod(stream.fileno(), 0o644) if hasattr(os, "fchmod") else None
            json.dump(event, stream, ensure_ascii=False)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        self.sync_dir()

    def sync_dir(self):
        if os.name == "posix":
            fd = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.fsync(fd)
            finally:
                os.close(fd)

    def event(self, event):
        line = json.dumps(event, ensure_ascii=False)
        if self.dry_run:
            print(line)
            return
        self.root.mkdir(parents=True, exist_ok=True, mode=0o750)
        with open(self.root / "events.jsonl", "a", encoding="utf-8") as stream:
            stream.write(line + "\n")
            stream.flush()
            os.fsync(stream.fileno())

    def alert(self, level, text):
        # Journald is the durable alert transport; watchtower tails events.jsonl.
        print(f"hft-dev2-guard: {level} {text}", flush=True)

    def clear(self, utc):
        if not self.dry_run:
            (self.root / "tripped").unlink(missing_ok=True)
            self.sync_dir()
        self.event({"utc": utc, "action": "清除标志", "signals": {}, "usage": {}})


class Guard:
    def __init__(self, cfg, effects):
        self.cfg = cfg
        self.effects = effects
        self.history = collections.deque()
        self.since = {}
        self.count = collections.Counter()
        self.reported = set()
        self.level = 0
        self.level_one_at = None
        self.healthy_since = None
        self.sim_stop_stamp = None
        self.restart_eligible = False
        self.restart_days = set()
        # Preserve the daily limit across daemon restarts. Never clear tripped here.
        events = effects.root / "events.jsonl"
        if events.exists():
            with events.open(encoding="utf-8") as stream:
                for line in stream:
                    event = json.loads(line)
                    if event.get("action") == "自动重启 sim":
                        self.restart_days.add(event["utc"][:10])

    def usage(self, sample, start):
        base = next((s for s in self.history if s["at"] >= start), self.history[0])
        dt = sample["at"] - base["at"]
        result = {}
        for name in ("hft", "batch"):
            cpu = max(0, sample[name + "_cpu_us"] - base[name + "_cpu_us"]) / (dt * 1e6) if dt > 0 else 0
            result[name] = {"cpu": cpu, "memory": sample[name + "_memory"]}
        return result

    def causal(self, usage, core_only=False):
        if self.cfg["mode"] == "dedicated":
            return True
        return any(row["cpu"] > self.cfg["cpu_cores"] or row["memory"] > self.cfg["memory_mib"] * MIB
                   for name, row in usage.items() if not core_only or name == "hft")

    def emit(self, sample, signals, action, level, usage, text=None):
        event = {"utc": sample["utc"], "signals": {s: sample[s] for s in signals},
                 "usage": usage, "action": action, "level": level}
        self.effects.event(event)
        self.effects.alert(level, text or action)
        return event

    def step(self, sample):
        now = sample["at"]
        if self.history and now <= self.history[-1]["at"]:
            raise ValueError("guard sample clock must increase")
        self.history.append(sample)
        keep = max([300, self.cfg["level_two_s"]] + [s.get("seconds", 0) for s in self.cfg["signals"].values()])
        while len(self.history) > 2 and self.history[1]["at"] < now - keep:
            self.history.popleft()
        active = []
        stop_violated = False
        for name, rule in self.cfg["signals"].items():
            value = sample[name]
            violated = value < rule["threshold"] if rule.get("below", False) else value > rule["threshold"]
            if rule["kind"] == "stop":
                stop_violated |= violated
            if not violated:
                self.since.pop(name, None)
                self.count[name] = 0
                self.reported.discard(name)
                continue
            self.since.setdefault(name, now)
            self.count[name] += 1
            if self.count[name] < rule.get("samples", 1) or now - self.since[name] < rule.get("seconds", 0):
                continue
            if rule["kind"] == "stop":
                active.append(name)
            elif name not in self.reported:
                self.emit(sample, [name], "未动作", rule["level"], self.usage(sample, self.since[name]), "hft 未动作: " + name)
                self.reported.add(name)
        if active:
            usage = self.usage(sample, min(self.since[s] for s in active))
            if self.level == 0:
                if self.causal(usage):
                    event = {"utc": sample["utc"], "signals": {s: sample[s] for s in active}, "level": 1}
                    self.effects.trip(event)  # durable before the first systemctl call
                    self.level = 1
                    self.level_one_at = now
                    self.emit(sample, active, "一级", "P1", usage)
                    for unit in BATCH:
                        if self.effects.active(unit):
                            self.effects.control("stop", unit)
                elif "causality" not in self.reported:
                    self.emit(sample, active, "未动作", "P1", usage, "hft 用量不明显,未动作")
                    self.reported.add("causality")
            elif self.level == 1 and now - self.level_one_at >= self.cfg["level_two_s"]:
                usage = self.usage(sample, self.level_one_at)
                if self.causal(usage, core_only=True):
                    self.emit(sample, active, "二级", "P1", usage)
                    stopped_sim = self.effects.active(START_UNITS[0])
                    for unit in CORE:
                        if self.effects.active(unit):
                            self.effects.control("stop", unit)
                    self.sim_stop_stamp = self.effects.sim_stamp()
                    self.restart_eligible = stopped_sim and sample["utc"][:10] not in self.restart_days
                    self.level = 2
                elif "level_two_causality" not in self.reported:
                    self.emit(sample, active, "未动作", "P1", usage, "hft 用量不明显,未动作")
                    self.reported.add("level_two_causality")
        if stop_violated:
            self.healthy_since = None
        else:
            self.healthy_since = now if self.healthy_since is None else self.healthy_since
            self.reported.discard("causality")
            if self.level == 1:
                self.level = 0
                self.level_one_at = None
            if self.level == 2 and now - self.healthy_since >= self.cfg["recover_s"]:
                day = sample["utc"][:10]
                if (self.restart_eligible and day not in self.restart_days
                        and self.effects.sim_stamp() == self.sim_stop_stamp
                        and not self.effects.active(START_UNITS[0])):
                    # Persist the daily reservation before start, even if systemctl fails.
                    self.emit(sample, [], "自动重启 sim", "P1", self.usage(sample, now))
                    self.restart_days.add(day)
                    self.restart_eligible = False
                    self.effects.control("start", START_UNITS[0])
                    self.level = 0


def fields(path):
    return dict(line.split()[:2] for line in path.read_text().splitlines())


def pressure(path):
    line = next(line for line in path.read_text().splitlines() if line.startswith("some "))
    return float(dict(item.split("=") for item in line.split()[1:])["avg10"])


class Sampler:
    def __init__(self, cfg, proc=Path("/proc"), cgroup=Path("/sys/fs/cgroup")):
        self.cfg, self.proc, self.cgroup = cfg, proc, cgroup
        self.previous = None
        self.swap = collections.deque()

    def sample(self):
        now = time.monotonic()
        mem = fields(self.proc / "meminfo")
        vm = fields(self.proc / "vmstat")
        used = (int(mem["SwapTotal:"]) - int(mem["SwapFree:"])) * 1024
        self.swap.append((now, used))
        while len(self.swap) > 1 and self.swap[1][0] <= now - 300:
            self.swap.popleft()
        result = {"at": now, "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  "mem_available_mib": int(mem["MemAvailable:"]) / 1024,
                  "host_memory": pressure(self.proc / "pressure/memory"),
                  "host_cpu": pressure(self.proc / "pressure/cpu"),
                  "swap_growth": max(0, used - self.swap[0][1]) / MIB}
        count = int(vm["pswpin"])
        result["swap_in"] = max(0, count - self.previous[1]) * 60 / (now - self.previous[0]) if self.previous else 0
        self.previous = (now, count)
        for name, directory in (("hft", "hft.slice"), ("batch", "hftbatch.slice")):
            root = self.cgroup / directory
            result[name + "_cpu_us"] = int(fields(root / "cpu.stat")["usage_usec"])
            result[name + "_memory"] = int((root / "memory.current").read_text())
        if self.cfg["mode"] == "shared":
            legacy = (self.cgroup / self.cfg["legacy_cgroup"]).resolve()
            if not legacy.is_relative_to(self.cgroup.resolve()):
                raise ValueError("legacy cgroup outside cgroup root")
            result["legacy_memory"] = pressure(legacy / "memory.pressure")
            result["legacy_cpu"] = pressure(legacy / "cpu.pressure")
            state = Path(self.cfg["legacy_state"])
            # The guard never invokes systemctl or opens a legacy unit file.
            result["legacy_unhealthy"] = int(not state.is_file() or time.time() - state.stat().st_mtime > 30)
        return result
