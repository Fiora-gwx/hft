#!/usr/bin/python3
"""Root pre-start dispatcher; AWS remains the default. Never emits secret values."""
import os
from pathlib import Path
import subprocess
import sys
import tomllib

sys.dont_write_bytecode = True
from local_secrets import OUTPUT, entries


def main():
    config_path = Path("/etc/hft/hft-core-prod.toml")
    manifest_path = Path("/etc/hft/secrets/hft-core.toml")
    release = Path(__file__).resolve().parents[2]
    config = tomllib.loads(config_path.read_text(encoding="utf-8"))
    # Includes RDB1 is_prod rejection and exact demo endpoint checks, before any copy.
    subprocess.run([str(release / "bin/hft-core"), "--check", "--config", str(config_path)], check=True, capture_output=True)
    mode = config["instance"].get("secrets_mode", "aws_secrets_manager")
    if mode == "aws_secrets_manager":
        subprocess.run([str(release / "bin/hft-secrets"), "fetch", "--manifest", str(manifest_path), "--out", str(OUTPUT), "--owner", "hft", "--mode", "0400"], check=True, capture_output=True)
        return
    if mode != "local_demo":
        raise ValueError("unknown secret mode")
    sources = entries(config, tomllib.loads(manifest_path.read_text(encoding="utf-8")))
    import pwd
    import tempfile
    account = pwd.getpwnam("hft")
    output = Path(str(OUTPUT))
    output.mkdir(mode=0o700, exist_ok=True)
    if output.is_symlink() or output.resolve() != output:
        raise ValueError("runtime secret directory alias")
    os.chown(output, account.pw_uid, account.pw_gid)
    os.chmod(output, 0o700)
    written = []
    try:
        for destination, source in sources.items():
            # Root-owned source directories prevent replacement by the service account.
            fd = os.open(source, os.O_RDONLY | os.O_NOFOLLOW)
            with os.fdopen(fd, "rb") as inp, tempfile.NamedTemporaryFile(dir=output, delete=False) as out:
                written.append(Path(out.name))
                data = bytearray(inp.read(4097))
                try:
                    if not 0 < len(data) <= 4096:
                        raise ValueError("local secret size changed")
                    out.write(data)
                    out.flush()
                finally:
                    data[:] = bytes(len(data))
                os.fchmod(out.fileno(), 0o400)
                os.fchown(out.fileno(), account.pw_uid, account.pw_gid)
            os.replace(out.name, destination)
            written[-1] = Path(destination)
    except BaseException:
        for path in written:
            path.unlink(missing_ok=True)
        raise


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
        print("secret preparation failed; no credential values logged", file=sys.stderr)
        raise SystemExit(1)
