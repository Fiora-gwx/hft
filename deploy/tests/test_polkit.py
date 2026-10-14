import json
from pathlib import Path
import shutil
import subprocess
import pytest


def dep_079_guard_polkit():
    node = shutil.which("node")
    if not node:
        pytest.skip("node is required for the polkit JS evaluator")
    policy = Path(__file__).resolve().parents[1] / "polkit/50-hft-dev2-guard.rules"
    cases = []
    stops = ["core@prod", "core@sim", "reconciler", "verify", "replay-verify",
             "journal-archiver", "pipeline", "treasury"]
    for name in stops:
        cases.append(["hft-guard", "hft-" + name + ".service", "stop", True])
    cases.append(["hft-guard", "hft-core@sim.service", "start", True])
    for unit, verb in [("hft-core@prod.service", "start"), ("hft-verify.service", "start"),
                       ("hft-verify.timer", "start"), ("hft-core@sim.service", "restart"),
                       ("hft-core@sim.service", "kill"), ("ssh.service", "stop"),
                       ("trading-collector.service", "stop")]:
        cases.append(["hft-guard", unit, verb, False])
    for name in ["watchtower", "ops-bot", "kill-switch", "offbox-watchdog"]:
        cases.append(["hft-guard", "hft-" + name + ".service", "stop", False])
    cases.append(["hft-watch", "hft-core@sim.service", "stop", False])
    script = """
let rule;
const polkit = { Result: {YES: 'yes', NO: 'no', NOT_HANDLED: 'none'}, addRule: f => {rule = f;} };
""" + policy.read_text() + "\nconst cases = " + json.dumps(cases) + ";\n" + """
for (const [user, unit, verb, expected] of cases) {
  const action = {id:'org.freedesktop.systemd1.manage-units', lookup:k=>({unit,verb}[k])};
  if ((rule(action, {user}) === 'yes') !== expected) throw new Error(JSON.stringify([user,unit,verb]));
}
if (rule({id:'unrelated', lookup:()=>''}, {user:'hft-guard'}) === 'yes') throw new Error('wrong action');
"""
    subprocess.run([node, "-"], input=script, text=True, check=True, capture_output=True)
