from nearai.agents.environment import Environment
from typing import Optional, Any, Dict, Awaitable, TypeVar
from py_near.account import Account

# `MCPTool` is imported only for static‑type checkers; it isn’t referenced at
# runtime, so we silence the linter warning with `# noqa:`.
from nearai.agents.models.tool_definition import MCPTool  # noqa: F401 – used for type hints

import asyncio
import logging
import sys
import os

# Type‐var for our coroutine runner
T = TypeVar("T")

# --------------------------------------------------------------------------- #
# Global connection cache                                                     #
# --------------------------------------------------------------------------- #

_near: Optional[Account] = None  # Global NEAR connection, set in run()
_loop: Optional[asyncio.AbstractEventLoop] = None  # Shared event loop for async calls
_logger = logging.getLogger(__name__)

# ensure logs show up
logging.basicConfig(stream=sys.stdout, level=logging.INFO,
                    format="%(asctime)s %(levelname)s %(name)s: %(message)s")


# --------------------------------------------------------------------------- #
# Helpers                                                                     #
# --------------------------------------------------------------------------- #

def _ensure_loop() -> asyncio.AbstractEventLoop:
    """Return a long-lived event loop, creating it once if necessary."""
    
    global _loop
    
    if _loop is None or _loop.is_closed():
        _loop = asyncio.new_event_loop()
        asyncio.set_event_loop(_loop)
    return _loop


def _run(coroutine: Awaitable[T]) -> T:
    """
    Helper to run an async coroutine on the shared event loop.
    """
    return _ensure_loop().run_until_complete(coroutine)


def _set_credentials(env: Environment) -> None:
    """Set the NEAR connection using environment variables."""
    
    global _near
    
    if _near is None:
        # Pull credentials from environment variables. All are mandatory.
        account_id = os.environ.get("NEAR_ACCOUNT_ID")
        private_key = os.environ.get("NEAR_PRIVATE_KEY")
        rpc_addr = os.environ.get("NEAR_RPC")
        
        # Check for missing environment variables.
        missing = [name for name, val in {
            "NEAR_ACCOUNT_ID": account_id,
            "NEAR_PRIVATE_KEY": private_key,
            "NEAR_RPC": rpc_addr,
        }.items() if val is None]
        if missing:
            raise RuntimeError(
                f"Missing required environment variable(s): {', '.join(missing)}"
            )
        
        # Set the NEAR connection using the environment variables.
        _near = env.set_near(
            account_id=account_id,
            private_key=private_key,
            rpc_addr=rpc_addr,
        )


# --------------------------------------------------------------------------- #
# Tool functions (auto‑schema via signature + docstring)                      #
# --------------------------------------------------------------------------- #

def vault_state(vault_id: str) -> Dict[str, Any]:
    """
    Fetch the full on-chain state for a SudoStake vault.

    Args:
      vault_id: NEAR account ID of the vault.
    Returns:
      A dict matching the contract's `get_vault_state` view.
    Raises:
      RuntimeError: if NEAR connection isn't initialised.
    """
    
    if _near is None:
        raise RuntimeError("NEAR connection not initialised.")

    try:
        response = _run(_near.view(vault_id, "get_vault_state", {}))
        return response.result
    except Exception as e:
        _logger.error("vault_state RPC error for %s: %s", vault_id, e, exc_info=True)
        return {"error": "Failed to fetch vault state", "details": str(e)}


# --------------------------------------------------------------------------- #
# Main entry point – executed automatically by NearAI each turn               #
# --------------------------------------------------------------------------- #

def run(env: Environment):
    """
    Entrypoint called by NearAI at import time.

    Sets up the event loop, NEAR credentials, and registers all tools
    before handing control to NearAI's tool-runner.
    """

    # Ensure asynchronous primitives have an event loop to bind to.
    _ensure_loop()

    # Set the NEAR connection using environment variables.
    _set_credentials(env)
    
    # Register tool functions – NearAI introspects the signature/docstrings.
    registry = env.get_tool_registry()
    registry.register_tool(vault_state)
        
    # Register the tools with the environment.
    tool_defs = [registry.get_tool_definition("vault_state")]
    
    # Build the system prompt and hand off to NearAI for inference.
    system_msg = {
        "role": "system",
        "content": "You help users interact with their SudoStake Vaults"
    }

    # Pass the system message and chat history to the LLM.
    env.completions_and_run_tools(
        [system_msg] + env.list_messages(),
        tools=tool_defs,
    )


# Only invoke run(env) if NearAI has injected `env` at import time.
if "env" in globals():
    run(env)  # type: ignore[name-defined]