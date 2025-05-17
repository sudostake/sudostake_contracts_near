import os
import asyncio
from typing import  Awaitable, TypeVar, Optional
from nearai.agents.environment import Environment
from py_near.account import Account
from decimal import Decimal

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
    Initialize and return a pynear_account (an instance of the Account class).

    * headless  -> full signing Account, session.signing_mode = 'headless'
    * wallet    -> view-only Account,  session.signing_mode = 'wallet'
    * neither   -> view-only Account,  signing_mode unset
    """
    
    account_id  = os.getenv("NEAR_ACCOUNT_ID")
    private_key = os.getenv("NEAR_PRIVATE_KEY")
    rpc_addr    = os.getenv("NEAR_RPC") or _DEFAULT_RPC.get(
        os.getenv("NEAR_NETWORK", "testnet")
    )
    
    # For headless signing, we need both account_id and private_key
    if account_id and private_key:
        near = env.set_near(account_id=account_id,
                            private_key=private_key,
                            rpc_addr=rpc_addr)
        _set_state(mode="headless", acct=account_id)
        return near
    
    # For wallet signing, we only need the account_id
    signer = getattr(env, "signer_account_id", None)
    _set_state(mode="wallet" if signer else None, acct=signer)
    near = env.set_near(account_id=signer or "anon", rpc_addr=rpc_addr)
    
    return near