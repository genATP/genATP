"""The PRD 5-line quickstart, expanded with a print statement.

Run::

    pip install genatp
    python examples/quickstart.py
"""

from genatp import Agent

agent = Agent(model="claude-3-5-sonnet-latest")
result = agent.run(
    "Find the cheapest direct flight from SFO to JFK next Friday.",
    start_url="https://www.google.com/travel/flights",
)
print(result.summary)
