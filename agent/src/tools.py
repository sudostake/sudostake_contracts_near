import logging
import sys

from decimal import Decimal
from typing import Optional
from py_near.account import Account
from nearai.agents.environment import Environment
from nearai.agents.models.tool_definition import MCPTool
from py_near.models import TransactionResult
from helpers import run_coroutine, get_explorer_url

# NEAR uses 10^24 yoctoNEAR per 1 NEAR
YOCTO_FACTOR: Decimal = Decimal("1e24")

# Global NEAR connection
_near: Optional[Account] = None  # Global NEAR connection, set in run()

# Logger for this module
_logger = logging.getLogger(__name__)

# ensure logs show up
logging.basicConfig(stream=sys.stdout, level=logging.INFO,
                    format="%(asctime)s %(levelname)s %(name)s: %(message)s")

def help() -> str:
    """
    Show a list of available tools and what they do.
    
    Returns:
      A markdown-formatted help message.
    """
    return (
        "🛠 **Available Tools:**\n\n"
        "- `vault_state(vault_id)` → View full vault status (ownership, staking, liquidity).\n"
        "- `view_available_balance(vault_id)` → Check withdrawable NEAR from a vault.\n"
        "- `delegate(vault_id, validator, amount)` → Stake NEAR to a validator from the vault.\n"
        "- `help()` → Show this help message.\n"
    )
    

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
        response = run_coroutine(_near.view(vault_id, "get_vault_state", {}))
        
        if not response or not hasattr(response, "result") or response.result is None:
            return f"❌ No data returned for `{vault_id}`. Is the contract deployed?"
        
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
        resp = run_coroutine(_near.view(vault_id, "view_available_balance", {}))
        
        if not resp or not hasattr(resp, "result") or resp.result is None:
            return f"❌ No data returned for `{vault_id}`. Is the contract deployed?"
        
        yocto = int(resp.result)
        near_amount = Decimal(yocto) / YOCTO_FACTOR
        
        return f"💰 Vault `{vault_id}` has **{near_amount:.5f} NEAR** available for withdrawal."
    except Exception as e:
        _logger.error("view_available_balance RPC error for %s: %s", vault_id, e, exc_info=True)
        return f"❌ Failed to fetch balance for `{vault_id}`\n\n**Error:** {e}"


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
        response: TransactionResult = run_coroutine(
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
        
        # Get explorer URL
        explorer = get_explorer_url()

        return (
            f"✅ **Delegation Successful**\n"
            f"Vault [`{vault_id}`]({explorer}/accounts/{vault_id}) delegated **{amount} NEAR** to validator `{validator}`.\n"
            f"🔹 **Transaction Hash**: [`{tx_hash}`]({explorer}/transactions/{tx_hash})  \n"
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
  

def register_tools(env: Environment, near: Account) -> list[MCPTool]:
    global _near
    _near = near

    registry = env.get_tool_registry()
    for tool in (help, vault_state, view_available_balance, delegate):
        registry.register_tool(tool)

    return [
        registry.get_tool_definition(name)
        for name in (
            "help",
            "vault_state",
            "view_available_balance",
            "delegate",
        )
    ]