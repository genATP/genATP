"""Async version of the PRD quickstart.

Use ``Agent.arun`` inside any ``asyncio`` event loop. Internally it
offloads the blocking Rust call to :func:`asyncio.to_thread` so the
loop stays unblocked.
"""

from __future__ import annotations

import asyncio

from genatp import Agent


async def main() -> None:
    agent = Agent(model="gpt-4o-mini", max_wall_seconds=120)
    result = await agent.arun(
        "Summarize the top story on news.ycombinator.com.",
        start_url="https://news.ycombinator.com",
    )
    print(f"[{result.outcome}] steps={result.steps} tokens={result.spend.tokens}")
    print(result.summary)


if __name__ == "__main__":
    asyncio.run(main())
