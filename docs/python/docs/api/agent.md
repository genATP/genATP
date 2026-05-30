# `genatp.Agent`

The high-level facade around the Rust orchestrator. Each call to
[`Agent.run`](#genatp.agent.Agent.run) launches a fresh Chrome
instance, runs the plan loop, then tears it down.

::: genatp.agent.Agent
    options:
      members:
        - run
        - arun
