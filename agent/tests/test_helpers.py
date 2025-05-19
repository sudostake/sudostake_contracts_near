import sys
import os
import pytest
import asyncio
from unittest.mock import MagicMock

# Make src/ importable
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '../src')))

import helpers  # type: ignore


# ─────────────────────────── async util ───────────────────────────
async def async_add(a, b):
    await asyncio.sleep(0.01)  # simulate awaitable work
    return a + b


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
        rpc_addr="https://rpc.testnet.near.org",
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
        rpc_addr="https://rpc.testnet.near.org",
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
