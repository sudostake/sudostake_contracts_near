import os
import asyncio
from typing import  Awaitable, TypeVar, Optional
from nearai.agents.environment import Environment
from py_near.account import Account
from decimal import Decimal


# maybe switch to rpc.fastnear.com
_DEFAULT_RPC = {
    "mainnet": "https://rpc.mainnet.near.org",
    "testnet": "https://rpc.testnet.near.org",
}

# NEAR uses 10^24 yoctoNEAR per 1 NEAR
YOCTO_FACTOR: Decimal = Decimal("1e24")

# Type‐var for our coroutine runner
T = TypeVar("T")

# ──────────────────────────────────────────────────────────────
# GLOBAL STATE
# ──────────────────────────────────────────────────────────────
_loop: Optional[asyncio.AbstractEventLoop] = None
_SIGNING_MODE: Optional[str] = None       # "headless", "wallet", or None
_ACCOUNT_ID: Optional[str] = None         # the user’s account when known

# expose handy getters
def signing_mode() -> Optional[str]: return _SIGNING_MODE
def account_id()   -> Optional[str]: return _ACCOUNT_ID
# ──────────────────────────────────────────────────────────────

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


def _set_state(mode: Optional[str], acct: Optional[str]):
    global _SIGNING_MODE, _ACCOUNT_ID
    _SIGNING_MODE, _ACCOUNT_ID = mode, acct
    
    
def init_near(env: Environment) -> Account:
    """
    Create a py-near Account.

    * headless  - secret key in env   → signing_mode = 'headless'
    * view-only - no key / wallet     → signing_mode None
    """
    
    # Check for required NEAR_NETWORK env variable
    network = os.getenv("NEAR_NETWORK")
    if network not in _DEFAULT_RPC:
        raise RuntimeError(
            "NEAR_NETWORK must be set to 'mainnet' or 'testnet' (got: "
            f"{network or 'unset'})"
        )
    
    account_id  = os.getenv("NEAR_ACCOUNT_ID")
    private_key = os.getenv("NEAR_PRIVATE_KEY")
    rpc_addr    = _DEFAULT_RPC.get(network)
    
    # For headless signing, we need both account_id and private_key
    if account_id and private_key:
        near = env.set_near(
            account_id=account_id,
            private_key=private_key,
            rpc_addr=rpc_addr
        )
        _set_state(mode="headless", acct=account_id)
        return near
    
    # view-only fallback
    signer = getattr(env, "signer_account_id", None)
    _set_state(mode=None, acct=signer)
    near = env.set_near(rpc_addr=rpc_addr)
    return near
