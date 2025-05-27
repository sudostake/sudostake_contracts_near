import sys
import os
import pytest
import time
import asyncio
from pathlib import Path
from typing import List
from unittest.mock import MagicMock

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
    
    
@pytest.fixture
def openai_mock(monkeypatch):
    """
    Patch helpers.openai.OpenAI with a MagicMock exposing just the bits
    init_vector_store() touches.  Tests can tweak attributes as needed.
    """
    
    client = MagicMock(name="OpenAIClient")
    
    # • files.create → returns objs that each carry a unique .id
    client.files.create.side_effect = [
        MagicMock(id=f"file_{i}") for i in range(1, 10)
    ]
    
    # • vector_stores.create → returns a VS obj with .id
    vs_obj = MagicMock(id="vs_1")
    client.vector_stores.create.return_value = vs_obj
    
    # • vector_stores.retrieve → default happy-path: first in_progress, then completed
    in_progress = MagicMock(
        file_counts=MagicMock(completed=0),
        status="in_progress",
        last_error=None,
    )
    completed = MagicMock(
        file_counts=MagicMock(completed=1),
        status="completed",
        last_error=None,
    )
    client.vector_stores.retrieve.side_effect = [in_progress, completed]
    
    monkeypatch.setattr(helpers.openai, "OpenAI", MagicMock(return_value=client))
    return client

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


# ───────────────────────── init_vector_store ────────────────
def test_init_vector_store_success(tmp_path, monkeypatch, openai_mock):
    # create a markdown file so the helper finds something
    (tmp_path / "README.md").write_text("# Demo docs")
    monkeypatch.chdir(tmp_path)
    
    helpers.init_vector_store()
    
    # upload & create calls happen
    openai_mock.files.create.assert_called()
    openai_mock.vector_stores.create.assert_called_once()
    assert helpers.vector_store_id() == "vs_1"


def test_init_vector_store_no_md_files(tmp_path, monkeypatch, openai_mock):
    monkeypatch.chdir(tmp_path)
    with pytest.raises(FileNotFoundError):
        helpers.init_vector_store()


def test_init_vector_store_timeout(tmp_path, monkeypatch, openai_mock):
    (tmp_path / "doc.md").write_text("stub")
    monkeypatch.chdir(tmp_path)
    
    # force retrieve to always return in_progress
    in_progress = MagicMock(
        file_counts=MagicMock(completed=0),
        status="in_progress",
        last_error=None,
    )
    openai_mock.vector_stores.retrieve.side_effect = [in_progress] * 5
    
    # make timeout immediate
    monkeypatch.setattr(helpers, "MAX_BUILD_MINUTES", 0)
    
    with pytest.raises(TimeoutError):
        helpers.init_vector_store()


def test_init_vector_store_expired(tmp_path, monkeypatch, openai_mock):
    (tmp_path / "doc.md").write_text("stub")
    monkeypatch.chdir(tmp_path)
    
    expired = MagicMock(
        file_counts=MagicMock(completed=0),
        status="expired",
        last_error="boom!",
    )
    
    openai_mock.vector_stores.retrieve.side_effect = [expired]
    
    with pytest.raises(RuntimeError, match="failed to build"):
        helpers.init_vector_store()
