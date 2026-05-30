#!/usr/bin/env python3
"""PRD 5-line quickstart — run after ``pip install genatp``.

    pip install genatp
    python examples/quickstart.py

Set provider env vars (see docs/CONFIGURATION.md) or use the mock
provider for a no-network smoke test::

    GENATP_LLM_PROVIDER=mock python examples/quickstart.py
"""

from __future__ import annotations

import os

from genatp import Agent

# Mock provider needs no API key — good for CI / first import check.
provider = os.environ.get("GENATP_LLM_PROVIDER")
kwargs: dict[str, object] = {}
if provider == "mock":
    kwargs["provider"] = "mock"
    kwargs["mock_reply"] = "Task complete."
    kwargs["max_steps"] = 3

agent = Agent(model=os.environ.get("GENATP_LLM_MODEL", "claude-3-5-sonnet-latest"), **kwargs)
result = agent.run(
    "Open example.com and tell me the page title.",
    start_url=os.environ.get("GENATP_START_URL", "https://example.com"),
)
print(result.summary)
