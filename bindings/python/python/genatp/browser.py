"""Direct browser-session control — the MCP tool surface, sync API.

Most users will reach for :class:`genatp.Agent`; this class exists for
the LangChain / LlamaIndex adapters that need the raw tool surface, and
for power users who want to drive Chrome without an LLM in the loop.

``BrowserSession`` is a context manager — close it (or use ``with``) to
release the underlying Chrome subprocess::

    with BrowserSession() as bs:
        bs.navigate("https://example.com")
        snapshot = bs.snapshot()
        bs.click("@e3")
"""

from __future__ import annotations

import asyncio
import base64
import json
from types import TracebackType

from genatp._native import NativeBrowserSession  # type: ignore[attr-defined]
from genatp.errors import GenAtpError, RunError


class BrowserSession:
    """One Chrome tab, driven by the genATP MCP tool dispatcher.

    Args:
        headless: Run Chrome headless. Defaults to ``True``; flip to
            ``False`` for debugging.
        user_data_dir: Persistent profile path. ``None`` (default)
            uses a fresh temp directory each session.

    Methods named after the MCP tools all return either a string
    (text-content tools) or :class:`bytes` (the ``screenshot`` tool).
    Errors surface as :class:`genatp.RunError`.
    """

    def __init__(
        self,
        *,
        headless: bool = True,
        user_data_dir: str | None = None,
    ) -> None:
        self._native = NativeBrowserSession(headless=headless, user_data_dir=user_data_dir)
        self._closed = False

    # ------------------------------------------------------------------
    # Context-manager glue
    # ------------------------------------------------------------------

    def __enter__(self) -> "BrowserSession":
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        tb: TracebackType | None,
    ) -> None:
        self.close()

    async def __aenter__(self) -> "BrowserSession":
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        tb: TracebackType | None,
    ) -> None:
        await asyncio.to_thread(self.close)

    def close(self) -> None:
        """Release the browser. Idempotent."""
        if not self._closed:
            self._native.close()
            self._closed = True

    # ------------------------------------------------------------------
    # The nine MCP tools
    # ------------------------------------------------------------------

    def navigate(self, url: str) -> str:
        """Drive the tab to ``url``. Returns the final URL after redirects."""
        self._guard()
        try:
            return self._native.navigate(url)
        except Exception as exc:  # noqa: BLE001
            raise RunError(str(exc)) from exc

    def snapshot(self) -> dict[str, object]:
        """Capture a fresh compressed accessibility snapshot.

        Returns:
            The parsed snapshot JSON — same shape as ``CompressedSnapshot``
            in :mod:`genatp_perception`.
        """
        self._guard()
        try:
            text = self._native.snapshot()
        except Exception as exc:  # noqa: BLE001
            raise RunError(str(exc)) from exc
        return json.loads(text)

    def click(self, ref: str) -> str:
        """Click the element identified by an ``@eN`` ref."""
        return self._call("click", {"ref": ref})

    def fill(self, ref: str, value: str) -> str:
        """Fill the input identified by an ``@eN`` ref with ``value``."""
        return self._call("fill", {"ref": ref, "value": value})

    def extract(self, query: str | None = None) -> str:
        """Read visible text from the cached snapshot, optionally filtered."""
        args: dict[str, object] = {}
        if query is not None:
            args["query"] = query
        return self._call("extract", args)

    def scroll(self, dx: int = 0, dy: int = 0) -> str:
        """Scroll the viewport by ``(dx, dy)`` CSS pixels."""
        return self._call("scroll", {"dx": dx, "dy": dy})

    def screenshot(self) -> bytes:
        """Capture a PNG screenshot of the viewport (returns raw bytes)."""
        self._guard()
        try:
            text, is_error, kind = self._native.call_tool("screenshot", "{}")
        except Exception as exc:  # noqa: BLE001
            raise RunError(str(exc)) from exc
        if is_error:
            raise RunError(text)
        if kind != "image":
            raise RunError(f"screenshot returned unexpected content kind: {kind}")
        return base64.b64decode(text)

    def get_url(self) -> str:
        """Return the current URL of the active tab."""
        return self._call("get_url", {})

    def go_back(self) -> str:
        """Navigate one step back in history (no-op if history is empty)."""
        return self._call("go_back", {})

    # ------------------------------------------------------------------
    # Raw escape hatch — used by the LangChain / LlamaIndex adapters.
    # ------------------------------------------------------------------

    def call_tool(self, name: str, arguments: dict[str, object] | None = None) -> str:
        """Dispatch ``tools/call`` directly. Returns text content or raises.

        Useful for forward-compatibility with future tool additions —
        anything the MCP server knows, this method can call.
        """
        return self._call(name, arguments or {})

    # ------------------------------------------------------------------
    # Internals
    # ------------------------------------------------------------------

    def _call(self, name: str, args: dict[str, object]) -> str:
        self._guard()
        try:
            text, is_error, _kind = self._native.call_tool(name, json.dumps(args))
        except Exception as exc:  # noqa: BLE001
            raise RunError(str(exc)) from exc
        if is_error:
            raise RunError(text)
        return text

    def _guard(self) -> None:
        if self._closed:
            raise GenAtpError("BrowserSession already closed")
