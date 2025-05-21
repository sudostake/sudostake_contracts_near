from decimal import Decimal
from logging import Logger
from .context import get_env, get_near, get_logger
from helpers import YOCTO_FACTOR, run_coroutine

def view_vault_status_with_validator(vault_id: str, validator_id: str) -> None:
    """
    Query the `get_account` view method on a validator contract to get vault staking info.

    Shows:
      - Staked balance
      - Unstaked balance
      - Withdrawable status
    """
    
    env = get_env()
    near = get_near()
    logger: Logger = get_logger()
    
    try:
        response = run_coroutine(
            near.view(
                contract_id=validator_id,
                method_name="get_account",
                args={"account_id": vault_id}
            )
        )
        
        if not response or not hasattr(response, "result") or response.result is None:
            env.add_reply(f"❌ No data returned for `{vault_id}` at validator `{validator_id}`.")
            return
        
        data = response.result
        staked = Decimal(data["staked_balance"]) / YOCTO_FACTOR
        unstaked = Decimal(data["unstaked_balance"]) / YOCTO_FACTOR
        can_withdraw = "✅ Yes" if data["can_withdraw"] else "❌ No"
        
        env.add_reply(
            f"📊 **Delegation Status** for `{vault_id}` at `{validator_id}`\n\n"
            f"- **Staked Balance**: {staked:.5f} NEAR\n"
            f"- **Unstaked Balance**: {unstaked:.5f} NEAR\n"
            f"- **Withdrawable Now**: {can_withdraw}"
        )
        
    except Exception as e:
        logger.error("view_vault_status_with_validator error: %s", e, exc_info=True)
        env.add_reply(
            f"❌ Failed to get delegation status for `{vault_id}` at `{validator_id}`\n\n**Error:** {e}"
        )
