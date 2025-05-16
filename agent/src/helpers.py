import os
import asyncio
from typing import  Awaitable, TypeVar, Optional
from nearai.agents.environment import Environment
from py_near.account import Account

# Type‐var for our coroutine runner
T = TypeVar("T")

# Global event loop
_loop: Optional[asyncio.AbstractEventLoop] = None

def get_explorer_url() -> str:
    """
    Return the correct NEAR Explorer URL based on NEAR_NETWORK.

    Raises:
        RuntimeError if NEAR_NETWORK is missing or invalid.
    """
    network = os.getenv("NEAR_NETWORK")
    if not network:
        raise RuntimeError("Missing required environment variable: NEAR_NETWORK")

    if network not in ("mainnet", "testnet"):
        raise RuntimeError(f"Unsupported NEAR_NETWORK: {network}")

    return {
        "mainnet": "https://explorer.near.org",
        "testnet": "https://explorer.testnet.near.org",
    }[network]


def ensure_loop() -> asyncio.AbstractEventLoop:
    """Return a long-lived event loop, creating it once if necessary."""
    
    global _loop
    
    if _loop is None or _loop.is_closed():
        _loop = asyncio.new_event_loop()
        asyncio.set_event_loop(_loop)
    return _loop


def run_coroutine(coroutine: Awaitable[T]) -> T:
    """
    Helper to run an async coroutine on the shared event loop.
    """
    return ensure_loop().run_until_complete(coroutine)


def set_credentials(env: Environment) -> Account:
    """Set the NEAR connection using environment variables."""
    
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
    return env.set_near(
        account_id=account_id,
        private_key=private_key,
        rpc_addr=rpc_addr,
    )
    