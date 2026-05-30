"""Unit tests for the pure-Python wrappers — no native module needed.

These run on every CI lane, including the pure-lint lane that hasn't
built the maturin wheel yet, because they only import modules that
work without the compiled extension.
"""

from __future__ import annotations

import pytest

from genatp._types import BudgetSpend, ProviderInfo, RunResult, ToolSpec
from genatp.errors import GenAtpError, ProviderError, RunError


class TestErrors:
    def test_hierarchy(self) -> None:
        assert issubclass(ProviderError, GenAtpError)
        assert issubclass(RunError, GenAtpError)

    def test_repr_includes_message(self) -> None:
        err = RunError("budget exceeded: tokens")
        assert "budget exceeded" in repr(err)


class TestDataclasses:
    def test_budget_spend_is_frozen(self) -> None:
        spend = BudgetSpend(tokens=10, usd=0.01, event_count=3)
        with pytest.raises(AttributeError):
            spend.tokens = 20  # type: ignore[misc]

    def test_run_result_ok_property(self) -> None:
        good = RunResult(
            outcome="success",
            summary="done",
            steps=1,
            spend=BudgetSpend(0, 0.0, 0),
        )
        bad = RunResult(
            outcome="aborted",
            summary="step limit",
            steps=30,
            spend=BudgetSpend(0, 0.0, 0),
        )
        assert good.ok is True
        assert bad.ok is False

    def test_provider_info_holds_kind_name_model(self) -> None:
        info = ProviderInfo(kind="anthropic", name="anthropic", model="claude-3-5-sonnet-latest")
        assert info.kind == "anthropic"
        assert info.model.startswith("claude")

    def test_tool_spec_default_shape(self) -> None:
        ts = ToolSpec(
            name="navigate",
            description="...",
            input_schema={"type": "object", "properties": {"url": {"type": "string"}}},
        )
        assert ts.name == "navigate"
        assert ts.input_schema["type"] == "object"
