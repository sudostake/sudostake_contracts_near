# tests/test_tools.py
import sys
import os
import pytest
import requests
import json

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
    docs
)

# ───────────────────────────  helpers  ───────────────────────────
def _make_dummy_resp(json_body):
    """Minimal stub mimicking requests.Response for our needs."""
    class DummyResp:
        def raise_for_status(self):          # no-op ⇢ 200 OK
            pass
        def json(self):
            return json_body
    return DummyResp()


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
def test_view_available_balance(monkeypatch, mock_setup):
    """
    Should display both NEAR and USDC balances when available:
      • Vault NEAR balance is returned via `view_available_balance`
      • Vault USDC balance is returned via `ft_balance_of`
      • Tool formats both in markdown and replies once
    """
    
    (dummy_env, mock_near) = mock_setup
    
    # Set the default network
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Simulated vault NEAR balance: 1.25 NEAR
    near_yocto = int(Decimal("1.25") * Decimal("1e24"))
    
    # Simulated vault USDC balance: 45.67 USDC
    usdc_raw = int(Decimal("45.67") * Decimal("1e6"))
    
    # First near.view call → NEAR balance
    # Second near.view call → USDC balance
    mock_near.view = AsyncMock(side_effect=[
        MagicMock(result=str(near_yocto)),  # response from view_available_balance
        MagicMock(result=str(usdc_raw))     # response from ft_balance_of
    ])
    
    # Execute the tool
    balance.view_available_balance("vault-0.factory.testnet")
    
    # Should emit one markdown reply
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    
    # Validate content of response
    assert "vault-0.factory.testnet" in msg
    assert "**NEAR:** `1.25000`" in msg
    assert "**USDC:** `45.67`" in msg
    

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
    

# ───────────────── mint_vault tests ─────────────────
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
                '"data":{"vault":"vault-0.factory.testnet"}}'
            ],
            status={}
        )
    )
    
    # Patch requests.post inside minting to capture indexing call
    called = {}
    def fake_post(url, json, timeout, headers):
        called["url"]  = url
        called["json"] = json
        class Resp:
            def raise_for_status(self): pass  # simulate 200 OK
        return Resp()

    monkeypatch.setattr(minting, "requests", MagicMock(post=fake_post))
    
    
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
    assert "vault-0.factory.testnet" in msg
    
    # Verify the expected tx_hash link
    expected_url = f"{helpers.get_explorer_url()}/transactions/tx123"
    assert expected_url in msg
    
    # Verify Firebase indexing call
    assert called["url"].endswith("/index_vault")
    assert called["json"] == {"vault": "vault-0.factory.testnet"}


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
    """Head-less mode: tool should return the correct NEAR and USDC balances."""
    (dummy_env, mock_near) = mock_setup
    
    # Mock NEAR balance (5 NEAR)
    yocto_five = int(5 * 1e24)
    mock_near.get_balance = AsyncMock(return_value=yocto_five)
    
    # Mock USDC balance (123.45 USDC)
    usdc_amount = int(Decimal("123.45") * Decimal("1e6"))
    mock_near.view = AsyncMock(return_value=MagicMock(result=str(usdc_amount)))

    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    monkeypatch.setattr(helpers, "_ACCOUNT_ID", "alice.testnet", raising=False)
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    balance.view_main_balance()
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    
    assert "**Account:** `alice.testnet`" in msg
    assert "**NEAR:** `5.00000`" in msg
    assert "**USDC:** `123.45`" in msg
    
    mock_near.get_balance.assert_awaited_once()
    mock_near.view.assert_awaited_once_with(
        helpers.usdc_contract(), "ft_balance_of", {"account_id": "alice.testnet"}
    )
    

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
        "account_id": "vault-1.factory.testnet",
        "staked_balance": str(int(Decimal("3") * Decimal("1e24"))),
        "unstaked_balance": str(int(Decimal("1.5") * Decimal("1e24"))),
        "can_withdraw": True
    }))
    
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless")
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    summary.view_vault_status_with_validator("vault-1.factory.testnet", "aurora.pool.f863973.m0")
    
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
        "account_id": "vault-2.factory.testnet",
        "staked_balance": str(int(Decimal("5") * Decimal("1e24"))),
        "unstaked_balance": str(int(Decimal("0.25") * Decimal("1e24"))),
        "can_withdraw": False
    }))
  
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless")
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    summary.view_vault_status_with_validator("vault-2.factory.testnet", "meta.pool.testnet")
    
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
    
    withdrawal.claim_unstaked_balance("vault-1.factory.testnet", "aurora.pool.f863973.m0")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Claim Initiated" in msg
    assert "vault-1.factory.testnet" in msg
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
    
    withdrawal.claim_unstaked_balance("vault-1.factory.testnet", "aurora.pool.f863973.m0")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Claim failed with" in msg
    assert "Unstaked funds not yet claimable" in msg
    

def test_claim_unstaked_balance_no_credentials(monkeypatch, mock_setup):
    """Should refuse to sign when not in headless mode."""
   
    (dummy_env, mock_near) = mock_setup
    
    monkeypatch.setattr(helpers, "_SIGNING_MODE", None)  # Simulate missing creds
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    withdrawal.claim_unstaked_balance("vault-1.factory.testnet", "aurora.pool.f863973.m0")
    
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
    
    withdrawal.claim_unstaked_balance("vault-1.factory.testnet", "aurora.pool.f863973.m0")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Failed to claim unstaked NEAR" in msg
    assert "Network error" in msg
    assert "vault-1.factory.testnet" in msg
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
    

# ───────────────── view_user_vaults tests ─────────────────
def test_view_user_vaults_success(monkeypatch, mock_setup):
    """
    Success:
      • head-less signer 'alice.testnet'
      • Cloud Function returns 2 vaults
      • Tool should emit a list with both IDs
    """
    
    (dummy_env, _) = mock_setup
    
    # Head-less mode + signer + network
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    monkeypatch.setattr(helpers, "_ACCOUNT_ID",  "alice.testnet", raising=False)
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Patch requests.get inside vault module to return fake JSON list
    monkeypatch.setattr(
        vault.requests, "get",
        lambda url, timeout: _make_dummy_resp(
            ["vault-0.factory.testnet", "vault-1.factory.testnet"]
        ),
    )
    
    # Run the tool
    vault.view_user_vaults()
    
    # Assertions
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "You have 2 vaults"  in msg
    assert "- vault-0.factory.testnet" in msg
    assert "- vault-1.factory.testnet" in msg
    

def test_view_user_vaults_empty(monkeypatch, mock_setup):
    """
    Cloud Function returns an empty list → tool should reply
    “No vaults found …” (and *not* list anything).
    """
    
    (dummy_env, _) = mock_setup
    
    # Head-less signer + network
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    monkeypatch.setattr(helpers, "_ACCOUNT_ID",  "bob.testnet", raising=False)
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Patch requests.get to return an empty JSON array
    monkeypatch.setattr(
        vault.requests, "get",
        lambda url, timeout: _make_dummy_resp([]),
    )
    
    # Invoke the tool
    vault.view_user_vaults()
    
    # Verify exactly one “no vaults” reply
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "No vaults found" in msg
    assert "bob.testnet"      in msg
    

def test_view_user_vaults_no_creds(monkeypatch, mock_setup):
    """
    When signing_mode ≠ 'headless' the tool should:
       • emit the 'no signing keys' warning
       • never call requests.get()
    """
    
    (dummy_env, _) = mock_setup
    
    # Simulate missing keys
    monkeypatch.setattr(helpers, "_SIGNING_MODE", None, raising=False)
    
    # Stub requests.get so we can assert it was not used
    dummy_get = MagicMock()
    monkeypatch.setattr(vault.requests, "get", dummy_get)
    
    # Run the tool
    vault.view_user_vaults()
    
    # Assertions
    dummy_env.add_reply.assert_called_once()
    warn = dummy_env.add_reply.call_args[0][0].lower()
    assert "no signing keys" in warn
    dummy_get.assert_not_called()
    

def test_view_user_vaults_http_error(monkeypatch, mock_setup):
    """
    Network failure:
       • requests.get raises ConnectionError
       • Tool should catch it and emit the generic failure reply
    """
    
    (dummy_env, _) = mock_setup
    
    # Head-less signer & network
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    monkeypatch.setattr(helpers, "_ACCOUNT_ID",  "carol.testnet", raising=False)
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Patch requests.get inside vault to raise an error
    def boom(url, timeout):
        raise requests.exceptions.ConnectionError("node unreachable")
    
    monkeypatch.setattr(vault.requests, "get", boom)
    
    # Run the tool
    vault.view_user_vaults()
    
    # Assertions
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Failed to fetch vault list" in msg
    assert "node unreachable"           in msg


# ───────────────── query_sudostake_docs tests ─────────────────

@pytest.fixture(autouse=True)
def _reset_vs(monkeypatch):
    """Ensure each test starts with a clean vector-store id."""
    monkeypatch.setattr(helpers, "_VECTOR_STORE_ID", None, raising=False)


def test_query_docs_success(mock_setup):
    """Vector-store exists and a user prompt is present."""
    
    env, _ = mock_setup
    helpers._VECTOR_STORE_ID = "vs_abc"
    
    env.list_messages.return_value = [{"content": "What is SudoStake?"}]
    env.query_vector_store.return_value = [{"chunk_text": "SudoStake is …"}]
    
    docs.query_sudostake_docs()
    
    env.query_vector_store.assert_called_once_with("vs_abc", "What is SudoStake?")
    expected = json.dumps([{"chunk_text": "SudoStake is …"}], indent=2)
    env.add_reply.assert_called_once_with(expected)


def test_query_docs_no_vs_id(mock_setup):
    """No vector-store built ⇒ guard fires, no query happens."""
    
    env, _ = mock_setup
    env.list_messages.return_value = [{"content": "hello"}]
    
    docs.query_sudostake_docs()
    
    env.query_vector_store.assert_not_called()
    env.add_reply.assert_called_once()
    assert "not initialised" in env.add_reply.call_args[0][0].lower()
    

def test_query_docs_no_user_message(mock_setup):
    """Thread has no user messages ⇒ guard fires, no query."""
    
    env, _ = mock_setup
    helpers._VECTOR_STORE_ID = "vs_any"
    env.list_messages.return_value = []          # empty
    
    docs.query_sudostake_docs()
    
    env.query_vector_store.assert_not_called()
    env.add_reply.assert_called_once()
    assert "no query provided" in env.add_reply.call_args[0][0].lower()
