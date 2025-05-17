import sys
import os
import pytest
import asyncio

from unittest.mock import MagicMock

# Add src/ to import path
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '../src')))

import helpers  # type: ignore[import-unresolved]

async def async_add(a, b):
    await asyncio.sleep(0.01)  # simulate awaitable work
    return a + b

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

def test_ensure_loop_returns_event_loop():
    loop1 = helpers.ensure_loop()
    loop2 = helpers.ensure_loop()
    
    assert loop1 is loop2  # ✅ Should return same instance
    assert loop1.is_running() is False  # ✅ It's not running yet
    assert hasattr(loop1, "run_until_complete")  # ✅ Sanity check
    
def test_run_coroutine_executes_async_function():
    result = helpers.run_coroutine(async_add(2, 3))
    assert result == 5
    
def test_set_credentials_calls_env(monkeypatch):
    monkeypatch.setenv("NEAR_ACCOUNT_ID", "alice.testnet")
    monkeypatch.setenv("NEAR_PRIVATE_KEY", "ed25519:fakekey")
    monkeypatch.setenv("NEAR_RPC", "https://rpc.testnet.near.org")
    
    fake_account = MagicMock()
    fake_env = MagicMock()
    fake_env.set_near.return_value = fake_account
    
    result = helpers.set_credentials(fake_env)
    
    fake_env.set_near.assert_called_once_with(
        account_id="alice.testnet",
        private_key="ed25519:fakekey",
        rpc_addr="https://rpc.testnet.near.org",
    )
    assert result is fake_account
    
def test_set_credentials_raises_if_missing(monkeypatch):
    monkeypatch.delenv("NEAR_ACCOUNT_ID", raising=False)
    monkeypatch.delenv("NEAR_PRIVATE_KEY", raising=False)
    monkeypatch.delenv("NEAR_RPC", raising=False)
    
    fake_env = MagicMock()
    
    with pytest.raises(RuntimeError) as e:
        helpers.set_credentials(fake_env)
    
    assert "Missing required environment variable" in str(e.value)