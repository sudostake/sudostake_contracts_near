import json

from decimal import Decimal
from logging import Logger
from .context import get_env, get_near, get_logger
from helpers import YOCTO_FACTOR, signing_mode, run_coroutine, get_failure_message_from_tx_status, get_explorer_url
from py_near.models import TransactionResult

def delegate(vault_id: str, validator: str, amount: str) -> None:
    """
    Delegate `amount` NEAR from `vault_id` to `validator`.

    • **Head-less mode only** - requires `NEAR_ACCOUNT_ID` + `NEAR_PRIVATE_KEY`.  
    • Sends exactly **one** `_env.add_reply()` message; returns `None`.  
    • Detects and surfaces contract panics (require!/assert! failures).
    """
    
    env = get_env()
    near = get_near()
    logger: Logger = get_logger()
    
    # 'headless' or None
    if signing_mode() != "headless":
        env.add_reply(
            "⚠️ I can't sign transactions in this session.\n "
            "Add `NEAR_ACCOUNT_ID` and `NEAR_PRIVATE_KEY` to your run's "
            "secrets, then try again."
        )
        return
    
    # Parse amount (NEAR → yocto)
    try:
        yocto = int((Decimal(amount) * YOCTO_FACTOR).quantize(Decimal("1")))
    except Exception:
        env.add_reply(f"❌ Invalid amount: {amount!r}")
        return
    
    try:
        # Perform the payable delegate call with 1 yoctoNEAR attached        
        response: TransactionResult = run_coroutine(
            near.call(
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
            env.add_reply(
                "❌ Delegate failed with **contract panic**:\n\n"
               f"> {json.dumps(failure, indent=2)}"
            )
            return

        # Extract only the primitive fields we care about
        tx_hash  = response.transaction.hash
        gas_tgas = response.transaction_outcome.gas_burnt / 1e12
        explorer = get_explorer_url()

        env.add_reply(
            "✅ **Delegation Successful**\n"
            f"Vault [`{vault_id}`]({explorer}/accounts/{vault_id}) delegated "
            f"**{amount} NEAR** to validator `{validator}`.\n"
            f"🔹 **Transaction Hash**: "
            f"[`{tx_hash}`]({explorer}/transactions/{tx_hash})\n"
            f"⛽ **Gas Burned**: {gas_tgas:.2f} Tgas"
        )
        
    except Exception as e:
        logger.error(
            "delegate error %s → %s (%s NEAR): %s",
            vault_id, validator, amount, e, exc_info=True
        )
        
        env.add_reply(
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
    
    env = get_env()
    near = get_near()
    logger: Logger = get_logger()
    
    # 'headless' or None
    if signing_mode() != "headless":
        env.add_reply(
            "⚠️ No signing keys available. Add `NEAR_ACCOUNT_ID` and "
            "`NEAR_PRIVATE_KEY` to secrets, then try again."
        )
        return
    
    # Parse amount (NEAR → yocto)
    try:
        yocto = int((Decimal(amount) * YOCTO_FACTOR).quantize(Decimal("1")))
    except Exception:
        env.add_reply(f"❌ Invalid amount: {amount!r}")
        return
    
    try:
        # Perform the payable undelegate call with 1 yoctoNEAR attached        
        response: TransactionResult = run_coroutine(
            near.call(
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
            env.add_reply(
                "❌ Undelegate failed with **contract panic**:\n\n"
               f"> {json.dumps(failure, indent=2)}"
            )
            return
        
        # Extract only the primitive fields we care about
        tx_hash  = response.transaction.hash
        gas_tgas = response.transaction_outcome.gas_burnt / 1e12
        explorer = get_explorer_url()
        
        env.add_reply(
            "✅ **Undelegation Successful**\n"
            f"Vault [`{vault_id}`]({explorer}/accounts/{vault_id}) undelegated "
            f"**{amount} NEAR** from `{validator}`.\n"
            f"🔹 **Transaction Hash**: "
            f"[`{tx_hash}`]({explorer}/transactions/{tx_hash})\n"
            f"⛽ **Gas Burned**: {gas_tgas:.2f} Tgas"
        )
    
    except Exception as e:
        logger.error(
            "undelegate error %s ← %s (%s NEAR): %s",
            vault_id, validator, amount, e, exc_info=True
        )
        env.add_reply(
            f"❌ Undelegate failed for `{vault_id}` ← `{validator}` "
            f"({amount} NEAR)\n\n**Error:** {e}"
        )