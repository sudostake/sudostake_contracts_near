# stdlib / typing
from decimal import Decimal
from typing import Optional
import logging
import sys
import os
import json

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
    account_id,
    get_failure_message_from_tx_status,
    YOCTO_FACTOR,
    FACTORY_CONTRACTS,
    VAULT_MINT_FEE_NEAR
)

# Global NEAR connection
_near: Optional[Account] = None # NEAR connection (headless or wallet)
_env:  Optional[Environment] = None  # NEAR AI Agent environment

# Logger for this module
_logger = logging.getLogger(__name__)

# ensure logs show up
logging.basicConfig(stream=sys.stdout, level=logging.INFO,
                    format="%(asctime)s %(levelname)s %(name)s: %(message)s")

def show_help_menu() -> None:
    """Send a concise list of available SudoStake tools."""
    _env.add_reply(
        "🛠 **Available Tools:**\n\n"
        "- `view_main_balance()` → Show the balance of your main wallet (requires signing keys).\n"
        "- `mint_vault()` → Create a new vault (fixed 10 NEAR minting fee).\n"
        "- `transfer_near_to_vault(vault_id, amount)` → Send NEAR from your wallet to a vault.\n"
        "- `vault_state(vault_id)` → View a vault's owner, staking and liquidity status.\n"
        "- `view_available_balance(vault_id)` → Show withdrawable NEAR for a vault.\n"
        "- `delegate(vault_id, validator, amount)` → Stake NEAR from the vault to a validator.\n"
        "- `undelegate(vault_id, validator, amount)` → Unstake NEAR from a validator for a vault.\n"
        "- `withdraw_balance(vault_id, amount, to_address=None)` → Withdraw NEAR from the vault. Optionally specify a recipient.\n"
        "- `view_vault_status_with_validator(vault_id, validator_id)` → Check vault's staking info with a validator (staked, unstaked, can withdraw).\n"
        "- `claim_unstaked_balance(vault_id, validator)` → Claim matured unstaked NEAR from a validator.\n"
        "- `show_help_menu()` → Display this help.\n"
    )


def view_main_balance() -> None:
    """
    Show the balance of the user's main wallet (the account whose key
    is loaded for head-less mode).

    • Works only when `signing_mode() == "headless"`.
    • Replies are sent via `_env.add_reply()`; nothing is returned.
    """
    
    # 'headless' or None
    if signing_mode() != "headless":
        _env.add_reply(
            "⚠️ No signing keys available. Add `NEAR_ACCOUNT_ID` and "
            "`NEAR_PRIVATE_KEY` to secrets, then try again."
        )
        return
    
    # Get the signer's account id
    acct_id = account_id()
    
    try:
        # py_near.Account.get_balance() -> int with 'amount' in yocto
        yocto = run_coroutine(_near.get_balance())
        near_bal = Decimal(yocto) / YOCTO_FACTOR
        
        _env.add_reply(
            f"💼 **Main Account Balance**\n"
            f"Account: `{acct_id}`\n"
            f"Available: **{near_bal:.5f} NEAR**"
        )
    
    except Exception as e:
        _logger.error("view_main_balance error: %s", e, exc_info=True)
        _env.add_reply(f"❌ Failed to fetch balance\n\n**Error:** {e}")


def mint_vault() -> None:
    """
    Mint a new SudoStake vault.

    • Head-less signing required (NEAR_ACCOUNT_ID + NEAR_PRIVATE_KEY).  
    • Uses the fixed 10 NEAR fee ( `VAULT_MINT_FEE_NEAR` ).  
    • Factory account is derived from `NEAR_NETWORK`.
    """
    
    # 'headless' or None
    if signing_mode() != "headless":
        _env.add_reply(
            "⚠️ I can't sign transactions in this session.\n "
            "Add `NEAR_ACCOUNT_ID` and `NEAR_PRIVATE_KEY` to your run's "
            "secrets, then try again."
        )
        return
    
    # Prepare call params
    factory_id = FACTORY_CONTRACTS[os.getenv("NEAR_NETWORK")]
    yocto_fee  = int((VAULT_MINT_FEE_NEAR * YOCTO_FACTOR).quantize(Decimal('1')))
    
    try:
        # Perform the payable delegate call with yocto_fee attached
        response: TransactionResult = run_coroutine(
            _near.call(
                contract_id=factory_id,
                method_name="mint_vault",
                args={},
                gas=300_000_000_000_000,        # 300 Tgas
                amount=yocto_fee,               # 10 NEAR in yocto
            )
        )
        
        # Inspect execution outcome for Failure / Panic
        failure = get_failure_message_from_tx_status(response.status)
        if failure:
            _env.add_reply(
                "❌ Mint vault failed with **contract panic**:\n\n"
                f"> {failure}"
            )
            return
        
        # Extract tx_hash from the response
        tx_hash  = response.transaction.hash
        explorer = get_explorer_url()
        
        # Extract new vault account from EVENT_JSON log
        vault_acct = None
        for log in response.logs:
            if log.startswith("EVENT_JSON:"):
                payload = json.loads(log.split("EVENT_JSON:")[1])
                if payload.get("event") == "vault_minted":
                    vault_acct = payload["data"]["vault"]
                    break
            
        if vault_acct is None:
            raise RuntimeError("vault_minted log not found in transaction logs")
        
        _env.add_reply(
            "🏗️ **Vault Minted**\n"
            f"🔑 Vault account: [`{vault_acct}`]({explorer}/accounts/{vault_acct})\n"
            f"🔹 Tx: [{tx_hash}]({explorer}/transactions/{tx_hash})"
        )
    
    except Exception as e:
        _logger.error("mint_vault error: %s", e, exc_info=True)
        _env.add_reply(f"❌ Vault minting failed\n\n**Error:** {e}")
    
    
def transfer_near_to_vault(vault_id: str, amount: str) -> None:
    """
    Transfer `amount` NEAR from the main wallet to `vault_id`.

    • Head-less signing required (NEAR_ACCOUNT_ID & NEAR_PRIVATE_KEY).
    • Uses py-near `send_money` (amount must be in yocto).
    """
     
    # 'headless' or None
    if signing_mode() != "headless":
        _env.add_reply(
            "⚠️ No signing keys available. Add `NEAR_ACCOUNT_ID` and "
            "`NEAR_PRIVATE_KEY` to secrets, then try again."
        )
        return
    
    # Parse amount (NEAR → yocto)
    try:
        yocto = int((Decimal(amount) * YOCTO_FACTOR).quantize(Decimal("1")))
    except Exception:
        _env.add_reply(f"❌ Invalid amount: {amount!r}")
        return
    
    try:
        tx: TransactionResult = run_coroutine(
            _near.send_money(account_id=vault_id, amount=yocto)
        )
        
        tx_hash  = tx.transaction.hash
        explorer = get_explorer_url()
        
        _env.add_reply(
            "💸 **Transfer Submitted**\n"
            f"Sent **{Decimal(amount):.5f} NEAR** to `{vault_id}`.\n"
            f"🔹 Tx: [{tx_hash}]({explorer}/transactions/{tx_hash})"
        )
        
    except Exception as e:
        _logger.error(
            "transfer_near_to_vault error → %s (%s NEAR): %s",
            vault_id, amount, e, exc_info=True
        )
        _env.add_reply(
            f"❌ Transfer failed for `{vault_id}` ({amount} NEAR)\n\n**Error:** {e}"
        )


def vault_state(vault_id: str) -> None:
    """
    Fetch the on-chain state for `vault_id` and send it to the user.

    Args:
      vault_id: NEAR account ID of the vault.
    """

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


def view_available_balance(vault_id: str) -> None:
    """
    Return the available NEAR balance in a readable sentence.

    Args:
      vault_id: NEAR account ID of the vault.
    """
    
    try:
        # call the on-chain view method (contract should expose "view_available_balance")
        resp = run_coroutine(_near.view(vault_id, "view_available_balance", {}))
        
        if not resp or not hasattr(resp, "result") or resp.result is None:
            _env.add_reply(f"❌ No data returned for `{vault_id}`. Is the contract deployed?")
            return
        
        yocto = int(resp.result)
        near_amount = Decimal(yocto) / YOCTO_FACTOR
        
        _env.add_reply(f"💰 Vault `{vault_id}` has **{near_amount:.5f} NEAR** available for withdrawal.")
    except Exception as e:
        _logger.error("view_available_balance RPC error for %s: %s", vault_id, e, exc_info=True)
        _env.add_reply(f"❌ Failed to fetch balance for `{vault_id}`\n\n**Error:** {e}")


def delegate(vault_id: str, validator: str, amount: str) -> None:
    """
    Delegate `amount` NEAR from `vault_id` to `validator`.

    • **Head-less mode only** - requires `NEAR_ACCOUNT_ID` + `NEAR_PRIVATE_KEY`.  
    • Sends exactly **one** `_env.add_reply()` message; returns `None`.  
    • Detects and surfaces contract panics (require!/assert! failures).
    """
    
    # 'headless' or None
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
        
        # Inspect execution outcome for Failure / Panic
        failure = get_failure_message_from_tx_status(response.status)
        if failure:
            _env.add_reply(
                "❌ Delegate failed with **contract panic**:\n\n"
                f"> {failure}"
            )
            return

        # Extract only the primitive fields we care about
        tx_hash  = response.transaction.hash
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
  
  
def undelegate(vault_id: str, validator: str, amount: str) -> None:
    """
    Undelegate `amount` NEAR from `validator` for `vault_id`.

    • **Head-less mode only** - requires `NEAR_ACCOUNT_ID` + `NEAR_PRIVATE_KEY`.
    • Uses the vault contract's `undelegate` method.  
    • Sends exactly **one** `_env.add_reply()` message; returns `None`.  
    • Detects and surfaces contract panics (require!/assert! failures).
    """
    
    # 'headless' or None
    if signing_mode() != "headless":
        _env.add_reply(
            "⚠️ No signing keys available. Add `NEAR_ACCOUNT_ID` and "
            "`NEAR_PRIVATE_KEY` to secrets, then try again."
        )
        return
    
    # Parse amount (NEAR → yocto)
    try:
        yocto = int((Decimal(amount) * YOCTO_FACTOR).quantize(Decimal("1")))
    except Exception:
        _env.add_reply(f"❌ Invalid amount: {amount!r}")
        return
    
    try:
        # Perform the payable undelegate call with 1 yoctoNEAR attached        
        response: TransactionResult = run_coroutine(
            _near.call(
                contract_id=vault_id,
                method_name="undelegate",
                args={"validator": validator, "amount": str(yocto)},
                gas=300_000_000_000_000,  # 300 TGas
                amount=1,                 # 1 yoctoNEAR deposit
            )
        )
        
        # Inspect execution outcome for Failure / Panic
        failure = get_failure_message_from_tx_status(response.status)
        if failure:
            _env.add_reply(
                "❌ Undelegate failed with **contract panic**:\n\n"
                f"> {failure}"
            )
            return
        
        # Extract only the primitive fields we care about
        tx_hash  = response.transaction.hash
        gas_tgas = response.transaction_outcome.gas_burnt / 1e12
        explorer = get_explorer_url()
        
        _env.add_reply(
            "✅ **Undelegation Successful**\n"
            f"Vault [`{vault_id}`]({explorer}/accounts/{vault_id}) undelegated "
            f"**{amount} NEAR** from `{validator}`.\n"
            f"🔹 **Transaction Hash**: "
            f"[`{tx_hash}`]({explorer}/transactions/{tx_hash})\n"
            f"⛽ **Gas Burned**: {gas_tgas:.2f} Tgas"
        )
    
    except Exception as e:
        _logger.error(
            "undelegate error %s ← %s (%s NEAR): %s",
            vault_id, validator, amount, e, exc_info=True
        )
        _env.add_reply(
            f"❌ Undelegate failed for `{vault_id}` ← `{validator}` "
            f"({amount} NEAR)\n\n**Error:** {e}"
        )


def withdraw_balance(vault_id: str, amount: str, to_address: str) -> None:
    """
    Withdraw `amount` NEAR from `vault_id`. Optionally, send to a third-party `to_address`.

    • Only works in headless mode.
    • Uses 1 yoctoNEAR for call.
    • Calls the `withdraw_balance` method on the vault contract.
    """
    
    # 'headless' or None
    if signing_mode() != "headless":
        _env.add_reply(
            "⚠️ I can't sign transactions in this session.\n"
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
        args = {
            "amount": str(yocto)
        }
        
        if to_address:
            args["to"] = to_address
        
        # Perform the payable withdraw_balance call with 1 yoctoNEAR attached  
        response: TransactionResult = run_coroutine(
            _near.call(
                contract_id=vault_id,
                method_name="withdraw_balance",
                args=args,
                gas=300_000_000_000_000,  # 300 TGas
                amount=1,                 # 1 yoctoNEAR
            )
        )
        
        # Inspect execution outcome for Failure / Panic
        failure = get_failure_message_from_tx_status(response.status)
        if failure:
            _env.add_reply(
                "❌ Withdraw failed with **contract panic**:\n\n"
                f"> {failure}"
            )
            return
        
        # Extract only the primitive fields we care about
        tx_hash = response.transaction.hash
        explorer = get_explorer_url()
        
        _env.add_reply(
            "✅ **Withdrawal Successful**\n"
            f"Vault [`{vault_id}`]({explorer}/accounts/{vault_id}) withdrew "
            f"**{amount} NEAR**"
            + (f" to `{to_address}`." if to_address else " to your main account.") +
            f"\n🔹 [View Tx]({explorer}/transactions/{tx_hash})"
        )
        
    except Exception as e:
        _logger.error(
            "withdraw_balance error %s → %s (%s NEAR): %s",
            vault_id, to_address, amount, e, exc_info=True
        )
        _env.add_reply(f"❌ Withdraw failed for `{vault_id}` → `{to_address or 'self'}`\n\n**Error:** {e}")
        
        
def view_vault_status_with_validator(vault_id: str, validator_id: str) -> None:
    """
    Query the `get_account` view method on a validator contract to get vault staking info.

    Shows:
      - Staked balance
      - Unstaked balance
      - Withdrawable status
    """
    
    try:
        response = run_coroutine(
            _near.view(
                contract_id=validator_id,
                method_name="get_account",
                args={"account_id": vault_id}
            )
        )
        
        if not response or not hasattr(response, "result") or response.result is None:
            _env.add_reply(f"❌ No data returned for `{vault_id}` at validator `{validator_id}`.")
            return
        
        data = response.result
        staked = Decimal(data["staked_balance"]) / YOCTO_FACTOR
        unstaked = Decimal(data["unstaked_balance"]) / YOCTO_FACTOR
        can_withdraw = "✅ Yes" if data["can_withdraw"] else "❌ No"
        
        _env.add_reply(
            f"📊 **Delegation Status** for `{vault_id}` at `{validator_id}`\n\n"
            f"- **Staked Balance**: {staked:.5f} NEAR\n"
            f"- **Unstaked Balance**: {unstaked:.5f} NEAR\n"
            f"- **Withdrawable Now**: {can_withdraw}"
        )
        
    except Exception as e:
        _logger.error("view_vault_status_with_validator error: %s", e, exc_info=True)
        _env.add_reply(
            f"❌ Failed to get delegation status for `{vault_id}` at `{validator_id}`\n\n**Error:** {e}"
        )


def claim_unstaked_balance(vault_id: str, validator: str) -> None:
    """
    Call the `claim_unstaked` method on the vault to withdraw matured unstaked NEAR.

    • Must be the vault owner.
    • Requires 1 yoctoNEAR.
    • Will only succeed if the unstaked balance is available.
    """
    
    # 'headless' or None
    if signing_mode() != "headless":
        _env.add_reply(
            "⚠️ I can't sign transactions in this session.\n"
            "Add `NEAR_ACCOUNT_ID` and `NEAR_PRIVATE_KEY` to your run's "
            "secrets, then try again."
        )
        return
    
    try:
        response: TransactionResult = run_coroutine(
            _near.call(
                contract_id=vault_id,
                method_name="claim_unstaked",
                args={"validator": validator},
                gas=300_000_000_000_000,
                amount=1  # 1 yoctoNEAR
            )
        )
        
        failure = get_failure_message_from_tx_status(response.status)
        if failure:
            _env.add_reply(
                f"❌ Claim failed with **contract panic**:\n\n> {failure}"
            )
            return
        
        tx_hash = response.transaction.hash
        explorer = get_explorer_url()
        
        _env.add_reply(
            "📥 **Claim Initiated**\n"
            f"Vault `{vault_id}` is claiming matured unstaked NEAR from `{validator}`.\n"
            f"🔹 [View Tx]({explorer}/transactions/{tx_hash})"
        )
    
    except Exception as e:
        _logger.error("claim_unstaked_balance error: %s", e, exc_info=True)
        _env.add_reply(
            f"❌ Failed to claim unstaked NEAR from `{validator}` for `{vault_id}`\n\n**Error:** {e}"
        )
        

def register_tools(env: Environment, near: Account) -> list[MCPTool]:
    global _near, _env
    _near, _env = near, env

    registry = env.get_tool_registry()
    for tool in (
        show_help_menu, 
        view_main_balance,
        mint_vault,
        transfer_near_to_vault,
        vault_state, 
        view_available_balance, 
        delegate,
        undelegate,
        withdraw_balance,
        view_vault_status_with_validator,
        claim_unstaked_balance,
    ):
        registry.register_tool(tool)

    return [
        registry.get_tool_definition(name)
        for name in (
            "show_help_menu",
            "view_main_balance",
            "mint_vault",
            "transfer_near_to_vault",
            "vault_state",
            "view_available_balance",
            "delegate",
            "undelegate",
            "withdraw_balance",
            "view_vault_status_with_validator",
            "claim_unstaked_balance"
        )
    ]