from genatp import Agent

agent = Agent(
    provider="nim",
    model="stepfun-ai/step-3.5-flash",
    headless=False,          # True = no visible Chrome window
)

result = agent.run(
    "Open the official Rust website. Find the current stable Rust version "
    "shown there, find the installation instructions for my operating system, "
    "and summarize exactly what command I should run. Cite the page title or section you used.",
    start_url="https://www.rust-lang.org",
)

print(f"\n=== RESULT ===")
print(f"Outcome: {result.outcome}")
print(f"Summary: {result.summary}")
print(f"Steps:   {result.steps}")
print(f"Tokens:  {result.spend.tokens} (input + output)")
print(f"Cost:    ${result.spend.usd:.4f}")
