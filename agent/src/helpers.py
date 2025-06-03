import os
import asyncio
import requests

from nearai.agents.environment import Environment
from typing import  Awaitable, TypeVar, Optional
from nearai.agents.environment import Environment
from py_near.account import Account
from decimal import Decimal
from datetime import datetime, timezone

# Type‐var for our coroutine runner
T = TypeVar("T")

# ──────────────────────────────────────────────────────────────
# GLOBAL STATE
# ──────────────────────────────────────────────────────────────
# fastnear.com
_DEFAULT_RPC = {
    "mainnet": "https://rpc.mainnet.fastnear.com",
    "testnet": "https://rpc.testnet.fastnear.com",
}

_EXPLORER_URL = {
    "mainnet": "https://explorer.near.org",
    "testnet": "https://explorer.testnet.near.org",
}

# Factory contract addresses per network
_FACTORY_CONTRACTS = {
    "mainnet": "sudostake.near",
    "testnet": "nzaza.testnet",
}

# USDC contract addresses per network
USDC_CONTRACTS = {
    "mainnet": "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1",
    "testnet": "usdc.tkn.primitives.testnet",
}

# Firebase functions vaults API
_FIREBASE_VAULTS_API = "https://us-central1-sudostake.cloudfunctions.net" 

# Define current vault_minting_fee
# TODO Later we can dynamically get this from the factory contract itself
VAULT_MINT_FEE_NEAR: Decimal = Decimal("10")

# NEAR uses 10^24 yoctoNEAR per 1 NEAR
YOCTO_FACTOR: Decimal = Decimal("1e24")

# USDC uses 10^6 for 1 USDC
USDC_FACTOR: Decimal = Decimal("1e6")

_loop: Optional[asyncio.AbstractEventLoop] = None
_SIGNING_MODE: Optional[str] = None       # "headless", "wallet", or None
_ACCOUNT_ID: Optional[str] = None         # the user’s account when known
_VECTOR_STORE_ID: str = "vs_ecd9ba192396493984d66feb" # default vector store ID


# expose handy getters
def signing_mode()    -> Optional[str]: return _SIGNING_MODE
def account_id()      -> Optional[str]: return _ACCOUNT_ID
def vector_store_id() -> Optional[str]: return _VECTOR_STORE_ID
def firebase_vaults_api() -> str:       return _FIREBASE_VAULTS_API
# ──────────────────────────────────────────────────────────────

def usdc_contract()   -> str:
    """
    Return the USDC contract address for the current NEAR_NETWORK.
    
    We don't have to check for the environment variable here,
    as this function is only called after the NEAR_NETWORK is set
    in the environment.
    """
    network = os.getenv("NEAR_NETWORK")
    return USDC_CONTRACTS[network]


def fetch_usdc_balance(near: Account, account_id: str) -> Decimal:
    """
    Retrieve and return the USDC balance (as a Decimal) for the given account ID.
    
    Raises:
        ValueError: if the view call fails or no result is returned.
    """
    
    resp = run_coroutine(
        near.view(usdc_contract(), "ft_balance_of", {"account_id": account_id})
    )
    
    if not resp or not hasattr(resp, "result") or resp.result is None:
        raise ValueError(f"❌ No USDC balance returned for `{account_id}`.")
    
    usdc_raw = int(resp.result)
    return Decimal(usdc_raw) / USDC_FACTOR
    

def get_explorer_url() -> str:
    """
    Return the correct NEAR Explorer URL based on NEAR_NETWORK.
    """
    network = os.getenv("NEAR_NETWORK")
    return _EXPLORER_URL.get(network)


def get_factory_contract() -> str:
    """
    Return the factory contract address for the current NEAR_NETWORK.
    
    We don't have to check for the environment variable here,
    as this function is only called after the NEAR_NETWORK is set
    in the environment.
    """
    network = os.getenv("NEAR_NETWORK")
    return _FACTORY_CONTRACTS[network]


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


def get_failure_message_from_tx_status(status: dict) -> dict:
    failure = status.get("Failure")
    if failure:
        action_err = failure.get("ActionError", {})
        return action_err.get("kind", {})


def log_contains_event(logs: list[str], event_name: str) -> bool:
    """
    Returns True if any log contains the given event name.
    Supports plain or EVENT_JSON logs.
    """
    
    for log in logs:
        if event_name in log:
            return True
    return False


def top_doc_chunks(env: Environment, vs_id: str, user_query: str, k: int = 6):
    """
    Return the top-k vector-store chunks for *user_query*.
    Does not touch env.add_reply(); safe for reuse.
    """

    results = env.query_vector_store(vs_id, user_query)
    return results[:k]                      # trim noise


def index_vault_to_firebase(vault_id: str) -> None:
    """
    Index the given vault to Firebase.

    Raises:
        Exception: If the request fails or Firebase responds with an error.
    """
    
    idx_url = f"{_FIREBASE_VAULTS_API}/index_vault"
    
    response = requests.post(
        idx_url,
        json={"vault": vault_id},
        timeout=10,
        headers={"Content-Type": "application/json"},
    )
    response.raise_for_status()


def format_near_timestamp(ns: int) -> str:
    """Convert NEAR block timestamp (ns since epoch) to a readable UTC datetime."""
    ts = ns / 1_000_000_000  # Convert nanoseconds to seconds
    return datetime.fromtimestamp(ts, tz=timezone.utc).strftime("%Y-%m-%d %H:%M UTC")


def format_firestore_timestamp(ts: dict) -> str:
    """Convert Firestore timestamp dict to 'YYYY-MM-DD HH:MM UTC'."""
    dt = datetime.fromtimestamp(ts["_seconds"], tz=timezone.utc)
    return dt.strftime("%Y-%m-%d %H:%M UTC")
