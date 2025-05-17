# stdlib / typing
from decimal import Decimal
from typing import Optional
import logging, sys

# external
from py_near.account import Account
from nearai.agents.environment import Environment
from nearai.agents.models.tool_definition import MCPTool
from py_near.models import TransactionResult

# project helpers
from helpers import (
    run_coroutine,
    get_explorer_url,
    signing_mode,
    YOCTO_FACTOR,
)

# Global NEAR connection
_near: Optional[Account] = None # NEAR connection (headless or wallet)
_env:  Optional[Environment] = None  # NEAR AI Agent environment

# Logger for this module
_logger = logging.getLogger(__name__)

# ensure logs show up
logging.basicConfig(stream=sys.stdout, level=logging.INFO,
                    format="%(asctime)s %(levelname)s %(name)s: %(message)s")

def help():
    """
    Show a concise list of the SudoStake tools.
    """
    _env.add_reply(
        "🛠 **Available Tools:**\n\n"
        "- `vault_state(vault_id)` → View full vault status (ownership, staking, liquidity).\n"
        "- `view_available_balance(vault_id)` → Check withdrawable NEAR from a vault.\n"
        "- `delegate(vault_id, validator, amount)` → Stake NEAR to a validator from the vault.\n"
        "- `help()` → Show this help message.\n"
    )
    

def vault_state(vault_id: str) -> None:
    """
    Fetch the on-chain state for `vault_id` and send it to the user.

    Args:
      vault_id: NEAR account ID of the vault.
    """
    
    if _near is None:
        _env.add_reply("❌ Agent not initialised. Please retry in a few seconds.")
        return

    try:
        response = run_coroutine(_near.view(vault_id, "get_vault_state", {}))
        if not response or not hasattr(response, "result") or response.result is None:
            _env.add_reply(f"❌ No data returned for `{vault_id}`. Is the contract deployed?")
            return
        
        # Get the result state from the response
        state = response.result
        _env.add_reply(
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
        _env.add_reply(f"❌ Failed to fetch vault state for `{vault_id}`\n\n**Error:** {e}")


def view_available_balance(vault_id: str):
    """
    Return the available NEAR balance in a readable sentence.

    Args:
      vault_id: NEAR account ID of the vault.
    """
    
    if _near is None:
        raise RuntimeError("NEAR connection not initialised.")
    
    try:
        # call the on-chain view method (contract should expose "view_available_balance")
        resp = run_coroutine(_near.view(vault_id, "view_available_balance", {}))
        
        if not resp or not hasattr(resp, "result") or resp.result is None:
            _env.add_reply(f"❌ No data returned for `{vault_id}`. Is the contract deployed?")
        
        yocto = int(resp.result)
        near_amount = Decimal(yocto) / YOCTO_FACTOR
        
        _env.add_reply(f"💰 Vault `{vault_id}` has **{near_amount:.5f} NEAR** available for withdrawal.")
    except Exception as e:
        _logger.error("view_available_balance RPC error for %s: %s", vault_id, e, exc_info=True)
        _env.add_reply(f"❌ Failed to fetch balance for `{vault_id}`\n\n**Error:** {e}")


def delegate(vault_id: str, validator: str, amount: str) -> None:
    """
    Delegate `amount` NEAR from `vault_id` to `validator`.

    • Available only in *head-less* mode (NEAR_ACCOUNT_ID + NEAR_PRIVATE_KEY).
    • Replies are pushed with _env.add_reply(); nothing is returned.
    """
    
    # Guard: agent initialised?
    if _near is None or _env is None:
         _env.add_reply("❌ Agent not initialised. Please retry in a few seconds.")
         return
    
    # 'headless', 'wallet', or None
    if signing_mode() != "headless":
        _env.add_reply(
            "⚠️ I can't sign transactions in this session.\n "
            "Add `NEAR_ACCOUNT_ID` and `NEAR_PRIVATE_KEY` to your run's "
            "secrets, then try again."
        )
        return
    
    # Parse amount (NEAR → yocto)
    try:
        yocto = int((Decimal(amount) * YOCTO_FACTOR).quantize(Decimal("1")))
    except Exception:
        _env.add_reply(f"❌ Invalid amount: {amount!r}")
        return
    
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
        gas_tgas = response.transaction_outcome.gas_burnt / 1e12
        explorer = get_explorer_url()

        _env.add_reply(
            "✅ **Delegation Successful**\n"
            f"Vault [`{vault_id}`]({explorer}/accounts/{vault_id}) delegated "
            f"**{amount} NEAR** to validator `{validator}`.\n"
            f"🔹 **Transaction Hash**: "
            f"[`{tx_hash}`]({explorer}/transactions/{tx_hash})\n"
            f"⛽ **Gas Burned**: {gas_tgas:.2f} Tgas"
        )
        
    except Exception as e:
        _logger.error(
            "delegate error %s → %s (%s NEAR): %s",
            vault_id, validator, amount, e, exc_info=True
        )
        
        _env.add_reply(
            f"❌ Delegate failed for `{vault_id}` → `{validator}` "
            f"({amount} NEAR)\n\n**Error:** {e}"
        )
  

def register_tools(env: Environment, near: Account) -> list[MCPTool]:
    global _near, _env
    _near, _env = near, env

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