import importlib.util
import json
from pathlib import Path
import re
import pytest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("guard", ROOT / "dev2/guard.py")
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class FakeSystemctl(guard.Effects):
    def __init__(self, path):
        super().__init__(path)
        self.calls = []
        self.alerts = []
        self.running = set(guard.STOP_UNITS)
        self.stamp = "1000"
        self.at = 0

    def active(self, unit):
        return unit in self.running

    def sim_stamp(self):
        return self.stamp

    def control(self, verb, unit):
        assert (self.root / "tripped").exists(), "stop must follow durable trip marker"
        self.calls.append((verb, unit, self.at))
        if verb == "stop":
            self.running.discard(unit)
        else:
            self.running.add(unit)
        if unit == "hft-core@sim.service":
            self.stamp = str(int(self.stamp) + 1)

    def alert(self, level, text):
        self.alerts.append((level, text))


def rig(tmp_path):
    cfg = guard.load_config(ROOT / "dev2/dev2-guard.toml.tmpl")
    fx = FakeSystemctl(tmp_path)
    return guard.Guard(cfg, fx), fx


def sample(at, **changes):
    row = dict(at=at, utc="2026-10-15T00:00:00+00:00", mem_available_mib=1000,
               legacy_memory=0, legacy_cpu=0, swap_in=0, host_memory=0,
               host_cpu=0, swap_growth=0, legacy_unhealthy=0,
               hft_cpu_us=0, hft_memory=300 * guard.MIB, batch_cpu_us=0, batch_memory=0)
    row.update(changes)
    return row


def tick(g, fx, at, **changes):
    fx.at = at
    g.step(sample(at, **changes))


def events(fx):
    path = fx.root / "events.jsonl"
    return [json.loads(s) for s in path.read_text(encoding="utf-8").splitlines()] if path.exists() else []


def core_180_mem_available_signal(tmp_path):
    for index, (values, tripped) in enumerate([
        ([599, 599], False), ([599, 599, 599], True),
        ([600] * 4, False), ([599, 599, 650, 599, 599], False),
    ]):
        g, fx = rig(tmp_path / str(index))
        for i, value in enumerate(values):
            tick(g, fx, i * 5, mem_available_mib=value)
        assert (fx.root / "tripped").exists() == tripped
        assert bool(fx.calls) == tripped


def duration_case(tmp_path, name, threshold, duration):
    g, fx = rig(tmp_path / "boundary")
    for t in range(0, duration * 2 + 1, 5):
        tick(g, fx, t, **{name: threshold})
    assert not fx.calls
    g, fx = rig(tmp_path / "over")
    for t in range(0, duration, 5):
        tick(g, fx, t, **{name: threshold + 0.01})
    assert not fx.calls
    tick(g, fx, duration, **{name: threshold + 0.01})
    assert [u for _, u, _ in fx.calls] == list(guard.BATCH)


def core_181_legacy_memory_pressure(tmp_path):
    duration_case(tmp_path, "legacy_memory", 5, 30)


def core_182_legacy_cpu_pressure(tmp_path):
    duration_case(tmp_path, "legacy_cpu", 20, 60)


def core_183_swap_in_rate(tmp_path):
    duration_case(tmp_path, "swap_in", 1000, 120)


def core_184_causality(tmp_path):
    for i, (cpu, mem, batch_cpu, expected) in enumerate([
        (.25, 256 * guard.MIB, .25, False),
        (.25, 256 * guard.MIB + 1, 0, True),
        (0, 0, .2501, True), (.3, 0, 0, True),
    ]):
        g, fx = rig(tmp_path / str(i))
        for at in (0, 5, 10):
            tick(g, fx, at, mem_available_mib=500, hft_memory=mem,
                 batch_memory=0, hft_cpu_us=int(cpu * at * 1e6), batch_cpu_us=int(batch_cpu * at * 1e6))
        assert bool(fx.calls) == expected
        assert (fx.root / "tripped").exists() == expected
        if not expected:
            assert fx.alerts == [("P1", "hft 用量不明显,未动作")]
            assert events(fx)[0]["action"] == "未动作"


def core_185_alert_only_signals(tmp_path):
    for i, (name, value, duration, expected) in enumerate([
        ("host_memory", 10.01, 30, "P2"), ("host_memory", 10, 60, None),
        ("host_cpu", 40.01, 60, "P2"), ("swap_growth", 64.00001, 300, "P1"),
        ("swap_growth", 64, 300, None), ("legacy_unhealthy", 1, 30, "P1"),
    ]):
        g, fx = rig(tmp_path / str(i))
        for t in range(0, duration + 1, 5):
            tick(g, fx, t, **{name: value})
        assert [level for level, _ in fx.alerts] == ([expected] if expected else [])
        assert not fx.calls
        assert not (fx.root / "tripped").exists()


def core_186_level_one_order(tmp_path):
    g, fx = rig(tmp_path)
    for at in (-10, -5, 0):
        tick(g, fx, at, mem_available_mib=500)
    assert [u for _, u, _ in fx.calls] == list(guard.BATCH)
    marker = json.loads((tmp_path / "tripped").read_text(encoding="utf-8"))
    assert marker["level"] == 1 and marker["signals"]["mem_available_mib"] == 500
    assert len(fx.alerts) == 1 and fx.alerts[0][0] == "P1"


def core_187_level_two(tmp_path):
    for core_memory, expected in ((300, True), (100, False)):
        g, fx = rig(tmp_path / str(core_memory))
        for at in (-10, -5, 0):
            tick(g, fx, at, mem_available_mib=500)
        for at in range(5, 30, 5):
            tick(g, fx, at, mem_available_mib=500, hft_memory=core_memory * guard.MIB)
        assert [u for _, u, _ in fx.calls] == list(guard.BATCH)
        tick(g, fx, 30, mem_available_mib=500, hft_memory=core_memory * guard.MIB)
        assert [u for _, u, _ in fx.calls] == list(guard.BATCH + (guard.CORE if expected else ()))
        assert all(level == "P1" for level, _ in fx.alerts)


def core_188_window_level_two(tmp_path):
    g, fx = rig(tmp_path)
    fx.running = set(guard.CORE)
    for at in range(-10, 31, 5):
        tick(g, fx, at, mem_available_mib=500)
        if at == 0:
            assert (tmp_path / "tripped").exists() and not fx.calls
    assert [u for _, u, _ in fx.calls] == list(guard.CORE)


def core_192_events_jsonl(tmp_path):
    g, fx = rig(tmp_path)
    for at in (0, 5, 10):
        tick(g, fx, at, mem_available_mib=500, hft_memory=0)
    for at in range(15, 46, 5):
        tick(g, fx, at, mem_available_mib=500)
    for at in range(50, 1851, 5):
        tick(g, fx, at)
    fx.clear("2026-10-15T01:00:00+00:00")
    rows = events(fx)
    assert [e["action"] for e in rows] == ["未动作", "一级", "二级", "自动重启 sim", "清除标志"]
    assert all({"utc", "signals", "usage", "action"} <= set(e) for e in rows)
    assert fx.calls[-1][:2] == ("start", "hft-core@sim.service")


def core_193_tripped_survives_restart(tmp_path):
    g, fx = rig(tmp_path)
    for at in (0, 5, 10):
        tick(g, fx, at, mem_available_mib=500)
    original = (tmp_path / "tripped").read_bytes()
    g = guard.Guard(g.cfg, fx)
    for at in range(0, 601, 5):
        tick(g, fx, at)
    assert (tmp_path / "tripped").read_bytes() == original
    fx.clear("2026-10-15T01:00:00+00:00")
    assert not (tmp_path / "tripped").exists()
    assert events(fx)[-1]["action"] == "清除标志"


def dep_080_whitelist_matches_polkit():
    policy = (ROOT / "polkit/50-hft-dev2-guard.rules").read_text()
    for verb, expected in (("stop", guard.STOP_UNITS), ("start", guard.START_UNITS)):
        block = re.search(r"var " + verb + r" = \[(.*?)\];", policy, re.S)[1]
        assert set(re.findall(r'"([^"]+)"', block)) == set(expected)
    assert all(name.startswith("hft-") for name in guard.STOP_UNITS)


def dep_085_level_two_latency(tmp_path):
    g, fx = rig(tmp_path)
    for at in range(-10, 41, 5):
        tick(g, fx, at, mem_available_mib=500)
    assert next(at for _, unit, at in fx.calls if unit == "hft-core@prod.service") == 30


def test_refuse_alert_to_stop_reclassification(tmp_path):
    source = (ROOT / "dev2/dev2-guard.toml.tmpl").read_text()
    path = tmp_path / "invalid.toml"
    path.write_text(source.replace('[signals.host_cpu]\nkind = "alert"', '[signals.host_cpu]\nkind = "stop"'))
    with pytest.raises(ValueError):
        guard.load_config(path)


def test_effect_boundary_rejects_legacy_or_restart(tmp_path):
    fx = guard.Effects(tmp_path, dry_run=True)
    for verb, unit in [("stop", "ssh.service"), ("restart", "hft-core@sim.service"),
                       ("start", "hft-core@prod.service"), ("stop", "hft-watchtower.service")]:
        with pytest.raises(ValueError):
            fx.control(verb, unit)


def test_guard_never_starts_a_sim_it_did_not_stop(tmp_path):
    g, fx = rig(tmp_path)
    fx.running.discard("hft-core@sim.service")
    for at in range(-10, 31, 5):
        tick(g, fx, at, mem_available_mib=500)
    for at in range(35, 1840, 5):
        tick(g, fx, at)
    assert not any(verb == "start" for verb, _, _ in fx.calls)
