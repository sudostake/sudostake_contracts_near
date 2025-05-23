# tests/test_tools.py
import sys
import os
import pytest
from decimal import Decimal
from unittest.mock import AsyncMock, MagicMock

# Make src/ importable
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "../src")))

import helpers # type: ignore
from tools import ( # type: ignore[import]
    context,
    balance,
    minting,
    delegation,
    transfer,
    vault,
    withdrawal,
    summary,
)


# ─────────────────────────── fixtures ───────────────────────────
@pytest.fixture
def headless_mode(monkeypatch):
    """Force helpers.signing_mode() → 'headless' for mutating tool tests."""
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    
    yield
    # cleanup (pytest will restore monkeypatch state automatically)
    
@pytest.fixture
def mock_setup():
    """Initialize mock environment, logger, and near — then set context."""
    env = MagicMock()
    near = MagicMock()

    # Set the context globally for tools
    context.set_context(env=env, near=near)

    return (env, near)


# ─────────────────────────── tests ──────────────────────────────
def test_vault_state(mock_setup):
    (dummy_env, mock_near) = mock_setup
    
    mock_near.view = AsyncMock(return_value=MagicMock(result={
        "owner": "alice.near",
        "index": 0,
        "version": 1,
        "is_listed_for_takeover": False,
        "pending_liquidity_request": None,
        "liquidity_request": None,
        "accepted_offer": None
    }))
    
    # Call the vault_state function
    vault.vault_state("vault-0.testnet")
    
    # Assertions
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "✅ **Vault State" in msg
    assert "`alice.near`" in msg
    

def test_view_available_balance(mock_setup):
    (dummy_env, mock_near) = mock_setup
    
    yocto_balance = int(Decimal("1.25") * Decimal("1e24"))
    mock_near.view = AsyncMock(return_value=MagicMock(result=str(yocto_balance)))
    
    balance.view_available_balance("vault-0.testnet")

    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "**1.25000 NEAR**" in msg
    

def test_delegate_headless(monkeypatch, mock_setup):
    """
    delegate() should succeed in head-less mode (secrets present) and
    embed the tx-hash plus success banner in the returned markdown.
    """
    
    (dummy_env, mock_near) = mock_setup
    
    mock_near.call = AsyncMock(return_value=MagicMock(
        transaction=MagicMock(hash="abc123"),
        transaction_outcome=MagicMock(gas_burnt=310_000_000_000_000),
        logs=[],
        status={}
    ))
    
    # Force helpers.signing_mode() → "headless"
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    
    # Needed for explorer link formatting
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Execute the delegate command
    result = delegation.delegate("vault-0.testnet", "validator.near", "1")
    
    # Assertions
    assert result is None
    dummy_env.add_reply.assert_called_once()
    
    # grab the message text and check key fragments
    msg = dummy_env.add_reply.call_args[0][0]
    assert "✅ **Delegation Successful**" in msg
    assert "abc123" in msg


def test_delegate_no_credentials(monkeypatch, mock_setup):
    """
    delegate() should refuse to sign when signing_mode != 'headless'
    and emit a single warning via _env.add_reply().
    """
    
    (dummy_env, _) = mock_setup
    
    # Force helpers.signing_mode() → None  (no creds, no wallet)
    monkeypatch.setattr(helpers, "_SIGNING_MODE", None, raising=False)
    
    # Ensures correct RPC call
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Call the tool
    result = delegation.delegate("vault-0.testnet", "validator.near", "1")
    
    # Assertions
    assert result is None                        # function returns nothing
    dummy_env.add_reply.assert_called_once()     # one warning sent

    warning = dummy_env.add_reply.call_args[0][0]
    assert "can't sign" in warning.lower()
    
    
def test_mint_vault_headless(monkeypatch, mock_setup):
    """
    mint_vault() should succeed when head-less credentials exist.
    It must push a single success message that contains:
      • the 'Vault Minted' banner
      • the new vault account id parsed from EVENT_JSON
      • the tx-hash link
    """
    
    (dummy_env, mock_near) = mock_setup
    
    # Pretend the chain call succeeded and emitted the standard macro log
    mock_near.call = AsyncMock(
        return_value=MagicMock(
            transaction=MagicMock(hash="tx123"),
            transaction_outcome=MagicMock(gas_burnt=1),
            logs=[
                'EVENT_JSON:{"event":"vault_minted",'
                '"data":{"vault":"vault-0.vaultmint.testnet"}}'
            ],
            status={}
        )
    )
    
    # Force head-less signing mode
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    
    # Select network → resolves factory_id internally
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
     # Run the tool
    minting.mint_vault()
    
    # Verify exactly one user reply with expected fragments
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Vault Minted" in msg
    assert "vault-0.vaultmint.testnet" in msg
    
    # Verify the expected tx_hash link
    expected_url = f"{helpers.get_explorer_url()}/transactions/tx123"
    assert expected_url in msg


def test_mint_vault_no_credentials(monkeypatch, mock_setup):
    """
    When signing is disabled, mint_vault() must:
      • NOT call _near.call()
      • emit exactly one warning via _env.add_reply()
      • return None
    """
    
    (dummy_env, mock_near) = mock_setup
    
    # No credentials / wallet → signing_mode == None
    monkeypatch.setattr(helpers, "_SIGNING_MODE", None, raising=False)
    
    # Network still needed for factory lookup
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Invoke tool
    result = minting.mint_vault()
    
    # Assertions
    assert result is None
    mock_near.call.assert_not_called()            # nothing signed
    dummy_env.add_reply.assert_called_once()
    assert "can't sign" in dummy_env.add_reply.call_args[0][0].lower()
    

def test_mint_vault_missing_event(monkeypatch, mock_setup):
    """
    If the tx logs do not include a vault_minted EVENT_JSON entry,
    mint_vault() must:
      • send a single error reply via _env.add_reply()
      • still return None
    """
    
    (dummy_env, mock_near) = mock_setup
    
    # Mock py_near.call() with NO event log
    mock_near.call = AsyncMock(
        return_value=MagicMock(
            transaction=MagicMock(hash="tx999"),
            transaction_outcome=MagicMock(gas_burnt=1),
            logs=[],          # ← no EVENT_JSON at all
            status={}
        )
    )
    
    # Head-less signing enabled
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Run the tool
    result = minting.mint_vault()
    
    # Assertions
    assert result is None
    mock_near.call.assert_called_once()           # call attempted
    dummy_env.add_reply.assert_called_once()      # one error message
    
    err_msg = dummy_env.add_reply.call_args[0][0].lower()
    assert "vault minting failed" in err_msg


def test_view_main_balance_headless(monkeypatch, mock_setup):
    """Head-less mode: tool should return the correct NEAR balance."""
    (dummy_env, mock_near) = mock_setup
    
    yocto_five = int(5 * 1e24)
    mock_near.get_balance = AsyncMock(return_value=yocto_five)

    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    monkeypatch.setattr(helpers, "_ACCOUNT_ID", "alice.testnet", raising=False)
    
    balance.view_main_balance()
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "**5.00000 NEAR**" in msg
    mock_near.get_balance.assert_awaited_once()
    

def test_view_main_balance_no_credentials(monkeypatch, mock_setup):
    """No signing keys: tool should warn and never call get_balance()."""
    (dummy_env, mock_near) = mock_setup
    monkeypatch.setattr(helpers, "_SIGNING_MODE", None, raising=False)
    
    balance.view_main_balance()
    
    dummy_env.add_reply.assert_called_once()
    warn = dummy_env.add_reply.call_args[0][0].lower()
    assert "no signing keys" in warn
    mock_near.get_balance.assert_not_called()


def test_transfer_near_headless(monkeypatch, mock_setup):
    """Head-less: transfer should call send_money and emit success reply."""
    
    (dummy_env, mock_near) = mock_setup
    
    # Mock the send_money call to simulate a successful transaction
    mock_near.send_money = AsyncMock(
        return_value=MagicMock(
            transaction=MagicMock(hash="tx456"),
            transaction_outcome=MagicMock(gas_burnt=1),
            logs=[],
        )
    )
    
    # Force helpers.signing_mode() → "headless"
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    
    # Set the network to testnet for explorer URL formatting
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Call the transfer_near function
    transfer.transfer_near_to_vault("vault-0.testnet", "3")
    
    # Assertions
    dummy_env.add_reply.assert_called_once()
    
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Transfer Submitted" in msg and "3.00000 NEAR" in msg
    assert "tx456" in msg
    
    mock_near.send_money.assert_awaited_once()
    

def test_transfer_near_no_creds(monkeypatch, mock_setup):
    """No signing keys: tool should warn and skip RPC call."""
    
    (dummy_env, mock_near) = mock_setup
    monkeypatch.setattr(helpers, "_SIGNING_MODE", None, raising=False)
    
    transfer.transfer_near_to_vault("vault-0.testnet", "2")
    
    # Assertions
    dummy_env.add_reply.assert_called_once()
    assert "no signing keys" in dummy_env.add_reply.call_args[0][0].lower()
    mock_near.send_money.assert_not_called()


def test_undelegate_headless(monkeypatch, mock_setup):
    """
    undelegate() should succeed in head-less mode (secrets present) and
    embed the tx-hash plus success banner in the returned markdown.
    """
    
    (dummy_env, mock_near) = mock_setup
    
    mock_near.call = AsyncMock(return_value=MagicMock(
        transaction=MagicMock(hash="abc123"),
        transaction_outcome=MagicMock(gas_burnt=310_000_000_000_000),
        logs=[],
        status={}
    ))
    
    # Force helpers.signing_mode() → "headless"
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    
    # Needed for explorer link formatting
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Execute the undelegate command
    result = delegation.undelegate("vault-0.testnet", "validator.near", "2")
    
    # Assertions
    assert result is None
    dummy_env.add_reply.assert_called_once()
    
    # grab the message text and check key fragments
    msg = dummy_env.add_reply.call_args[0][0]
    assert "✅ **Undelegation Successful**" in msg
    assert "2 NEAR" in msg
    assert "abc123" in msg
    
    
def test_undelegate_no_credentials(monkeypatch, mock_setup):
    """
    undelegate() should refuse to sign when signing_mode != 'headless'
    and emit a single warning via _env.add_reply().
    """
    
    (dummy_env, mock_near) = mock_setup
    
    # Force helpers.signing_mode() → None  (no creds, no wallet)
    monkeypatch.setattr(helpers, "_SIGNING_MODE", None, raising=False)
    
    # Ensures correct RPC call
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Execute the undelegate command
    result = delegation.undelegate("vault-0.testnet", "validator.near", "2")
    
    # Assertions
    assert result is None                        # function returns nothing
    dummy_env.add_reply.assert_called_once()     # one warning sent
    
    warning = dummy_env.add_reply.call_args[0][0]
    assert "no signing keys" in warning.lower()
    mock_near.call.assert_not_called()
    

def test_withdraw_balance_self(monkeypatch, mock_setup):
    """Withdraw NEAR to the vault's own account (no to_address)."""
    
    (dummy_env, mock_near) = mock_setup
    
    mock_near.call = AsyncMock(return_value=MagicMock(
        transaction=MagicMock(hash="tx789"),
        transaction_outcome=MagicMock(gas_burnt=100_000_000_000_000),
        logs=[],
        status={}
    ))
    
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless")
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    withdrawal.withdraw_balance("vault-1.testnet", "1", "")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Withdrawal Successful" in msg
    assert "vault-1.testnet" in msg
    assert "1 NEAR" in msg
    assert "tx789" in msg
    mock_near.call.assert_awaited_once()


def test_withdraw_balance_to_address(monkeypatch, mock_setup):
    """Withdraw NEAR to a specific recipient address."""
    
    (dummy_env, mock_near) = mock_setup
    
    mock_near.call = AsyncMock(return_value=MagicMock(
        transaction=MagicMock(hash="tx999"),
        transaction_outcome=MagicMock(gas_burnt=150_000_000_000_000),
        logs=[],
        status={}
    ))
   
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless")
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    withdrawal.withdraw_balance("vault-1.testnet", "2.5", to_address="palingram.testnet")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Withdrawal Successful" in msg
    assert "palingram.testnet" in msg
    assert "2.5 NEAR" in msg
    assert "tx999" in msg
    mock_near.call.assert_awaited_once()
    called_args = mock_near.call.call_args[1]["args"]
    assert called_args["to"] == "palingram.testnet"


def test_withdraw_balance_no_creds(monkeypatch, mock_setup):
    """Withdraw should fail when not in headless mode."""
    
    (dummy_env, mock_near) = mock_setup
    monkeypatch.setattr(helpers, "_SIGNING_MODE", None)  # simulate missing keys
    
    withdrawal.withdraw_balance("vault-1.testnet", "1", "")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "can't sign" in msg.lower()
    mock_near.call.assert_not_called()


def test_view_vault_status_with_validator_success(monkeypatch, mock_setup):
    """Should display staked/unstaked/can_withdraw for a given vault+validator pair."""
    
    (dummy_env, mock_near) = mock_setup
    
    # Simulated return from staking pool
    mock_near.view = AsyncMock(return_value=MagicMock(result={
        "account_id": "vault-1.vaultmint.testnet",
        "staked_balance": str(int(Decimal("3") * Decimal("1e24"))),
        "unstaked_balance": str(int(Decimal("1.5") * Decimal("1e24"))),
        "can_withdraw": True
    }))
    
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless")
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    summary.view_vault_status_with_validator("vault-1.vaultmint.testnet", "aurora.pool.f863973.m0")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Delegation Status" in msg
    assert "3.00000 NEAR" in msg
    assert "1.50000 NEAR" in msg
    assert "✅ Yes" in msg
    

def test_view_vault_status_with_validator_not_withdrawable(monkeypatch, mock_setup):
    """Should display ❌ No when can_withdraw is False."""
    
    (dummy_env, mock_near) = mock_setup
    
    mock_near.view = AsyncMock(return_value=MagicMock(result={
        "account_id": "vault-2.vaultmint.testnet",
        "staked_balance": str(int(Decimal("5") * Decimal("1e24"))),
        "unstaked_balance": str(int(Decimal("0.25") * Decimal("1e24"))),
        "can_withdraw": False
    }))
  
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless")
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    summary.view_vault_status_with_validator("vault-2.vaultmint.testnet", "meta.pool.testnet")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "5.00000 NEAR" in msg
    assert "0.25000 NEAR" in msg
    assert "❌ No" in msg
    

def test_view_vault_status_with_validator_no_data(monkeypatch, mock_setup):
    """Should show error when validator returns no data for vault."""
    
    (dummy_env, mock_near) = mock_setup
    
    mock_near.view = AsyncMock(return_value=MagicMock(result=None))
    
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    summary.view_vault_status_with_validator("vault-x.testnet", "unknown.pool.testnet")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "No data returned" in msg
    assert "vault-x.testnet" in msg
    assert "unknown.pool.testnet" in msg


def test_view_vault_status_with_validator_exception(monkeypatch, mock_setup):
    """Should catch and report unexpected RPC errors (e.g. network or contract panic)."""
    
    (dummy_env, mock_near) = mock_setup
    
    mock_near.view = AsyncMock(side_effect=RuntimeError("Contract not deployed"))

    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    summary.view_vault_status_with_validator("vault-y.testnet", "broken.pool.testnet")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Failed to get delegation status" in msg
    assert "vault-y.testnet" in msg
    assert "broken.pool.testnet" in msg
    assert "Contract not deployed" in msg


def test_claim_unstaked_balance_success(monkeypatch, mock_setup):
    """Should successfully call claim_unstaked and return a transaction link."""
    
    (dummy_env, mock_near) = mock_setup
    
    mock_near.call = AsyncMock(return_value=MagicMock(
        transaction=MagicMock(hash="claimtx123"),
        transaction_outcome=MagicMock(gas_burnt=120_000_000_000_000),
        logs=[],
        status={}
    ))

    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless")
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    withdrawal.claim_unstaked_balance("vault-1.vaultmint.testnet", "aurora.pool.f863973.m0")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Claim Initiated" in msg
    assert "vault-1.vaultmint.testnet" in msg
    assert "aurora.pool.f863973.m0" in msg
    assert "claimtx123" in msg
    

def test_claim_unstaked_balance_contract_panic(monkeypatch, mock_setup):
    """Should detect contract panic and emit the failure message."""
    
    (dummy_env, mock_near) = mock_setup
    
    mock_near.call = AsyncMock(return_value=MagicMock(
        transaction=MagicMock(hash="panic123"),
        transaction_outcome=MagicMock(gas_burnt=130_000_000_000_000),
        logs=[],
        status={
            "Failure": {
                "ActionError": {
                    "kind": {
                        "FunctionCallError": {
                            "ExecutionError": "Unstaked funds not yet claimable"
                        }
                    }
                }
            }
        }
    ))

    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless")
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    withdrawal.claim_unstaked_balance("vault-1.vaultmint.testnet", "aurora.pool.f863973.m0")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Claim failed with" in msg
    assert "Unstaked funds not yet claimable" in msg
    

def test_claim_unstaked_balance_no_credentials(monkeypatch, mock_setup):
    """Should refuse to sign when not in headless mode."""
   
    (dummy_env, mock_near) = mock_setup
    
    monkeypatch.setattr(helpers, "_SIGNING_MODE", None)  # Simulate missing creds
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    withdrawal.claim_unstaked_balance("vault-1.vaultmint.testnet", "aurora.pool.f863973.m0")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0].lower()
    assert "can't sign" in msg
    mock_near.call.assert_not_called()


def test_claim_unstaked_balance_runtime_exception(monkeypatch, mock_setup):
    """Should catch unexpected exceptions and display error message."""
    
    (dummy_env, mock_near) = mock_setup
    
    mock_near.call = AsyncMock(side_effect=RuntimeError("Network error: node unreachable"))
    
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless")
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    withdrawal.claim_unstaked_balance("vault-1.vaultmint.testnet", "aurora.pool.f863973.m0")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Failed to claim unstaked NEAR" in msg
    assert "Network error" in msg
    assert "vault-1.vaultmint.testnet" in msg
    assert "aurora.pool.f863973.m0" in msg


def test_vault_delegation_summary_success(mock_setup):
    """
    ✅ Basic Success Case:
    - Vault has one active validator: `validator1.near`
    - Validator reports 10 NEAR staked, 0 NEAR unstaked
    - `can_withdraw` is True (so no `unstaked_at` or `current_epoch` should show)
    - Should render a clean summary with balances and ✅ Yes
    """
    
    (dummy_env, mock_near) = mock_setup
    
    mock_near.view = AsyncMock(side_effect=[
        # get_vault_state call
        MagicMock(result={
            "current_epoch": 100,
            "unstake_entries": {},
            "active_validators": ["validator1.near"]
        }),
        # validator.get_account call
        MagicMock(result={
            "staked_balance": str(int(Decimal("10.0") * Decimal("1e24"))),
            "unstaked_balance": "0",
            "can_withdraw": True
        })
    ])
    
    summary.vault_delegation_summary("vault-0.testnet")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    
    assert "vault-0.testnet" in msg
    assert "validator1.near" in msg
    assert "10.00000 NEAR" in msg
    assert "0.00000 NEAR" in msg
    assert "✅ Yes" in msg


def test_vault_delegation_summary_with_locked_unstake(mock_setup):
    """
    🔒 Locked Unstake Case:
    - Vault has one validator (`validator2.near`) with 3.5 NEAR unstaked.
    - `can_withdraw` is False → should show locked status.
    - Should include: ❌ No, Unlocks at: `epoch_height`, Current Epoch.
    """
    
    (dummy_env, mock_near) = mock_setup
    
    mock_near.view = AsyncMock(side_effect=[
        # get_vault_state call
        MagicMock(result={
            "current_epoch": 108,
            "unstake_entries": [
                ["validator2.near", {"epoch_height": 105}]
            ],
            "active_validators": []
        }),
        # validator.get_account call
        MagicMock(result={
            "staked_balance": "0",
            "unstaked_balance": str(int(Decimal("3.5") * Decimal("1e24"))),
            "can_withdraw": False
        })
    ])
    
    summary.vault_delegation_summary("vault-locked.testnet")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    
    assert "vault-locked.testnet" in msg
    assert "validator2.near" in msg
    assert "3.50000 NEAR" in msg
    assert "❌ No" in msg
    assert "Unlocks at:     `105`" in msg
    assert "Current Epoch:  `108`" in msg


def test_vault_delegation_summary_active_and_unstaked(mock_setup):
    """
    🧪 Validator in both active_validators and unstake_entries.
    - Validator has 4 NEAR staked and 2 NEAR unstaked (not withdrawable).
    - Should show:
      • Both balances
      • ❌ No
      • Unlocks at + current_epoch
    """
    
    (dummy_env, mock_near) = mock_setup
    
    mock_near.view = AsyncMock(side_effect=[
        # get_vault_state call
        MagicMock(result={
            "current_epoch": 50,
            "active_validators": ["validator3.near"],
            "unstake_entries": [
                [
                    "validator3.near",
                    {
                        "amount": str(int(Decimal("2") * Decimal("1e24"))),
                        "epoch_height": 47
                    }
                ]
            ]
        }),
        # get_account call for validator
        MagicMock(result={
            "staked_balance": str(int(Decimal("4") * Decimal("1e24"))),
            "unstaked_balance": str(int(Decimal("2") * Decimal("1e24"))),
            "can_withdraw": False
        })
    ])
    
    summary.vault_delegation_summary("vault-mixed.testnet")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    
    assert "validator3.near" in msg
    assert "4.00000 NEAR" in msg
    assert "2.00000 NEAR" in msg
    assert "❌ No" in msg
    assert "Unlocks at:     `47`" in msg
    assert "Current Epoch:  `50`" in msg
    

def test_vault_delegation_summary_with_rpc_error(mock_setup):
    """
    ❌ Validator RPC error case.
    - get_vault_state succeeds.
    - get_account for validator4.near fails with an exception.
    - Should include:
      • Validator listed
      • ⛔ Error: <message>
    """
    
    (dummy_env, mock_near) = mock_setup
    
    mock_near.view = AsyncMock(side_effect=[
        # get_vault_state call
        MagicMock(result={
            "current_epoch": 200,
            "active_validators": ["validator4.near"],
            "unstake_entries": []
        }),
        # get_account fails
        RuntimeError("Contract not deployed")
    ])
    
    summary.vault_delegation_summary("vault-error.testnet")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    
    assert "validator4.near" in msg
    assert "⛔ Error" in msg
    assert "Contract not deployed" in msg
    

def test_vault_delegation_summary_empty(mock_setup):
    """
    ⚠️ No Validators Case:
    - active_validators and unstake_entries are both empty.
    - Should reply with a warning: "No delegation data found."
    """
    
    (dummy_env, mock_near) = mock_setup
    
    mock_near.view = AsyncMock(return_value=MagicMock(result={
        "current_epoch": 123,
        "active_validators": [],
        "unstake_entries": []
    }))
    
    summary.vault_delegation_summary("vault-empty.testnet")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]

    assert "No delegation data found" in msg
    assert "vault-empty.testnet" not in msg  # No need for vault header