"""Makes `az_probe` and the test helpers importable without installing the package (CI installs it
with `pip install -e`)."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
sys.path.insert(0, str(Path(__file__).resolve().parent))
