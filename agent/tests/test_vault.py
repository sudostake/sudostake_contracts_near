import sys
import os
import pytest
import requests

from decimal import Decimal
from unittest.mock import AsyncMock, MagicMock
from datetime import datetime

# Make src/ importable
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "../src")))

import helpers # type: ignore
from tools import ( # type: ignore[import]
    context,
    vault,
)
from helpers import ( # type: ignore[import]
    USDC_FACTOR,
    YOCTO_FACTOR
)


@pytest.fixture
def mock_setup():
    """Initialize mock environment, logger, and near — then set context."""
    env = MagicMock()
    near = MagicMock()

    # Set the context globally for tools
    context.set_context(env=env, near=near)

    return (env, near)


@pytest.fixture
def minimal_vault_state():
    return {
        "owner": "alice.testnet",
        "index": 1,
        "version": 3,
        "is_listed_for_takeover": False,
        "liquidity_request": None,
        "accepted_offer": None,
        "liquidation": None,
        "active_validators": [],
        "unstake_entries": [],
        "current_epoch": 1000,
    }
    

def _make_dummy_resp(json_body):
    """Minimal stub mimicking requests.Response for our needs."""
    class DummyResp:
        def raise_for_status(self):          # no-op ⇢ 200 OK
            pass
        def json(self):
            return json_body
    return DummyResp()

    
def with_liquidity(state):
    """Inject a liquidity request into vault state."""
    
    state["liquidity_request"] = {
        "token": "usdc.testnet",
        "amount": str(int(Decimal("120.00") * USDC_FACTOR)),
        "interest": str(int(Decimal("2.50") * USDC_FACTOR)),
        "collateral": str(int(Decimal("5.0") * YOCTO_FACTOR)),
        "duration": 86400,
        "created_at": str(int(datetime(2024, 1, 1, 12, 0).timestamp() * 1_000_000_000)),
    }
    return state


def with_offer(state):
    """Inject an accepted offer into vault state."""
    
    state["accepted_offer"] = {
        "lender": "bob.testnet",
        "accepted_at": int(datetime(2024, 1, 2, 14, 0).timestamp() * 1_000_000_000),
    }
    return state


def with_liquidation(state):
    """Inject a liquidation record into vault state."""
    
    state["liquidation"] = {
        "liquidated": str(int(Decimal("3.5") * YOCTO_FACTOR))
    }
    return state


def test_vault_state_minimal(mock_setup, minimal_vault_state):
    """
    Should display basic vault state summary correctly when no liquidity request,
    accepted offer, or liquidation is present.

    Verifies:
    - Vault metadata (owner, index, version) is displayed
    - Flags for liquidity request and accepted offer are both False
    - No exception is raised during the call
    """
    
    (dummy_env, mock_near) = mock_setup
    mock_near.view = AsyncMock(return_value=MagicMock(result=minimal_vault_state))
    
    vault.vault_state("vault-0.factory.testnet")
    
    reply = dummy_env.add_reply.call_args_list[-1][0][0]
    assert "✅ **Vault State: `vault-0.factory.testnet`**" in reply
    assert "| Owner" in reply
    assert "| Index" in reply
    assert "| Version" in reply
    assert "`False`" in reply  # For both Active Request and Accepted Offer


def test_vault_state_with_liquidity_request(mock_setup, minimal_vault_state):
    """
    Should correctly display liquidity request summary when present in vault state.

    Verifies:
    - Vault summary appears as usual
    - Liquidity request fields (token, amount, interest, collateral, duration, created_at) are rendered
    - Human-readable formatting is applied for USDC and NEAR
    """
    
    (dummy_env, mock_near) = mock_setup
    mock_near.view = AsyncMock(return_value=MagicMock(result=with_liquidity(minimal_vault_state)))
    
    vault.vault_state("vault-1.factory.testnet")
    
    replies = [call[0][0] for call in dummy_env.add_reply.call_args_list]
    base = replies[0]
    liquidity = replies[1]
    
    # Base vault metadata
    assert "vault-1.factory.testnet" in base
    assert "Owner" in base
    
    # Liquidity request summary
    assert "**📦 Liquidity Request Summary**" in liquidity
    assert "usdc.testnet" in liquidity
    assert "**120.00 USDC**" in liquidity
    assert "**2.50 USDC**" in liquidity
    assert "**5.00000 NEAR**" in liquidity
    assert "1d" in liquidity  # human-readable 86400s
    assert "2024-01-01" in liquidity  # formatted date
    
    
def test_vault_state_with_offer(mock_setup, minimal_vault_state):
    """
    Should correctly display accepted offer summary when present in vault state.

    Verifies:
    - Vault summary includes correct vault ID and fields
    - Accepted offer block is rendered with lender and acceptance timestamp
    - Human-readable formatting applied to timestamp
    """
    
    (dummy_env, mock_near) = mock_setup
    mock_near.view = AsyncMock(return_value=MagicMock(result=with_offer(minimal_vault_state)))
    
    vault.vault_state("vault-2.factory.testnet")
    
    replies = [call[0][0] for call in dummy_env.add_reply.call_args_list]
    base = replies[0]
    offer = replies[1]
    
    assert "vault-2.factory.testnet" in base
    assert "Accepted Offer" in base
    assert "bob.testnet" in offer
    assert "2024-01-02" in offer
    assert "**🤝 Accepted Offer Summary**" in offer


def test_vault_state_with_liquidation(mock_setup, minimal_vault_state):
    """
    Should correctly display liquidation summary when liquidation is active.

    Verifies:
    - All vault base fields render correctly
    - Liquidity request and accepted offer blocks are present
    - Liquidation section shows total debt, liquidated amount, and outstanding debt
    - All NEAR amounts are formatted as human-readable
    """
    
    (dummy_env, mock_near) = mock_setup
    state = with_liquidation(with_offer(with_liquidity(minimal_vault_state)))
    mock_near.view = AsyncMock(return_value=MagicMock(result=state))
    
    vault.vault_state("vault-3.factory.testnet")
    
    replies = [call[0][0] for call in dummy_env.add_reply.call_args_list]
    base = replies[0]
    liquidity = replies[1]
    offer = replies[2]
    liquidation = replies[3]
    
    assert "vault-3.factory.testnet" in base
    assert "**📦 Liquidity Request Summary**" in liquidity
    assert "**🤝 Accepted Offer Summary**" in offer
    assert "**⚠️ Liquidation Summary**" in liquidation
    assert "Liquidated" in liquidation
    assert "Outstanding Debt" in liquidation


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