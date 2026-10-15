"""DEP-235 CPU compatibility contract: build flags, manifest, and all host CPUs agree."""
from pathlib import Path
import pytest
from test_release import load

cpu = load("cpu")
REPO = Path(__file__).resolve().parents[2]


def dep_235_cpu_common_v1_v2():
    def compiler(argv, **_kwargs):
        assert argv[-4:] == ["-C", "target-cpu=neoverse-v1", "-C", "target-feature=-sm4,-rand,-dit"]
        return b'target_feature="neon"\ntarget_feature="aes"\ntarget_feature="sha3"\ntarget_feature="lse"\n'
    manifest = cpu.envelope(REPO, compiler)
    available = "Features : fp asimd aes pmull sha3 sha512 atomics\n"
    assert cpu.compatible(manifest, available * 2)
    assert not cpu.compatible(manifest, available + "Features : fp asimd\n")
    assert not cpu.compatible(manifest, available.replace("atomics", ""))
    assert not cpu.compatible(manifest, "")
    for unsupported in ("sm4", "sve2-sm4", "future-unknown"):
        with pytest.raises(ValueError, match="unmapped"):
            cpu.envelope(REPO, lambda *a, **kw: f'target_feature="{unsupported}"\n'.encode())
