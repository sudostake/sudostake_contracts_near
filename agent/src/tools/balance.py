from decimal import Decimal
from logging import Logger

from .context import get_env, get_near, get_logger
from helpers import (
    YOCTO_FACTOR,
    USDC_FACTOR,
    signing_mode, account_id, run_coroutine, usdc_contract
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
        usdc_contract_address = usdc_contract()
        resp = run_coroutine(
            near.view(usdc_contract_address, "ft_balance_of", {"account_id": acct_id})
        )
        
        if not resp or not hasattr(resp, "result") or resp.result is None:
            env.add_reply(f"❌ No USDC balance returned for `{acct_id}`.")
            return
        
        usdc_raw = int(resp.result)
        usdc_amount = Decimal(usdc_raw) / USDC_FACTOR
        
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
    Return the available NEAR balance in a readable sentence.

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
        
        env.add_reply(f"💰 Vault `{vault_id}` has **{near_amount:.5f} NEAR** available for withdrawal.")
    except Exception as e:
        logger.error("view_available_balance RPC error for %s: %s", vault_id, e, exc_info=True)
        env.add_reply(f"❌ Failed to fetch balance for `{vault_id}`\n\n**Error:** {e}")