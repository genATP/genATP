from genatp import Agent

agent = Agent(provider="nim", model="stepfun-ai/step-3.5-flash", headless=False)

result = agent.run(
    "Open my calendar. Find the first free 30-minute slot tomorrow between "
    "10 AM and 4 PM. Draft, but do not send, an email to Pat saying I am "
    "available at that time. Ask me for approval before sending.",
    start_url="https://calendar.google.com",
)
print(f"Outcome: {result.outcome}")
print(f"Summary: {result.summary}")
print(f"Tokens: {result.spend.tokens}")
