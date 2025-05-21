from decimal import Decimal
from logging import Logger
from .context import get_env, get_near, get_logger
from helpers import YOCTO_FACTOR, signing_mode, run_coroutine, get_failure_message_from_tx_status, get_explorer_url
from py_near.models import TransactionResult

def withdraw_balance(vault_id: str, amount: str, to_address: str) -> None:
    """
    Withdraw `amount` NEAR from `vault_id`. Optionally, send to a third-party `to_address`.

    • Only works in headless mode.
    • Uses 1 yoctoNEAR for call.
    • Calls the `withdraw_balance` method on the vault contract.
    """
    
    env = get_env()
    near = get_near()
    logger: Logger = get_logger()
    
    # 'headless' or None
    if signing_mode() != "headless":
        env.add_reply(
            "⚠️ I can't sign transactions in this session.\n"
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
        args = {
            "amount": str(yocto)
        }
        
        if to_address:
            args["to"] = to_address
        
        # Perform the payable withdraw_balance call with 1 yoctoNEAR attached  
        response: TransactionResult = run_coroutine(
            near.call(
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
            env.add_reply(
                "❌ Withdraw failed with **contract panic**:\n\n"
                f"> {failure}"
            )
            return
        
        # Extract only the primitive fields we care about
        tx_hash = response.transaction.hash
        explorer = get_explorer_url()
        
        env.add_reply(
            "✅ **Withdrawal Successful**\n"
            f"Vault [`{vault_id}`]({explorer}/accounts/{vault_id}) withdrew "
            f"**{amount} NEAR**"
            + (f" to `{to_address}`." if to_address else " to your main account.") +
            f"\n🔹 [View Tx]({explorer}/transactions/{tx_hash})"
        )
        
    except Exception as e:
        logger.error(
            "withdraw_balance error %s → %s (%s NEAR): %s",
            vault_id, to_address, amount, e, exc_info=True
        )
        env.add_reply(f"❌ Withdraw failed for `{vault_id}` → `{to_address or 'self'}`\n\n**Error:** {e}")


def claim_unstaked_balance(vault_id: str, validator: str) -> None:
    """
    Call the `claim_unstaked` method on the vault to withdraw matured unstaked NEAR.

    • Must be the vault owner.
    • Requires 1 yoctoNEAR.
    • Will only succeed if the unstaked balance is available.
    """
    
    env = get_env()
    near = get_near()
    logger: Logger = get_logger()
    
    # 'headless' or None
    if signing_mode() != "headless":
        env.add_reply(
            "⚠️ I can't sign transactions in this session.\n"
            "Add `NEAR_ACCOUNT_ID` and `NEAR_PRIVATE_KEY` to your run's "
            "secrets, then try again."
        )
        return
    
    try:
        response: TransactionResult = run_coroutine(
            near.call(
                contract_id=vault_id,
                method_name="claim_unstaked",
                args={"validator": validator},
                gas=300_000_000_000_000,
                amount=1  # 1 yoctoNEAR
            )
        )
        
        failure = get_failure_message_from_tx_status(response.status)
        if failure:
            env.add_reply(
                f"❌ Claim failed with **contract panic**:\n\n> {failure}"
            )
            return
        
        tx_hash = response.transaction.hash
        explorer = get_explorer_url()
        
        env.add_reply(
            "📥 **Claim Initiated**\n"
            f"Vault `{vault_id}` is claiming matured unstaked NEAR from `{validator}`.\n"
            f"🔹 [View Tx]({explorer}/transactions/{tx_hash})"
        )
    
    except Exception as e:
        logger.error("claim_unstaked_balance error: %s", e, exc_info=True)
        env.add_reply(
            f"❌ Failed to claim unstaked NEAR from `{validator}` for `{vault_id}`\n\n**Error:** {e}"
        )
