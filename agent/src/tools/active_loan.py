import json

from logging import Logger
from .context import get_env, get_near, get_logger
from py_near.models import TransactionResult
from helpers import (
    run_coroutine,
    get_explorer_url,
    log_contains_event,
    get_failure_message_from_tx_status,
    index_vault_to_firebase,
)


def repay_loan(vault_id: str) -> None:
    """
    Repay an active SudoStake loan for the given vault.

    This performs the following:
    - Calls `repay_loan` on the vault contract with 1 yoctoNEAR.
    - Checks for contract panics or `repay_loan_failed` events.
    - Indexes the vault to Firebase.
    - Responds with a success message and explorer link if successful.
    """
    
    env = get_env()
    near = get_near()
    logger: Logger = get_logger()
    
    try:
        tx: TransactionResult = run_coroutine(
            near.call(
                contract_id=vault_id,
                method_name="repay_loan",
                args={},
                gas=300_000_000_000_000,  # 300 Tgas
                amount=1,                 # 1 yoctoNEAR deposit
            )
        )
        
        # Contract panic?
        failure = get_failure_message_from_tx_status(tx.status)
        if failure:
            env.add_reply(
                "❌ Loan repayment failed due to contract panic:\n\n"
                f"> {json.dumps(failure, indent=2)}"
            )
            return
        
        # Check for log error
        if log_contains_event(tx.logs, "repay_loan_failed"):
            env.add_reply(
                "❌ Loan repayment failed. Funds could not be transferred to the lender."
            )
            return
        
        # Index the updated vault
        try:
            index_vault_to_firebase(vault_id)
        except Exception as e:
            logger.warning("index_vault_to_firebase failed: %s", e, exc_info=True)
        
        explorer = get_explorer_url()
        env.add_reply(
            f"✅ **Loan Repaid Successfully**\n"
            f"- 🏦 Vault: [`{vault_id}`]({explorer}/accounts/{vault_id})\n"
            f"- 🔗 Tx: [{tx.transaction.hash}]({explorer}/transactions/{tx.transaction.hash})"
        )
    except Exception as e:
        logger.error("repay_loan failed: %s", e, exc_info=True)
        env.add_reply(f"❌ Unexpected error during loan repayment\n\n**Error:** {e}")
