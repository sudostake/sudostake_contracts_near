import sys
import os
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '../src')))

import pytest
from unittest.mock import AsyncMock, MagicMock
from decimal import Decimal
import tools # type: ignore[import-unresolved]


@pytest.fixture
def mock_near():
    mock = MagicMock()
    return mock

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
    
    result = tools.vault_state("vault-0.testnet")
    assert "✅ **Vault State" in result
    assert "`alice.near`" in result
    

def test_view_available_balance(monkeypatch, mock_near):
    yocto_balance = int(Decimal("1.25") * Decimal("1e24"))
    mock_near.view = AsyncMock(return_value=MagicMock(result=str(yocto_balance)))
    monkeypatch.setattr(tools, "_near", mock_near)

    result = tools.view_available_balance("vault-0.testnet")
    assert "**1.25000 NEAR**" in result
    

def test_delegate(monkeypatch, mock_near):
    mock_near.call = AsyncMock(return_value=MagicMock(
        transaction=MagicMock(hash="abc123"),
        transaction_outcome=MagicMock(gas_burnt=310_000_000_000_000),
        logs=[
            "EVENT_JSON:{\"event\":\"lock_acquired\"}",
            "vault deposited",
            "vault staking",
            "EVENT_JSON:{\"event\":\"lock_released\"}",
            "EVENT_JSON:{\"event\":\"delegate_completed\"}"
        ]
    ))
    monkeypatch.setattr(tools, "_near", mock_near)
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    result = tools.delegate("vault-0.testnet", "validator.near", "1")
    assert "✅ **Delegation Successful**" in result
    assert "🔒 Lock acquired" in result
    assert "📈 Stake successful" in result
    assert "📥 Deposit received" in result
    assert "✅ Delegate completed" in result
