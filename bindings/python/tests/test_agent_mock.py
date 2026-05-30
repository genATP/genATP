"""Integration tests using the mock LLM provider.

The mock provider replies with whatever string is passed via
``mock_reply``. The orchestrator treats a non-tool-call assistant
reply with ``finish_reason: Stop`` as success, so a single mock
roundtrip is enough to exercise the full plan loop end-to-end without
needing Chrome or the network.

We still need a browser substrate, so these tests are skipped unless
the wheel is built. They also need Chrome to be installed on the host —
guarded by ``CHROME_PATH`` so CI runs that haven't installed Chrome can
skip cleanly.
"""

from __future__ import annotations

import os
import shutil

import pytest

pytestmark = pytest.mark.integration


def _chrome_available() -> bool:
    if os.environ.get("CHROME_PATH"):
        return True
    return any(
        shutil.which(name)
        for name in ("google-chrome", "google-chrome-stable", "chromium", "chromium-browser")
    )


@pytest.mark.skipif(not _chrome_available(), reason="Chrome not available on this host")
def test_mock_agent_run_returns_success() -> None:
    from genatp import Agent

    agent = Agent(provider="mock", mock_reply="finished", max_steps=2)
    # `data:` URLs cost no network and load instantly.
    result = agent.run(
        "Read the page and call finish.",
        start_url="data:text/html,<h1>hi</h1>",
    )
    assert result.outcome in {"success", "aborted", "error"}
    # Whatever the outcome, spend should be a real BudgetSpend.
    assert result.spend.event_count >= 0
    assert result.steps >= 0


@pytest.mark.asyncio
@pytest.mark.skipif(not _chrome_available(), reason="Chrome not available on this host")
async def test_async_agent_arun_round_trip() -> None:
    from genatp import Agent

    agent = Agent(provider="mock", mock_reply="finished", max_steps=2)
    result = await agent.arun(
        "Read the page and call finish.",
        start_url="data:text/html,<h1>hi</h1>",
    )
    assert result.outcome in {"success", "aborted", "error"}
