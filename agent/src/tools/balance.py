from decimal import Decimal
from logging import Logger

from .context import get_env, get_near, get_logger
from helpers import (
    YOCTO_FACTOR,
    fetch_usdc_balance,
    signing_mode, account_id, run_coroutine
)

def view_main_balance() -> None:
    """
    Show the NEAR and USDC balances of the user's main wallet (the account whose key
    is loaded for head-less mode).

    • Works only when `signing_mode() == "headless"`.
    • Replies are sent via `_env.add_reply()`; nothing is returned.
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
    
    # Get the signer's account id
    acct_id = account_id()
    
    try:
        # py_near.Account.get_balance() -> int with 'amount' in yocto
        yocto = run_coroutine(near.get_balance())
        near_bal = Decimal(yocto) / YOCTO_FACTOR
        
        # Fetch USDC balance
        try:
            usdc_amount = fetch_usdc_balance(near, acct_id)
        except ValueError as e:
            env.add_reply(str(e))
            return
        
        env.add_reply(
            f"💼 **Main Account Balance**\n"
            f"- **Account:** `{acct_id}`\n"
            f"- **NEAR:** `{near_bal:.5f}`\n"
            f"- **USDC:** `{usdc_amount:.2f}`"
        )
    
    except Exception as e:
        logger.error("view_main_balance error: %s", e, exc_info=True)
        env.add_reply(f"❌ Failed to fetch balance\n\n**Error:** {e}")


def view_available_balance(vault_id: str) -> None:
    """
    Return the available NEAR and USDC balances in a readable sentence.

    Args:
      vault_id: NEAR account ID of the vault.
    """
    
    env = get_env()
    near = get_near()
    logger: Logger = get_logger()
    
    try:
        # call the on-chain view method (contract should expose "view_available_balance")
        resp = run_coroutine(near.view(vault_id, "view_available_balance", {}))
        
        if not resp or not hasattr(resp, "result") or resp.result is None:
            env.add_reply(f"❌ No data returned for `{vault_id}`. Is the contract deployed?")
            return
        
        yocto = int(resp.result)
        near_amount = Decimal(yocto) / YOCTO_FACTOR
        
        # Fetch USDC balance
        try:
            usdc_amount = fetch_usdc_balance(near, vault_id)
        except ValueError as e:
            env.add_reply(str(e))
            return
        
        env.add_reply(
            f"💰 Vault `{vault_id}` balances:\n"
            f"- **NEAR:** `{near_amount:.5f}` available\n"
            f"- **USDC:** `{usdc_amount:.2f}`"
        )
    except Exception as e:
        logger.error("view_available_balance RPC error for %s: %s", vault_id, e, exc_info=True)
        env.add_reply(f"❌ Failed to fetch balance for `{vault_id}`\n\n**Error:** {e}")