#!/usr/bin/env python3
"""Run a free-tier genATP benchmark smoke suite.

This script intentionally uses only Python's standard library. It can probe
either NVIDIA NIM's OpenAI-compatible free tier or a local Ollama server, then
runs local genATP checks and writes a Markdown report.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass
class CheckResult:
    name: str
    command: str
    ok: bool
    seconds: float
    output: str


def load_env_file(path: Path) -> None:
    """Load simple KEY=VALUE pairs without overriding existing env vars."""
    if not path.exists():
        return
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip().strip('"').strip("'")
        if key and key not in os.environ:
            os.environ[key] = value


def detect_provider(requested: str) -> str:
    if requested != "auto":
        return requested
    env_provider = os.environ.get("GENATP_LLM_PROVIDER", "").strip().lower()
    if env_provider in {"nim", "nvidia"}:
        return "nim"
    if os.environ.get("NVIDIA_API_KEY"):
        return "nim"
    return "ollama"


def default_model(provider: str, requested_model: str | None) -> str:
    if requested_model:
        return requested_model
    env_model = os.environ.get("GENATP_LLM_MODEL", "").strip()
    if env_model:
        return env_model
    return "stepfun-ai/step-3.5-flash" if provider == "nim" else "llama3.1:8b"


def run_command(name: str, command: list[str], cwd: Path) -> CheckResult:
    start = time.perf_counter()
    try:
        proc = subprocess.run(
            command,
            cwd=cwd,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        elapsed = time.perf_counter() - start
        return CheckResult(
            name=name,
            command=" ".join(command),
            ok=proc.returncode == 0,
            seconds=elapsed,
            output=proc.stdout.strip(),
        )
    except FileNotFoundError as exc:
        elapsed = time.perf_counter() - start
        return CheckResult(
            name=name,
            command=" ".join(command),
            ok=False,
            seconds=elapsed,
            output=f"command not found: {exc}",
        )


def ollama_chat(base_url: str, model: str, timeout: int) -> dict[str, Any]:
    payload = {
        "model": model,
        "stream": False,
        "messages": [
            {
                "role": "user",
                "content": "Reply with exactly: OK",
            }
        ],
    }
    data = json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        f"{base_url.rstrip('/')}/api/chat",
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.loads(response.read().decode("utf-8"))


def nim_chat(base_url: str, api_key: str, model: str, timeout: int) -> dict[str, Any]:
    payload = {
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": "Reply with exactly: OK",
            }
        ],
        "max_tokens": 16,
        "temperature": 0,
    }
    data = json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        f"{base_url.rstrip('/')}/v1/chat/completions",
        data=data,
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}",
        },
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.loads(response.read().decode("utf-8"))


def format_nim_section(base_url: str, model: str, timeout: int) -> tuple[str, bool]:
    api_key = os.environ.get("NVIDIA_API_KEY") or os.environ.get("OPENAI_API_KEY")
    if not api_key:
        return (
            "## NVIDIA NIM token probe\n\n"
            "❌ `NVIDIA_API_KEY` is not set. Add it to `.env` or export it before running this script.\n",
            False,
        )
    start = time.perf_counter()
    try:
        response = nim_chat(base_url, api_key, model, timeout)
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        return (
            "## NVIDIA NIM token probe\n\n"
            f"❌ NIM HTTP {exc.code} from `{base_url}/v1/chat/completions`.\n\n"
            f"```text\n{body[-2000:]}\n```\n",
            False,
        )
    except urllib.error.URLError as exc:
        return (
            "## NVIDIA NIM token probe\n\n"
            f"❌ Could not call `{base_url}/v1/chat/completions`: `{exc}`\n",
            False,
        )
    except json.JSONDecodeError as exc:
        return (f"## NVIDIA NIM token probe\n\n❌ NIM returned invalid JSON: `{exc}`\n", False)

    elapsed = time.perf_counter() - start
    usage = response.get("usage") or {}
    prompt_tokens = int(usage.get("prompt_tokens") or 0)
    completion_tokens = int(usage.get("completion_tokens") or 0)
    total_tokens = int(usage.get("total_tokens") or (prompt_tokens + completion_tokens))
    choices = response.get("choices") or []
    content = ""
    if choices:
        content = (choices[0].get("message") or {}).get("content") or ""
    section = f"""## NVIDIA NIM token probe

| Field | Value |
|---|---:|
| Base URL | `{base_url}` |
| Model | `{model}` |
| Prompt tokens | {prompt_tokens} |
| Completion tokens | {completion_tokens} |
| Total tokens | {total_tokens} |
| Wall-clock seconds | {elapsed:.2f} |
| Response | `{content.strip()}` |

"""
    return section, True


def format_ollama_section(base_url: str, model: str, timeout: int) -> tuple[str, bool]:
    start = time.perf_counter()
    try:
        response = ollama_chat(base_url, model, timeout)
    except urllib.error.URLError as exc:
        return (
            "## Ollama token probe\n\n"
            f"❌ Could not call `{base_url}/api/chat`: `{exc}`\n\n"
            "Start Ollama and pull the model, for example:\n\n"
            "```bash\nollama serve\nollama pull {model}\n```\n".format(model=model),
            False,
        )
    except json.JSONDecodeError as exc:
        return (f"## Ollama token probe\n\n❌ Ollama returned invalid JSON: `{exc}`\n", False)

    elapsed = time.perf_counter() - start
    prompt_tokens = int(response.get("prompt_eval_count") or 0)
    completion_tokens = int(response.get("eval_count") or 0)
    total_tokens = prompt_tokens + completion_tokens
    content = response.get("message", {}).get("content", "").strip()
    total_duration_ns = int(response.get("total_duration") or 0)
    total_duration_s = total_duration_ns / 1_000_000_000 if total_duration_ns else elapsed

    section = f"""## Ollama token probe

| Field | Value |
|---|---:|
| Model | `{model}` |
| Prompt tokens | {prompt_tokens} |
| Completion tokens | {completion_tokens} |
| Total tokens | {total_tokens} |
| Wall-clock seconds | {elapsed:.2f} |
| Ollama reported seconds | {total_duration_s:.2f} |
| Response | `{content}` |

"""
    return section, True


def summarize_check(result: CheckResult) -> str:
    status = "✅" if result.ok else "❌"
    tail = "\n".join(result.output.splitlines()[-20:]) if result.output else ""
    return f"""### {status} {result.name}

```bash
{result.command}
```

Elapsed: {result.seconds:.2f}s

<details><summary>last output lines</summary>

```text
{tail}
```

</details>

"""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--provider",
        choices=["auto", "nim", "ollama"],
        default="auto",
        help="free provider to probe; auto uses GENATP_LLM_PROVIDER/NVIDIA_API_KEY",
    )
    parser.add_argument("--model", help="model to probe; defaults to GENATP_LLM_MODEL")
    parser.add_argument(
        "--env-file",
        type=Path,
        default=Path(".env"),
        help="optional .env file to load without overriding exported variables",
    )
    parser.add_argument(
        "--nim-url",
        default="https://integrate.api.nvidia.com",
        help="NVIDIA NIM OpenAI-compatible base URL",
    )
    parser.add_argument(
        "--ollama-url",
        default="http://127.0.0.1:11434",
        help="Ollama base URL",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=120,
        help="Ollama probe timeout in seconds",
    )
    parser.add_argument(
        "--full",
        action="store_true",
        help="also run the fast perception Criterion benchmark",
    )
    parser.add_argument(
        "--all-benches",
        action="store_true",
        help="run every workspace Criterion benchmark; slow and may launch browsers",
    )
    parser.add_argument(
        "--out",
        type=Path,
        help="optional Markdown report path",
    )
    args = parser.parse_args()

    repo = Path(__file__).resolve().parents[1]
    env_file = args.env_file if args.env_file.is_absolute() else repo / args.env_file
    load_env_file(env_file)
    provider = detect_provider(args.provider)
    model = default_model(provider, args.model)
    generated_at = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    report_parts = [
        "# genATP Free Benchmark Report\n\n",
        f"Generated at: `{generated_at}`\n\n",
        f"Provider: `{provider}`  \n",
        f"Model: `{model}`  \n\n",
        "This report uses local commands plus a free-tier/local LLM token probe. It never prints API keys.\n\n",
    ]

    if provider == "nim":
        probe_section, probe_ok = format_nim_section(args.nim_url, model, args.timeout)
    else:
        probe_section, probe_ok = format_ollama_section(args.ollama_url, model, args.timeout)
    report_parts.append(probe_section)

    checks = [
        ("Git commit", ["git", "rev-parse", "--short", "HEAD"]),
        ("CLI tests", ["cargo", "test", "-p", "genatp-cli"]),
        (
            "Perception token gate",
            ["cargo", "test", "-p", "genatp-perception", "--test", "golden"],
        ),
    ]
    if args.full:
        checks.append(
            (
                "Perception Criterion benchmark",
                ["cargo", "bench", "-p", "genatp-perception", "--bench", "perception_tokens"],
            )
        )
    if args.all_benches:
        checks.append(
            (
                "All workspace Criterion benchmarks",
                [
                    "cargo",
                    "bench",
                    "--workspace",
                    "--exclude",
                    "genatp-python",
                    "--exclude",
                    "genatp-node",
                ],
            )
        )

    report_parts.append("## Local genATP checks\n\n")
    results = [run_command(name, command, repo) for name, command in checks]
    for result in results:
        report_parts.append(summarize_check(result))

    ok = probe_ok and all(result.ok for result in results)
    report_parts.append(f"## Overall result\n\n{'✅ PASS' if ok else '❌ FAIL'}\n")
    report = "".join(report_parts)

    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(report, encoding="utf-8")
        print(f"wrote {args.out}")
    else:
        print(report)

    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
