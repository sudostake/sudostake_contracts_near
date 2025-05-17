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