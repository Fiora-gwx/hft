"""Collect frozen acceptance IDs without renaming them to pytest's test_ prefix."""
import inspect
import pytest


def pytest_pycollect_makeitem(collector, name, obj):
    if inspect.isfunction(obj) and name.startswith(("core_", "dep_")):
        return pytest.Function.from_parent(collector, name=name)
