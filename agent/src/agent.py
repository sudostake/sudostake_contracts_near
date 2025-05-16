from nearai.agents.environment import Environment
from typing import Optional, Any, Dict, Awaitable, TypeVar
from py_near.account import Account
from py_near.models import TransactionResult
from decimal import Decimal

# `MCPTool` is imported only for static‑type checkers; it isn’t referenced at
# runtime, so we silence the linter warning with `# noqa:`.
from nearai.agents.models.tool_definition import MCPTool  # noqa: F401 – used for type hints

import asyncio
import logging
import sys
import os

# Type‐var for our coroutine runner
T = TypeVar("T")

# NEAR uses 10^24 yoctoNEAR per 1 NEAR
YOCTO_FACTOR: Decimal = Decimal("1e24")

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

def vault_state(vault_id: str) -> str:
    """
    Fetch and render the on-chain state for a SudoStake vault in Markdown format.

    Args:
      vault_id: NEAR account ID of the vault.
    Returns:
      A markdown-formatted string showing the vault's state.
    Raises:
      RuntimeError: if NEAR connection isn't initialised.
    """
    
    if _near is None:
        raise RuntimeError("NEAR connection not initialised.")

    try:
        response = _run(_near.view(vault_id, "get_vault_state", {}))
        state = response.result
        
        return (
            f"✅ **Vault State: `{vault_id}`**\n\n"
            f"| Field                  | Value                       |\n"
            f"|------------------------|-----------------------------|\n"
            f"| Owner                  | `{state['owner']}`          |\n"
            f"| Index                  | `{state['index']}`          |\n"
            f"| Version                | `{state['version']}`        |\n"
            f"| Listed for Takeover    | `{state['is_listed_for_takeover']}` |\n"
            f"| Pending Request        | `{state['pending_liquidity_request']}` |\n"
            f"| Active Request         | `{state['liquidity_request']}` |\n"
            f"| Accepted Offer         | `{state['accepted_offer']}` |\n"
        )
    except Exception as e:
        _logger.error("vault_state RPC error for %s: %s", vault_id, e, exc_info=True)
        return f"❌ Failed to fetch vault state for `{vault_id}`\n\n**Error:** {e}"


def view_available_balance(vault_id: str) -> str:
    """
    Return the available NEAR balance in a readable sentence.

    Args:
      vault_id: NEAR account ID of the vault.
    Returns:
      A plain string reporting the withdrawable NEAR balance.
    Raises:
      RuntimeError: if NEAR connection isn't initialised.
    """
    
    if _near is None:
        raise RuntimeError("NEAR connection not initialised.")
    
    try:
        # call the on-chain view method (contract should expose "view_available_balance")
        resp = _run(_near.view(vault_id, "view_available_balance", {}))
        yocto = int(resp.result)
        near_amount = Decimal(yocto) / YOCTO_FACTOR
        
        return f"💰 Vault `{vault_id}` has **{near_amount:.5f} NEAR** available for withdrawal."
    except Exception as e:
        _logger.error("view_available_balance RPC error for %s: %s", vault_id, e, exc_info=True)
        return {"error": "Failed to fetch available balance", "details": str(e)}


def delegate(vault_id: str, validator: str, amount: str) -> str:
    """
    Delegate `amount` NEAR from `vault_id` to `validator`.

    Returns:
      A markdown summary of the transaction result, formatted for display or piping to glow.
    """
    
    if _near is None:
        raise RuntimeError("NEAR connection not initialised.")
    
    # Convert NEAR -> yoctoNEAR
    try:
        yocto = int(Decimal(amount) * YOCTO_FACTOR)
    except Exception:
        raise ValueError(f"Invalid amount: {amount!r}")
    
    try:
        # Perform the payable delegate call with 1 yoctoNEAR attached        
        response: TransactionResult = _run(
            _near.call(
                contract_id=vault_id,
                method_name="delegate",
                args={"validator": validator, "amount": str(yocto)},
                gas=300_000_000_000_000,  # 300 TGas
                amount=1,                 # 1 yoctoNEAR deposit
            )
        )

        # Extract only the primitive fields we care about
        tx_hash   = response.transaction.hash
        gas_burnt = response.transaction_outcome.gas_burnt
        logs      = response.logs
        
        # Parse log highlights
        parsed_logs = []
        for log in logs:
            if "lock_acquired" in log:
                parsed_logs.append("🔒 Lock acquired")
            elif "lock_released" in log:
                parsed_logs.append("🔓 Lock released")
            elif "delegate_completed" in log:
                parsed_logs.append("✅ Delegate completed")
            elif "staking" in log.lower():
                parsed_logs.append("📈 Stake successful")
            elif "deposited" in log.lower():
                parsed_logs.append("📥 Deposit received")
        
        # Convert to Tgas
        gas_tgas = gas_burnt / 1e12

        return (
            f"✅ **Delegation Successful**\n"
            f"Vault [`{vault_id}`](https://explorer.testnet.near.org/accounts/{vault_id}) delegated **{amount} NEAR** to validator `{validator}`.\n"
            f"🔹 **Transaction Hash**: [`{tx_hash}`](https://explorer.testnet.near.org/transactions/{tx_hash})  \n"
            f"⛽ **Gas Burned**: {gas_tgas:.2f} Tgas\n\n"
            f"📄 **Logs**:\n" +
            "\n".join(f"- {line}" for line in parsed_logs) if parsed_logs else "_No log entries found._"
        )
    except Exception as e:
        _logger.error(
            "delegate transaction error for %s → %s (%s NEAR): %s",
            vault_id, validator, amount, e, exc_info=True
        )
        
        return f"❌ Delegate transaction failed for `{vault_id}` → `{validator}` ({amount} NEAR)\n\n**Error:** {e}"
  
    
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
    for tool in (vault_state, view_available_balance, delegate):
        registry.register_tool(tool)
        
    # Register the tools with the environment.
    tool_defs = [
        registry.get_tool_definition(name)
        for name in (
            "vault_state", 
            "view_available_balance",
            "delegate"
        )
    ]
    
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
