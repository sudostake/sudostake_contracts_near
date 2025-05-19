# tests/test_tools.py
import sys
import os
import pytest
from decimal import Decimal
from unittest.mock import AsyncMock, MagicMock

# Make src/ importable
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "../src")))

import tools            # type: ignore
import helpers          # type: ignore


# ─────────────────────────── fixtures ───────────────────────────
@pytest.fixture
def mock_near():
    mock = MagicMock()
    return mock

@pytest.fixture
def headless_mode(monkeypatch):
    """Force helpers.signing_mode() → 'headless' for mutating tool tests."""
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    
    yield
    # cleanup (pytest will restore monkeypatch state automatically)
    
@pytest.fixture
def mock_env(monkeypatch):
    """Dummy Environment instance captured by tools._env."""
    env = MagicMock()
    monkeypatch.setattr(tools, "_env", env, raising=False)
    return env


# ─────────────────────────── tests ──────────────────────────────
def test_vault_state(monkeypatch, mock_near):
    mock_near.view = AsyncMock(return_value=MagicMock(result={
        "owner": "alice.near",
        "index": 0,
        "version": 1,
        "is_listed_for_takeover": False,
        "pending_liquidity_request": None,
        "liquidity_request": None,
        "accepted_offer": None
    }))
    monkeypatch.setattr(tools, "_near", mock_near)
    
    dummy_env = MagicMock()
    monkeypatch.setattr(tools, "_env", dummy_env, raising=False)
    
    tools.vault_state("vault-0.testnet")
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "✅ **Vault State" in msg
    assert "`alice.near`" in msg
    

def test_view_available_balance(monkeypatch, mock_near):
    yocto_balance = int(Decimal("1.25") * Decimal("1e24"))
    mock_near.view = AsyncMock(return_value=MagicMock(result=str(yocto_balance)))
    monkeypatch.setattr(tools, "_near", mock_near)
    
    dummy_env = MagicMock()
    monkeypatch.setattr(tools, "_env", dummy_env, raising=False)
    
    tools.view_available_balance("vault-0.testnet")

    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "**1.25000 NEAR**" in msg
    

def test_delegate_headless(monkeypatch, mock_near):
    """
    delegate() should succeed in head-less mode (secrets present) and
    embed the tx-hash plus success banner in the returned markdown.
    """
    
    mock_near.call = AsyncMock(return_value=MagicMock(
        transaction=MagicMock(hash="abc123"),
        transaction_outcome=MagicMock(gas_burnt=310_000_000_000_000),
        logs=[]
    ))
    monkeypatch.setattr(tools, "_near", mock_near)
    
    # Inject a dummy Environment so _env guard passes
    dummy_env = MagicMock()
    monkeypatch.setattr(tools, "_env", dummy_env, raising=False)
    
    # Force helpers.signing_mode() → "headless"
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    
    # Needed for explorer link formatting
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Execute the delegate command
    result = tools.delegate("vault-0.testnet", "validator.near", "1")
    
    # Assertions
    assert result is None
    dummy_env.add_reply.assert_called_once()
    
    # grab the message text and check key fragments
    msg = dummy_env.add_reply.call_args[0][0]
    assert "✅ **Delegation Successful**" in msg
    assert "abc123" in msg


def test_delegate_no_credentials(monkeypatch, mock_near):
    """
    delegate() should refuse to sign when signing_mode != 'headless'
    and emit a single warning via _env.add_reply().
    """
    
    # Provide mocked _near so the init guard passes
    monkeypatch.setattr(tools, "_near", mock_near)
    
    # Dummy Environment with add_reply
    dummy_env = MagicMock()
    monkeypatch.setattr(tools, "_env", dummy_env, raising=False)
    
    # Force helpers.signing_mode() → None  (no creds, no wallet)
    monkeypatch.setattr(helpers, "_SIGNING_MODE", None, raising=False)
    
    # Ensure explorer URL can format
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Call the tool
    result = tools.delegate("vault-0.testnet", "validator.near", "1")
    
    # Assertions
    assert result is None                        # function returns nothing
    dummy_env.add_reply.assert_called_once()     # one warning sent

    warning = dummy_env.add_reply.call_args[0][0]
    assert "can't sign" in warning or "can't sign" in warning.lower()
    
    
def test_mint_vault_headless(monkeypatch, mock_near):
    """
    mint_vault() should succeed when head-less credentials exist.
    It must push a single success message that contains:
      • the 'Vault Minted' banner
      • the new vault account id parsed from EVENT_JSON
      • the tx-hash link
    """
    
    # Pretend the chain call succeeded and emitted the standard macro log
    mock_near.call = AsyncMock(
        return_value=MagicMock(
            transaction=MagicMock(hash="tx123"),
            transaction_outcome=MagicMock(gas_burnt=1),
            logs=[
                'EVENT_JSON:{"event":"vault_minted",'
                '"data":{"vault":"vault-0.vaultmint.testnet"}}'
            ],
        )
    )
    monkeypatch.setattr(tools, "_near", mock_near)
    
    # Provide a dummy Environment so tools._env.add_reply works
    dummy_env = MagicMock()
    monkeypatch.setattr(tools, "_env", dummy_env, raising=False)
    
    # Force head-less signing mode
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    
    # Select network → resolves factory_id internally
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
     # Run the tool
    tools.mint_vault()
    
    # Verify exactly one user reply with expected fragments
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Vault Minted" in msg
    assert "vault-0.vaultmint.testnet" in msg
    
    # Verify the expected tx_hash link
    expected_url = f"{helpers.get_explorer_url()}/transactions/tx123"
    assert expected_url in msg


def test_mint_vault_no_credentials(monkeypatch, mock_near):
    """
    When signing is disabled, mint_vault() must:
      • NOT call _near.call()
      • emit exactly one warning via _env.add_reply()
      • return None
    """
    
    # Stub _near so agent init guard passes, but ensure .call is never used
    monkeypatch.setattr(tools, "_near", mock_near)
    
    # Dummy env to capture reply
    dummy_env = MagicMock()
    monkeypatch.setattr(tools, "_env", dummy_env, raising=False)
    
    # No credentials / wallet → signing_mode == None
    monkeypatch.setattr(helpers, "_SIGNING_MODE", None, raising=False)
    
    # Network still needed for factory lookup
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Invoke tool
    result = tools.mint_vault()
    
    # Assertions
    assert result is None
    mock_near.call.assert_not_called()            # nothing signed
    dummy_env.add_reply.assert_called_once()
    assert "can't sign" in dummy_env.add_reply.call_args[0][0].lower()
    

def test_mint_vault_missing_event(monkeypatch, mock_near):
    """
    If the tx logs do not include a vault_minted EVENT_JSON entry,
    mint_vault() must:
      • send a single error reply via _env.add_reply()
      • still return None
    """
    
    # Mock py_near.call() with NO event log
    mock_near.call = AsyncMock(
        return_value=MagicMock(
            transaction=MagicMock(hash="tx999"),
            transaction_outcome=MagicMock(gas_burnt=1),
            logs=[],          # ← no EVENT_JSON at all
        )
    )
    monkeypatch.setattr(tools, "_near", mock_near)
    
    # Dummy env
    dummy_env = MagicMock()
    monkeypatch.setattr(tools, "_env", dummy_env, raising=False)
    
    # Head-less signing enabled
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Run the tool
    result = tools.mint_vault()
    
    # Assertions
    assert result is None
    mock_near.call.assert_called_once()           # call attempted
    dummy_env.add_reply.assert_called_once()      # one error message
    
    err_msg = dummy_env.add_reply.call_args[0][0].lower()
    assert "vault minting failed" in err_msg


def test_view_main_balance_headless(monkeypatch, mock_near):
    """Head-less mode: tool should return the correct NEAR balance."""
    yocto_five = int(5 * 1e24)
    mock_near.get_balance = AsyncMock(return_value=yocto_five)
    monkeypatch.setattr(tools, "_near", mock_near)
    
    dummy_env = MagicMock()
    monkeypatch.setattr(tools, "_env", dummy_env, raising=False)
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    monkeypatch.setattr(helpers, "_ACCOUNT_ID", "alice.testnet", raising=False)
    
    tools.view_main_balance()
    
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "**5.00000 NEAR**" in msg
    mock_near.get_balance.assert_awaited_once()
    

def test_view_main_balance_no_credentials(monkeypatch, mock_near):
    """No signing keys: tool should warn and never call get_balance()."""
    monkeypatch.setattr(tools, "_near", mock_near)
    
    dummy_env = MagicMock()
    monkeypatch.setattr(tools, "_env", dummy_env, raising=False)
    monkeypatch.setattr(helpers, "_SIGNING_MODE", None, raising=False)
    
    tools.view_main_balance()
    
    dummy_env.add_reply.assert_called_once()
    warn = dummy_env.add_reply.call_args[0][0].lower()
    assert "no signing keys" in warn
    mock_near.get_balance.assert_not_called()


def test_transfer_near_headless(monkeypatch, mock_near):
    """Head-less: transfer should call send_money and emit success reply."""
    
    # Mock the send_money call to simulate a successful transaction
    mock_near.send_money = AsyncMock(
        return_value=MagicMock(
            transaction=MagicMock(hash="tx456"),
            transaction_outcome=MagicMock(gas_burnt=1),
            logs=[],
        )
    )
    monkeypatch.setattr(tools, "_near", mock_near)
    
    # Set up a dummy environment to capture the reply
    dummy_env = MagicMock()
    monkeypatch.setattr(tools, "_env", dummy_env, raising=False)
    
    # Force helpers.signing_mode() → "headless"
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless", raising=False)
    
    # Set the network to testnet for explorer URL formatting
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Call the transfer_near function
    tools.transfer_near_to_vault("vault-0.testnet", "3")
    
    # Assertions
    dummy_env.add_reply.assert_called_once()
    
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Transfer Submitted" in msg and "3.00000 NEAR" in msg
    assert "tx456" in msg
    
    mock_near.send_money.assert_awaited_once()
    

def test_transfer_near_no_creds(monkeypatch, mock_near):
    """No signing keys: tool should warn and skip RPC call."""
    
    monkeypatch.setattr(tools, "_near", mock_near)
    
    dummy_env = MagicMock()
    monkeypatch.setattr(tools, "_env", dummy_env, raising=False)
    monkeypatch.setattr(helpers, "_SIGNING_MODE", None, raising=False)
    
    tools.transfer_near_to_vault("vault-0.testnet", "2")
    
    # Assertions
    dummy_env.add_reply.assert_called_once()
    assert "no signing keys" in dummy_env.add_reply.call_args[0][0].lower()
    mock_near.send_money.assert_not_called()
