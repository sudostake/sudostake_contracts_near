import json
import os
import requests

from decimal import Decimal
from logging import Logger
from .context import get_env, get_near, get_logger
from helpers import (
    YOCTO_FACTOR,
    VAULT_MINT_FEE_NEAR,
    FACTORY_CONTRACTS,
    FIREBASE_VAULTS_API,
    signing_mode,
    run_coroutine,
    get_failure_message_from_tx_status,
    get_explorer_url,
)
from py_near.models import TransactionResult

def mint_vault() -> None:
    """
    Mint a new SudoStake vault.

    • Head-less signing required (NEAR_ACCOUNT_ID + NEAR_PRIVATE_KEY).  
    • Uses the fixed 10 NEAR fee ( `VAULT_MINT_FEE_NEAR` ).  
    • Factory account is derived from `NEAR_NETWORK`.
    """
    
    env = get_env()
    near = get_near()
    logger: Logger = get_logger()
    
    # 'headless' or None
    if signing_mode() != "headless":
        env.add_reply(
            "⚠️ I can't sign transactions in this session.\n "
            "Add `NEAR_ACCOUNT_ID` and `NEAR_PRIVATE_KEY` to your run's "
            "secrets, then try again."
        )
        return
    
    # Prepare call params
    factory_id = FACTORY_CONTRACTS[os.getenv("NEAR_NETWORK")]
    yocto_fee  = int((VAULT_MINT_FEE_NEAR * YOCTO_FACTOR).quantize(Decimal('1')))
    
    try:
        # Perform the payable delegate call with yocto_fee attached
        response: TransactionResult = run_coroutine(
            near.call(
                contract_id=factory_id,
                method_name="mint_vault",
                args={},
                gas=300_000_000_000_000,        # 300 Tgas
                amount=yocto_fee,               # 10 NEAR in yocto
            )
        )
        
        # Inspect execution outcome for Failure / Panic
        failure = get_failure_message_from_tx_status(response.status)
        if failure:
            env.add_reply(
                "❌ Mint vault failed with **contract panic**:\n\n"
                f"> {failure}"
            )
            return
        
        # Extract tx_hash from the response
        tx_hash  = response.transaction.hash
        explorer = get_explorer_url()
        
        # Extract new vault account from EVENT_JSON log
        vault_acct = None
        for log in response.logs:
            if log.startswith("EVENT_JSON:"):
                payload = json.loads(log.split("EVENT_JSON:")[1])
                if payload.get("event") == "vault_minted":
                    vault_acct = payload["data"]["vault"]
                    break
            
        if vault_acct is None:
            raise RuntimeError("vault_minted log not found in transaction logs")
        
        # Index the new vault to Firebase
        try:
            idx_url = f"{FIREBASE_VAULTS_API}/index_vault"
            resp    = requests.post(
                idx_url,
                json={"vault": vault_acct},
                timeout=10,
                headers={"Content-Type": "application/json"},
            )
            resp.raise_for_status()
        except Exception as e:
            # Warn in logs only; minting still succeeded
            logger.warning("index_vault error: %s", e, exc_info=True)
        
        env.add_reply(
            "🏗️ **Vault Minted**\n"
            f"🔑 Vault account: [`{vault_acct}`]({explorer}/accounts/{vault_acct})\n"
            f"🔹 Tx: [{tx_hash}]({explorer}/transactions/{tx_hash})"
        )
    
    except Exception as e:
        logger.error("mint_vault error: %s", e, exc_info=True)
        env.add_reply(f"❌ Vault minting failed\n\n**Error:** {e}")