import openai
import time
from pathlib import Path
import nearai
import os
import json
import asyncio
from typing import  Awaitable, TypeVar, Optional
from nearai.agents.environment import Environment
from py_near.account import Account
from decimal import Decimal
from typing import Final, List


# Type‐var for our coroutine runner
T = TypeVar("T")

# ──────────────────────────────────────────────────────────────
# GLOBAL STATE
# ──────────────────────────────────────────────────────────────
# maybe switch to rpc.fastnear.com
_DEFAULT_RPC = {
    "mainnet": "https://rpc.mainnet.near.org",
    "testnet": "https://rpc.testnet.near.org",
}

# Factory contract addresses per network
FACTORY_CONTRACTS = {
    "mainnet": "sudostake.near",
    "testnet": "nzaza.testnet",
}

# Firebase functions vaults API
FIREBASE_VAULTS_API = "https://us-central1-sudostake.cloudfunctions.net" 

# Define current vault_minting_fee
# TODO Later we can dynamically get this from the factory contract itself
VAULT_MINT_FEE_NEAR: Decimal = Decimal("10")

# NEAR uses 10^24 yoctoNEAR per 1 NEAR
YOCTO_FACTOR: Decimal = Decimal("1e24")

_loop: Optional[asyncio.AbstractEventLoop] = None
_SIGNING_MODE: Optional[str] = None       # "headless", "wallet", or None
_ACCOUNT_ID: Optional[str] = None         # the user’s account when known
_VECTOR_STORE_ID: Optional[str] = None    # The global vector store for SudoStake

# Tweak these knobs if you want different behaviour
POLL_INTERVAL_S:    Final[int] = 2          # seconds between status checks
MAX_BUILD_MINUTES:  Final[int] = 10         # hard cap (to avoid endless loop)

# expose handy getters
def signing_mode()    -> Optional[str]: return _SIGNING_MODE
def account_id()      -> Optional[str]: return _ACCOUNT_ID
def vector_store_id() -> Optional[str]: return _VECTOR_STORE_ID
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


def init_vector_store() -> None:
    """
    Create a NEAR-AI vector-store containing **every** Markdown file
    under *root*.  Returns the completed VectorStore object.

    Raises
    ------
    TimeoutError
        If the vector-store build fails to complete within
        ``MAX_BUILD_MINUTES``.
    RuntimeError
        If the vector-store ends in a non-"completed" status.
    """
    
    # Bootstrap the client
    config = nearai.config.load_config_file()
    base_url = config.get("api_url", "https://api.near.ai/") + "v1"
    auth = config["auth"]
    root = '.'
    client = openai.OpenAI(base_url=base_url, api_key=json.dumps(auth))
    
    # Gather *.md docs
    md_paths: List[Path] = list(Path(root).rglob("*.md"))
    
    if not md_paths:
        raise FileNotFoundError("No Markdown files found under", Path(root).resolve())
    
    # Upload each file (binary mode)
    file_ids: List[str] = []
    for p in md_paths:
        print(f"↳ uploading {p.relative_to(root)}")
        with p.open("rb") as fh:                                # binary!
            f = client.files.create(file=fh, purpose="assistants")
            file_ids.append(f.id)
    
    # Create the vector store
    vs = client.vector_stores.create(
        name="sudostake-vector-store",
        file_ids=file_ids,
        # chunking_strategy=dict(chunk_overlap_tokens=400,
        #                        max_chunk_size_tokens=800),
    )
    
    print(f"⏳ building vector-store {vs.id} ({len(file_ids)} files)…")
    
    # Poll until every file is processed or we time-out
    deadline = time.monotonic() + MAX_BUILD_MINUTES * 60
    
    while time.monotonic() < deadline:
        status = client.vector_stores.retrieve(vs.id)
        
        if (status.file_counts.completed == len(file_ids)
                and status.status == "completed"):
            print("✅ vector-store ready!")
            break
        
        if status.status == "expired":
            raise RuntimeError(f"Vector-store {vs.id} failed to build: "
                               f"{status.last_error}")
        
        time.sleep(POLL_INTERVAL_S)
        
    else:
        raise TimeoutError(f"Vector-store {vs.id} build timed out after "
                           f"{MAX_BUILD_MINUTES} minutes")
    
    # Store the vector store ID globally
    global _VECTOR_STORE_ID
    _VECTOR_STORE_ID = vs.id


def get_failure_message_from_tx_status(status: dict) -> str:
    failure = status.get("Failure")
    if failure:
        action_err = failure.get("ActionError", {})
        kind       = action_err.get("kind", {})
        func_err   = kind.get("FunctionCallError", {})
        
        return func_err.get("ExecutionError")


def top_doc_chunks(env, vs_id: str, user_query: str, k: int = 6):
    """
    Return the top-k vector-store chunks for *user_query*.
    Does not touch env.add_reply(); safe for reuse.
    """
    results = env.query_vector_store(vs_id, user_query)
    return results[:k]                      # trim noise