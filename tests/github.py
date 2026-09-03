from genatp import Agent

agent = Agent(
    provider="nim",
    model="stepfun-ai/step-3.5-flash",
    headless=False,
    # user_data_dir="C:\\Users\\prasa\\AppData\\Local\\Google\\Chrome\\User Data",
)

result = agent.run(
    "Open GitHub with my saved session."
    "mark all the ci cd failure related notifications as done which are from the genatp repository",
    start_url="https://github.com",
)

print(f"Outcome: {result.outcome}")
print(f"Summary: {result.summary}")
print(f"Tokens: {result.spend.tokens}")
