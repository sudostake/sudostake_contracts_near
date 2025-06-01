import json
import requests

from decimal import Decimal
from logging import Logger
from typing import List, TypedDict
from .context import get_env, get_near, get_logger
from token_registry import get_token_metadata, get_token_metadata_by_contract
from py_near.models import TransactionResult
from helpers import (
    YOCTO_FACTOR,
    get_factory_contract,
    index_vault_to_firebase,
    run_coroutine, 
    get_explorer_url, 
    log_contains_event,
    get_failure_message_from_tx_status,
    firebase_vaults_api,
    account_id,
    signing_mode
)

# Define the structure of the liquidity request
class LiquidityRequest(TypedDict):
    token: str
    amount: str
    interest: str
    collateral: str
    duration: int

# Define the structure of an accepted offer
class AcceptedOffer(TypedDict):
    lender: str
    accepted_at: str

# Define the structure of a pending liquidity request
class PendingRequest(TypedDict):
    id: str
    owner: str
    state: str
    liquidity_request: LiquidityRequest

# Define the structure of an active lending request
class ActiveRequest(TypedDict):
    id: str
    owner: str
    state: str
    liquidity_request: LiquidityRequest
    accepted_offer: AcceptedOffer
    

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
    
    # 'headless' or None
    if signing_mode() != "headless":
        env.add_reply(
            "⚠️ No signing keys available. Add `NEAR_ACCOUNT_ID` and "
            "`NEAR_PRIVATE_KEY` to secrets, then try again."
        )
        return
    
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
        factory_id = get_factory_contract()
        
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


def accept_liquidity_request(vault_id: str) -> None:
    """
    Accept a pending liquidity request on the given vault by sending the
    required amount of tokens via `ft_transfer_call`.

    Args:
        vault_id (str): NEAR account ID of the vault (e.g., vault-0.factory.testnet)
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
        
        req = state.get("liquidity_request")
        offer = state.get("accepted_offer")
        
        if offer or not req:
            env.add_reply(
                f"❌ `{vault_id}` has no active liquidity request or it has already been accepted."
            )
            return
        
        msg_payload = {
            "action": "AcceptLiquidityRequest",
            "token": req["token"],
            "amount": req["amount"],
            "interest": req["interest"],
            "collateral": req["collateral"],
            "duration": req["duration"],
        }
        
        token_contract = req["token"]
        token_amount = req["amount"]
        
        # Send ft_transfer_call
        tx: TransactionResult = run_coroutine(
            near.call(
                contract_id=token_contract,
                method_name="ft_transfer_call",
                args={
                    "receiver_id": vault_id,
                    "amount": token_amount,
                    "msg": json.dumps(msg_payload),
                },
                gas=300_000_000_000_000,  # 300 TGas
                amount=1,                 # 1 yoctoNEAR deposit
            )
        )
        
        failure = get_failure_message_from_tx_status(tx.status)
        if failure:
            env.add_reply(
                f"❌ Failed to accept liquidity request\n\n> {json.dumps(failure, indent=2)}"
            )
            return
        
        # Index the vault to Firebase
        try:
            index_vault_to_firebase(vault_id)
        except Exception as e:
            logger.warning("index_vault_to_firebase failed: %s", e, exc_info=True)
        
        # Get the token metadata
        token_meta = get_token_metadata_by_contract(token_contract)
        decimals = token_meta["decimals"]
        symbol = token_meta["symbol"]
        token_amount = (Decimal(token_amount) / Decimal(10 ** decimals)).quantize(Decimal(1))

        explorer = get_explorer_url()
        env.add_reply(
            f"✅ **Accepted Liquidity Request**\n"
            f"- 🏦 Vault: [`{vault_id}`]({explorer}/accounts/{vault_id})\n"
            f"- 🪙 Token: `{token_contract}`\n"
            f"- 💵 Amount: `{token_amount}` {symbol}\n"
            f"- 🔗 Tx: [{tx.transaction.hash}]({explorer}/transactions/{tx.transaction.hash})"
        )
    
    except Exception as e:
        logger.error("accept_liquidity_request failed: %s", e, exc_info=True)
        env.add_reply(f"❌ Error while accepting liquidity request:\n\n**{e}**")


def view_lender_positions() -> None:
    """
    Display all vaults where the current user is the lender (i.e., has an active accepted_offer).
    """
    
    env = get_env()
    logger = get_logger()
    
    # 'headless' or None
    if signing_mode() != "headless":
        env.add_reply(
            "⚠️ No signing keys available. Add `NEAR_ACCOUNT_ID` and "
            "`NEAR_PRIVATE_KEY` to secrets, then try again."
        )
        return
    
    try:
        lender_id = account_id()
        factory_id = get_factory_contract()
        
        # Construct the query to Firebase API
        url = f"{firebase_vaults_api()}/view_lender_positions"
        response = requests.get(
            url,
            params={"factory_id": factory_id, "lender_id": lender_id},
            timeout=10,
            headers={"Content-Type": "application/json"},
        )
        response.raise_for_status()
        
        vaults: List[ActiveRequest] = response.json()
        if not vaults:
            env.add_reply("✅ You have no active lending positions.")
            return
        
        message = f"**📄 Active Lending Positions for `{lender_id}`**\n\n"
        for v in vaults:
            state = v.get("accepted_offer")
            req = v.get("liquidity_request")
            token_meta = get_token_metadata_by_contract(req["token"])
            decimals = token_meta["decimals"]
            symbol = token_meta["symbol"]
            
            amount = (Decimal(req["amount"]) / Decimal(10 ** decimals)).quantize(Decimal(1))
            interest = (Decimal(req["interest"]) / Decimal(10 ** decimals)).quantize(Decimal(1))
            collateral = (Decimal(req["collateral"]) / YOCTO_FACTOR).quantize(Decimal(1))
            duration_days = req["duration"] // 86400
            
            message += (
                f"- 🏦 `{v['id']}`\n"
                f"  • Token: `{req['token']}`\n"
                f"  • Amount: `{amount}` {symbol}\n"
                f"  • Interest: `{interest}` {symbol}\n"
                f"  • Duration: `{duration_days} days`\n"
                f"  • Collateral: `{collateral}` NEAR\n"
                f"  • Accepted At: `{state['accepted_at']}`\n\n"
            )
            
        env.add_reply(message)
        
    except Exception as e:
        logger.warning("view_lender_positions failed: %s", e, exc_info=True)
        env.add_reply(f"❌ Failed to fetch lending positions\n\n**Error:** {e}")
