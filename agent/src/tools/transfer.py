from decimal import Decimal
from logging import Logger
from .context import get_env, get_near, get_logger
from helpers import YOCTO_FACTOR, signing_mode, run_coroutine, get_explorer_url
from py_near.models import TransactionResult

def transfer_near_to_vault(vault_id: str, amount: str) -> None:
    """
    Transfer `amount` NEAR from the main wallet to `vault_id`.

    • Head-less signing required (NEAR_ACCOUNT_ID & NEAR_PRIVATE_KEY).
    • Uses py-near `send_money` (amount must be in yocto).
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
        tx: TransactionResult = run_coroutine(
            near.send_money(account_id=vault_id, amount=yocto)
        )
        
        tx_hash  = tx.transaction.hash
        explorer = get_explorer_url()
        
        env.add_reply(
            "💸 **Transfer Submitted**\n"
            f"Sent **{Decimal(amount):.5f} NEAR** to `{vault_id}`.\n"
            f"🔹 Tx: [{tx_hash}]({explorer}/transactions/{tx_hash})"
        )
        
    except Exception as e:
        logger.error(
            "transfer_near_to_vault error → %s (%s NEAR): %s",
            vault_id, amount, e, exc_info=True
        )
        env.add_reply(
            f"❌ Transfer failed for `{vault_id}` ({amount} NEAR)\n\n**Error:** {e}"
        )