import json

from decimal import Decimal
from logging import Logger
from .context import get_env, get_near, get_logger
from helpers import run_coroutine

def show_help_menu() -> None:
    """Send a concise list of available SudoStake tools."""
    
    get_env().add_reply(
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


def vault_state(vault_id: str) -> None:
    """
    Fetch the on-chain state for `vault_id` and send it to the user.

    Args:
      vault_id: NEAR account ID of the vault.
    """
    
    env = get_env()
    near = get_near()
    logger: Logger = get_logger()

    try:
        response = run_coroutine(near.view(vault_id, "get_vault_state", {}))
        if not response or not hasattr(response, "result") or response.result is None:
            env.add_reply(f"❌ No data returned for `{vault_id}`. Is the contract deployed?")
            return
        
        # Get the result state from the response
        state = response.result
        print(json.dumps(state, indent=2))
        
        env.add_reply(
            f"✅ **Vault State: `{vault_id}`**\n\n"
            f"| Field                  | Value                       |\n"
            f"|------------------------|-----------------------------|\n"
            f"| Owner                  | `{state['owner']}`          |\n"
            f"| Index                  | `{state['index']}`          |\n"
            f"| Version                | `{state['version']}`        |\n"
            f"| Listed for Takeover    | `{state['is_listed_for_takeover']}` |\n"
            f"| Active Request         | `{state['liquidity_request']}` |\n"
            f"| Accepted Offer         | `{state['accepted_offer']}` |\n"
        )
    except Exception as e:
        logger.error("vault_state RPC error for %s: %s", vault_id, e, exc_info=True)
        env.add_reply(f"❌ Failed to fetch vault state for `{vault_id}`\n\n**Error:** {e}")
