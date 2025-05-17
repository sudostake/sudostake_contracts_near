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
    
    
# ─────────────────────────── init_near (headless) ─────────────────
def test_init_near_headless(monkeypatch):
    monkeypatch.setenv("NEAR_ACCOUNT_ID", "alice.testnet")
    monkeypatch.setenv("NEAR_PRIVATE_KEY", "ed25519:fake")
    monkeypatch.setenv("NEAR_RPC", "https://rpc.testnet.near.org")
    
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


# ─────────────────────────── init_near (no creds) ────────────────
def test_init_near_no_credentials(monkeypatch):
    """init_near should create a view-only account when no secrets or wallet."""
    # Clear any credential env-vars
    for var in ("NEAR_ACCOUNT_ID", "NEAR_PRIVATE_KEY", "NEAR_RPC"):
        monkeypatch.delenv(var, raising=False)
    
    # Give the helper a deterministic RPC endpoint
    monkeypatch.setenv("NEAR_NETWORK", "testnet")     
        
    # Fake Environment
    fake_account = MagicMock()
    fake_env = MagicMock()
    
    # Make sure the helper thinks no wallet signer is present
    if hasattr(fake_env, "signer_account_id"):
        delattr(fake_env, "signer_account_id")
    
    # set_near should be called with only rpc_addr & a dummy ID
    fake_env.set_near.return_value = fake_account
    
    account = helpers.init_near(fake_env)
    
    # Called exactly once with an anon account id
    fake_env.set_near.assert_called_once_with(
        account_id="anon",
        rpc_addr="https://rpc.testnet.near.org",
    )
    assert account is fake_account
    assert helpers.signing_mode() is None