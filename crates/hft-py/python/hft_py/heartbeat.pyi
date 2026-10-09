"""hft_py.heartbeat: read-only view of the hft-rt HeartbeatBlock."""

from os import PathLike
from typing import Any, final

from typing_extensions import Self

__all__ = ["Watch", "read"]

def read(instance: str, root: str | PathLike[str] | None = None) -> dict[str, Any]: ...
@final
class Watch:
    def __new__(cls, instance: str, root: str | PathLike[str] | None = None) -> Self: ...
    def check(self) -> dict[str, Any]: ...
