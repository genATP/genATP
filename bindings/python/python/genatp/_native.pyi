"""Type stubs for the PyO3-built native extension.

These stubs are hand-maintained alongside ``src/lib.rs``. The native
module is NOT part of the public SDK surface — import from :mod:`genatp`
instead. The stubs are still shipped so editors give correct
autocomplete inside :mod:`genatp` itself.

If a new ``#[pyfunction]`` or ``#[pyclass]`` lands in
``bindings/python/src/lib.rs``, mirror it here.
"""

from typing import final


@final
class NativeRunResult:
    outcome: str
    summary: str
    steps: int
    tokens: int
    usd: float
    event_count: int

    def __repr__(self) -> str: ...


@final
class NativeProviderInfo:
    kind: str
    name: str
    model: str

    def __repr__(self) -> str: ...


@final
class NativeToolDescriptor:
    name: str
    description: str
    input_schema_json: str

    def __repr__(self) -> str: ...


@final
class NativeBrowserSession:
    def __init__(
        self,
        headless: bool = True,
        user_data_dir: str | None = None,
    ) -> None: ...
    def call_tool(self, name: str, arguments_json: str) -> tuple[str, bool, str]: ...
    def navigate(self, url: str) -> str: ...
    def snapshot(self) -> str: ...
    def close(self) -> None: ...


def version() -> str: ...
def resolve_provider_from_env() -> NativeProviderInfo: ...
def list_mcp_tools() -> list[NativeToolDescriptor]: ...
def run_agent(
    goal: str,
    start_url: str,
    model: str | None = None,
    max_steps: int = 30,
    max_tokens: int = 0,
    max_wall_s: int = 0,
    max_usd: float = 0.0,
    system_prompt: str | None = None,
    task_id: str | None = None,
    headless: bool = True,
    user_data_dir: str | None = None,
    provider: str | None = None,
    mock_reply: str | None = None,
) -> NativeRunResult: ...
def build_info() -> dict[str, object]: ...
