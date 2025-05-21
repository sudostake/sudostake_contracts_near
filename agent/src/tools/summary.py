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


def vault_delegation_summary(vault_id: str) -> None:
    """
    Show a summary of delegation for the given vault by combining:
    • Active validators
    • Unstake entries
    • Real-time status from each validator via get_account(vault_id)
    """
    
    env = get_env()
    near = get_near()
    logger = get_logger()
    
    try:
        # Fetch vault state
        response = run_coroutine(near.view(vault_id, "get_vault_state", {}))
        state = response.result
        
        current_epoch = state["current_epoch"]
        unstake_entries = dict(state["unstake_entries"])
        active_validators = set(state["active_validators"])
        unstake_validators = set(unstake_entries.keys())
        
        # Union of all validator accounts involved
        all_validators = sorted(active_validators.union(unstake_validators))
        
        summary = []
        
        for validator in all_validators:
            try:
                result = run_coroutine(
                    near.view(validator, "get_account", {"account_id": vault_id})
                ).result
                
                staked = Decimal(result["staked_balance"]) / YOCTO_FACTOR
                unstaked = Decimal(result["unstaked_balance"]) / YOCTO_FACTOR
                can_withdraw = result["can_withdraw"]
                
                entry = {
                    "validator": validator,
                    "staked_balance": f"{staked:.5f} NEAR",
                    "unstaked_balance": f"{unstaked:.5f} NEAR",
                    "can_withdraw": can_withdraw
                }
                
                if not can_withdraw:
                    unstaked_at = unstake_entries.get(validator, {}).get("epoch_height")
                    entry["unstaked_at"] = unstaked_at
                    entry["current_epoch"] = current_epoch
                    
                summary.append(entry)
                
            except Exception as e:
                logger.warning("Failed to fetch get_account for %s: %s", validator, e)
                summary.append({
                    "validator": validator,
                    "error": str(e)
                })
            
        if not summary:
            env.add_reply("⚠️ No delegation data found.")
            return
        
        # Format the summary for display
        lines = ["📊 **Vault Delegation Summary**", f"Vault: `{vault_id}`", ""]
        
        for item in summary:
            lines.append(f"Validator: `{item['validator']}`")

            if "error" in item:
                lines.append(f"  ⛔ Error: `{item['error']}`")
                lines.append("")  # spacing between validators
                continue

            lines.append(f"  Staked:         **{item['staked_balance']}**")
            lines.append(f"  Unstaked:       **{item['unstaked_balance']}**")
            lines.append(f"  Can Withdraw:   {'✅ Yes' if item['can_withdraw'] else '❌ No'}")

            if not item["can_withdraw"] and "unstaked_at" in item:
                lines.append(f"  Unlocks at:     `{item['unstaked_at']}`")
                lines.append(f"  Current Epoch:  `{item['current_epoch']}`")

            lines.append("")  # blank line between validators
                
        env.add_reply("\n".join(lines).strip())
        
    except Exception as e:
        logger.error("vault_delegation_summary error for %s: %s", vault_id, e, exc_info=True)
        env.add_reply(
            f"❌ Failed to fetch delegation summary for `{vault_id}`\n\n**Error:** {e}"
        )
