import os
import requests
import textwrap

from typing import List, cast
from logging import Logger
from .context import get_env, get_near, get_logger
from helpers import (
    FIREBASE_VAULTS_API,
    FACTORY_CONTRACTS,
    signing_mode,
    account_id,
    run_coroutine
)

def show_help_menu() -> None:
    """
    Display a list of supported commands the agent can respond to.
    This is shown when the user types `help`.
    """
    
    help_text = textwrap.dedent("""
        🛠 **SudoStake Agent Commands**

        __Vaults__
        • mint vault  
        • view state for <vault>  
        • view available balance in <vault>  
        • transfer <amount> to <vault>  
        • withdraw <amount> from <vault>  
        • withdraw <amount> from <vault> to <receiver>  
        • show my vaults  

        __Staking__
        • delegate <amount> to <validator> from <vault>  
        • undelegate <amount> from <validator> for <vault>  
        • claim unstaked balance from <validator> for <vault>  
        • show delegation summary for <vault>  
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


def view_user_vaults() -> None:
    """
    List every SudoStake vault owned by the *current* head-less signer.

    • Requires `NEAR_ACCOUNT_ID` + `NEAR_PRIVATE_KEY` in secrets  
    • Uses `$NEAR_NETWORK` to resolve the factory contract  
    • Calls the Firebase Cloud Function:  get_user_vaults
    """
    
    env = get_env()
    log = get_logger()
    
    # 'headless' or None
    if signing_mode() != "headless":
        env.add_reply(
            "⚠️ No signing keys available. Add `NEAR_ACCOUNT_ID` and "
            "`NEAR_PRIVATE_KEY` to secrets, then try again."
        )
        return
    
    # Get the signer's account id
    acct_id = account_id()
    
    # Resolve factory for the active network
    network    = os.getenv("NEAR_NETWORK")
    factory_id = FACTORY_CONTRACTS.get(network)
    
    # Call the Firebase Cloud Function to get vaults
    url = (
        f"{FIREBASE_VAULTS_API}/get_user_vaults"
        f"?owner={acct_id}&factory_id={factory_id}"
    )
    
    try:
        resp    = requests.get(url, timeout=10)
        resp.raise_for_status()
        vaults: List[str] = cast(List[str], resp.json())
        
        if not vaults:
            env.add_reply(f"🔍 No vaults found for `{acct_id}` on **{network}**.")
            return
        
        count  = len(vaults)
        plural = "" if count == 1 else "s"
        lines  = "\n".join(f"- {v}" for v in vaults)
        
        env.add_reply(
            f"**You have {count} vault{plural} in total**\n{lines}"
        )
    
    except Exception as e:
        log.error("view_user_vaults error for %s: %s", acct_id, e, exc_info=True)
        env.add_reply(f"❌ Failed to fetch vault list\n\n**Error:** {e}")

