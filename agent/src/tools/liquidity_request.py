import json
import os
import requests

from decimal import Decimal
from logging import Logger
from typing import List, TypedDict
from .context import get_env, get_near, get_logger
from token_registry import get_token_metadata, get_token_metadata_by_contract
from py_near.models import TransactionResult
from helpers import (
    YOCTO_FACTOR,
    FACTORY_CONTRACTS,
    index_vault_to_firebase,
    run_coroutine, 
    get_explorer_url, 
    log_contains_event,
    get_failure_message_from_tx_status,
    firebase_vaults_api
)

# Define the structure of the liquidity request
class LiquidityRequest(TypedDict):
    token: str
    amount: str
    interest: str
    collateral: str
    duration: int

# Define the structure of a pending liquidity request
class PendingRequest(TypedDict):
    id: str
    owner: str
    state: str
    liquidity_request: LiquidityRequest


def request_liquidity(
    vault_id: str,
    amount: int,
    denom: str,
    interest: int,
    duration: int,
    collateral: int,
) -> None:
    """
    Open a SudoStake liquidity request using staked NEAR as collateral.

    Parameters:
    - vault_id (str): Vault account ID (e.g., "vault-0.factory.testnet")
    - amount (int): Requested loan amount
    - denom (str): The requested token denomination (e.g., "usdc")
    - interest (int): Interest in same denomination as amount (e.g., 50)
    - duration (int): Duration in days (e.g., 30)
    - collateral (int): Collateral in NEAR (e.g., 100)
    """
    
    env = get_env()
    near = get_near()
    logger: Logger = get_logger()
    
    try:
        # Parse amount and resolve token
        token_meta = get_token_metadata(denom.strip().lower())
        
        # Scale amount using token decimals
        amount_scaled = int((Decimal(amount) * (10 ** token_meta["decimals"])).quantize(Decimal("1")))
        
        # Scale interest using same token decimals
        interest_scaled = int((Decimal(interest) * (10 ** token_meta["decimals"])).quantize(Decimal("1")))
        
        # Convert NEAR collateral to yocto
        collateral_yocto = int((Decimal(collateral) * YOCTO_FACTOR).quantize(Decimal("1")))
        
        # Convert duration to seconds
        duration_secs = duration * 86400
        
        # Prepare the transaction arguments 
        args = {
            "token": token_meta["contract"],
            "amount": str(amount_scaled),
            "interest": str(interest_scaled),
            "collateral": str(collateral_yocto),
            "duration": duration_secs,
        }
        
        # Perform the liquidity request call with 1 yoctoNEAR attached
        response: TransactionResult = run_coroutine(
            near.call(
                contract_id=vault_id,
                method_name="request_liquidity",
                args=args,
                gas=300_000_000_000_000,  # 300 TGas
                amount=1,                 # 1 yoctoNEAR deposit
            )
        )
        
        # Catch any panic errors
        failure = get_failure_message_from_tx_status(response.status)
        if failure:
            env.add_reply(
                "❌ Liquidity Request failed with **contract panic**:\n\n"
                f"> {json.dumps(failure, indent=2)}"
            )
            return
        
        # Inspect the logs for event : liquidity_request_failed_insufficient_stake
        if log_contains_event(response.logs, "liquidity_request_failed_insufficient_stake"):
            env.add_reply(
                "❌ Liquidity Request failed\n"
                "> You may not have enough staked NEAR to cover the collateral."
            )
            return
        
        # Index the vault to Firebase
        try:
            index_vault_to_firebase(vault_id)
        except Exception as e:
            logger.warning("index_vault_to_firebase failed: %s", e, exc_info=True)
        
        explorer = get_explorer_url()
        env.add_reply(
            f"💧 **Liquidity Request Submitted**\n"
            f"- 🏦 Vault: [`{vault_id}`]({explorer}/accounts/{vault_id})\n"
            f"- 💵 Amount: `{amount}` ({token_meta['symbol']})\n"
            f"- 📈 Interest: `{interest}` {token_meta['symbol']}\n"
            f"- ⏳ Duration: `{duration}` days\n"
            f"- 💰 Collateral: `{collateral}` NEAR\n"
            f"- 🔗 Tx: [{response.transaction.hash}]({explorer}/transactions/{response.transaction.hash})"
        )
        
    except Exception as e:
        env.add_reply(f"❌ Liquidity request failed\n\n**Error:** {e}")


def view_pending_liquidity_requests() -> None:
    """
    Display all pending liquidity requests from the Firebase index
    for vaults minted under the current network's factory contract.
    """
    
    env = get_env()
    logger = get_logger()
    
    try:
        # Resolve factory for the active network
        network = os.getenv("NEAR_NETWORK")
        factory_id = FACTORY_CONTRACTS.get(network)
        
        url = f"{firebase_vaults_api()}/view_pending_liquidity_requests"
        response = requests.get(
            url,
            params={"factory_id": factory_id},
            timeout=10,
            headers={"Content-Type": "application/json"},
        )
        response.raise_for_status()
        
        pending: List[PendingRequest] = response.json()
        
        if not pending:
            env.add_reply("✅ No pending liquidity requests found.")
            return

        message = "**📋 Pending Liquidity Requests**\n\n"
        for item in pending:
            lr = item["liquidity_request"]
            token_meta = get_token_metadata_by_contract(lr["token"])
            decimals = token_meta["decimals"]
            symbol = token_meta["symbol"]
            
            amount = (Decimal(lr["amount"]) / Decimal(10 ** decimals)).quantize(Decimal(1))
            interest = (Decimal(lr["interest"]) / Decimal(10 ** decimals)).quantize(Decimal(1))
            collateral = Decimal(lr["collateral"]) / YOCTO_FACTOR
            duration_days = lr["duration"] // 86400
            
            message += (
                f"- 🏦 `{item['id']}`\n"
                f"  • Token: `{lr['token']}`\n"
                f"  • Amount: `{amount}` {symbol}\n"
                f"  • Interest: `{interest}` {symbol}\n"
                f"  • Duration: `{duration_days} days`\n"
                f"  • Collateral: `{collateral.normalize()}` NEAR\n\n"
            )
        
        env.add_reply(message)
            
        
    except Exception as e:
        logger.warning("view_pending_liquidity_requests failed: %s", e, exc_info=True)
        env.add_reply(f"❌ Failed to fetch pending liquidity requests\n\n**Error:** {e}")
