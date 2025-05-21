import textwrap
from logging import Logger
from .context import get_env, get_near, get_logger
from helpers import run_coroutine

def show_help_menu() -> None:
    """
    Display a list of supported commands the agent can respond to.
    This is shown when the user types `help`.
    """
    
    help_text = textwrap.dedent("""
        🛠 **SudoStake Agent Commands**

        __Vaults__
        • mint vault  
        • view vault state for <vault>  
        • view available balance in <vault>  
        • transfer <amount> to <vault>  
        • withdraw <amount> from <vault>  
        • withdraw <amount> from <vault> to <receiver>  

        __Staking__
        • delegate <amount> to <validator> from <vault>  
        • undelegate <amount> from <validator> in <vault>  
        • claim unstaked balance from <validator> for <vault>  
        • show vault delegation summary for <vault> ← 🆕  
        • show <vault> delegation status with <validator>  

        __Main Account__
        • what's my main account balance?

        _You can type any of these in plain English to get started._
    """)

    get_env().add_reply(help_text.strip())


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
