import sys
import os
import pytest
import time
import asyncio
import requests

from unittest.mock import MagicMock
from unittest.mock import AsyncMock
from decimal import Decimal

# Make src/ importable
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '../src')))

import helpers  # type: ignore


# ─────────────────────────── async util ───────────────────────────
async def async_add(a, b):
    await asyncio.sleep(0.01)  # simulate awaitable work
    return a + b

# ─────────────────────────── fixtures ─────────────────────────────
@pytest.fixture(autouse=True)
def reset_globals(monkeypatch):
    """Ensure module-level globals are clean between tests."""
    
    helpers._VECTOR_STORE_ID = None
    helpers._SIGNING_MODE = None
    helpers._ACCOUNT_ID = None
    yield
    

@pytest.fixture(autouse=True)
def fast_clock(monkeypatch):
    """Skip real sleeping to keep tests quick."""
    monkeypatch.setattr(time, "sleep", lambda *_: None)


# ─────────────────────────── get_explorer_url ─────────────────────
def test_get_explorer_url_mainnet(monkeypatch):
    monkeypatch.setenv("NEAR_NETWORK", "mainnet")
    assert helpers.get_explorer_url() == "https://explorer.near.org"
    
def test_get_explorer_url_testnet(monkeypatch):
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    assert helpers.get_explorer_url() == "https://explorer.testnet.near.org"
    
def test_get_explorer_url_missing(monkeypatch):
    monkeypatch.delenv("NEAR_NETWORK", raising=False)
    with pytest.raises(RuntimeError, match="Missing required environment variable: NEAR_NETWORK"):
        helpers.get_explorer_url()

def test_get_explorer_url_invalid(monkeypatch):
    monkeypatch.setenv("NEAR_NETWORK", "invalidnet")
    with pytest.raises(RuntimeError, match="Unsupported NEAR_NETWORK"):
        helpers.get_explorer_url()


# ─────────────────────────── event-loop helpers ───────────────────
def test_ensure_loop_returns_event_loop():
    loop1 = helpers.ensure_loop()
    loop2 = helpers.ensure_loop()
    
    assert loop1 is loop2  # ✅ Should return same instance
    assert loop1.is_running() is False  # ✅ It's not running yet
    assert hasattr(loop1, "run_until_complete")  # ✅ Sanity check
    
def test_run_coroutine_executes_async_function():
    result = helpers.run_coroutine(async_add(2, 3))
    assert result == 5
    
    
# ----------------------------------------------------------------------
# init_near happy-path: headless (creds + network)
# ----------------------------------------------------------------------
def test_init_near_headless(monkeypatch):
    monkeypatch.setenv("NEAR_ACCOUNT_ID", "alice.testnet")
    monkeypatch.setenv("NEAR_PRIVATE_KEY", "ed25519:fake")
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    fake_account = MagicMock()
    fake_env = MagicMock()
    fake_env.set_near.return_value = fake_account
    
    account = helpers.init_near(fake_env)
    
    fake_env.set_near.assert_called_once_with(
        account_id="alice.testnet",
        private_key="ed25519:fake",
        rpc_addr="https://rpc.testnet.fastnear.com",
    )
    assert account is fake_account
    assert helpers.signing_mode() == "headless"


# ----------------------------------------------------------------------
# init_near happy-path: view-only (no creds, but network set)
# ----------------------------------------------------------------------
def test_init_near_view_only(monkeypatch):
    # ensure creds absent
    for var in ("NEAR_ACCOUNT_ID", "NEAR_PRIVATE_KEY"):
        monkeypatch.delenv(var, raising=False)
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    fake_env = MagicMock()
    fake_account = MagicMock()
    fake_env.set_near.return_value = fake_account
    
    account = helpers.init_near(fake_env)
    
    fake_env.set_near.assert_called_once_with(
        rpc_addr="https://rpc.testnet.fastnear.com",
    )
    assert account is fake_account
    assert helpers.signing_mode() is None
    

# ----------------------------------------------------------------------
# init_near error: NEAR_NETWORK missing
# ----------------------------------------------------------------------
def test_init_near_missing_network(monkeypatch):
    for var in ("NEAR_NETWORK", "NEAR_ACCOUNT_ID", "NEAR_PRIVATE_KEY"):
        monkeypatch.delenv(var, raising=False)
        
    with pytest.raises(RuntimeError, match="NEAR_NETWORK must be set"):
        helpers.init_near(MagicMock())


# ----------------------------------------------------------------------
# init_near error: NEAR_NETWORK invalid
# ----------------------------------------------------------------------
def test_init_near_invalid_network(monkeypatch):
    monkeypatch.setenv("NEAR_NETWORK", "betanet")  # unsupported
    
    with pytest.raises(RuntimeError, match="NEAR_NETWORK must be set"):
        helpers.init_near(MagicMock())


# ─────────────────────────── fetch_usdc_balance tests ─────────────────────
def test_fetch_usdc_balance_success(monkeypatch):
    """Should correctly return a Decimal USDC balance when view result is valid."""
    
    mock_near = MagicMock()
    raw_usdc = int(Decimal("123.45") * Decimal("1e6"))
    
    # Mock the view response
    mock_near.view = AsyncMock(return_value=MagicMock(result=str(raw_usdc)))
    
    # Patch usdc_contract() to return a fake contract address
    monkeypatch.setattr(helpers, "usdc_contract", lambda: "usdc.token.testnet")
    
    result = helpers.fetch_usdc_balance(mock_near, "vault-1.factory.testnet")
    
    assert isinstance(result, Decimal)
    assert result == Decimal("123.45")
    mock_near.view.assert_called_once_with(
        "usdc.token.testnet", "ft_balance_of", {"account_id": "vault-1.factory.testnet"}
    )
    

def test_fetch_usdc_balance_no_result(monkeypatch):
    """Should raise ValueError when the USDC view call returns no result."""
    
    mock_near = MagicMock()
    mock_near.view = AsyncMock(return_value=MagicMock(result=None))
    
    monkeypatch.setattr(helpers, "usdc_contract", lambda: "usdc.token.testnet")
    
    with pytest.raises(ValueError, match="No USDC balance returned for `vault-1.factory.testnet`"):
        helpers.fetch_usdc_balance(mock_near, "vault-1.factory.testnet")



# ─────────────────────────── index_vault_to_firebase tests ─────────────────────

def test_index_vault_to_firebase_success(monkeypatch):
    """Should call requests.post and succeed without raising."""
    
    captured = {}
    
    def fake_post(url, json, timeout, headers):
        captured["url"] = url
        captured["json"] = json
        captured["headers"] = headers
        class FakeResp:
            def raise_for_status(self): pass
        return FakeResp()
    
    monkeypatch.setattr(helpers.requests, "post", fake_post)
    
    helpers.index_vault_to_firebase("vault-123.factory.testnet")
    
    assert captured["url"].endswith("/index_vault")
    assert captured["json"] == {"vault": "vault-123.factory.testnet"}
    assert captured["headers"]["Content-Type"] == "application/json"


def test_index_vault_to_firebase_failure(monkeypatch):
    """Should raise HTTPError if indexing fails."""
    
    class FakeResp:
        def raise_for_status(self):
            raise requests.HTTPError("Mocked 500 error")
        
    def fake_post(*args, **kwargs):
        return FakeResp()
    
    monkeypatch.setattr(helpers.requests, "post", fake_post)
    
    with pytest.raises(requests.HTTPError, match="Mocked 500 error"):
        helpers.index_vault_to_firebase("vault-999.factory.testnet")


