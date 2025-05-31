import sys
import os
import pytest
from unittest.mock import AsyncMock, MagicMock

# Make src/ importable
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "../src")))

import helpers # type: ignore
from tools import ( # type: ignore[import]
    context,
    vault,
    liquidity_request,
)

@pytest.fixture
def mock_setup():
    """Initialize mock environment, logger, and near — then set context."""
    
    env = MagicMock()
    near = MagicMock()

    # Set the context globally for tools
    context.set_context(env=env, near=near)

    return (env, near)


def test_request_liquidity_success(monkeypatch, mock_setup):
    """Should send a successful liquidity request and show confirmation."""
    
    env, mock_near = mock_setup
    
    mock_near.call = AsyncMock(return_value=MagicMock(
        transaction=MagicMock(hash="tx123"),
        logs=[],
        status={"SuccessValue": ""},
    ))
    
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless")
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    liquidity_request.request_liquidity(
        vault_id="vault-0.factory.testnet",
        amount=500,
        denom="usdc",
        interest=50,
        duration=30,
        collateral=100,
    )
    
    env.add_reply.assert_called_once()
    msg = env.add_reply.call_args[0][0]
    assert "Liquidity Request Submitted" in msg
    assert "vault-0.factory.testnet" in msg
    assert "tx123" in msg


def test_request_liquidity_contract_panic(monkeypatch, mock_setup):
    """Should detect contract panic and return a failure message."""
    
    env, mock_near = mock_setup
    
    mock_near.call = AsyncMock(return_value=MagicMock(
        transaction=MagicMock(hash="tx456"),
        logs=[],
        status={
            "Failure": {
                "ActionError": {
                    "kind": {
                        "FunctionCallError": {
                            "ExecutionError": "Smart contract panicked: A request is already open",
                        }
                    }
                }
            }
        }
    ))
    
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless")
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    liquidity_request.request_liquidity(
        vault_id="vault-0.factory.testnet",
        amount=200,
        denom="usdc",
        interest=25,
        duration=14,
        collateral=50,
    )
    
    env.add_reply.assert_called_once()
    msg = env.add_reply.call_args[0][0]
    assert "Liquidity Request failed" in msg
    assert "contract panic" in msg
    assert "A request is already open" in msg


def test_request_liquidity_insufficient_stake_log(monkeypatch, mock_setup):
    """Should detect soft failure from logs and return a helpful error."""
    
    env, mock_near = mock_setup
    
    mock_near.call = AsyncMock(return_value=MagicMock(
        transaction=MagicMock(hash="tx789"),
        logs=[
            'EVENT_JSON:{"event":"liquidity_request_failed_insufficient_stake"}'
        ],
        status={"SuccessValue": ""},
    ))
    
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless")
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    liquidity_request.request_liquidity(
        vault_id="vault-0.factory.testnet",
        amount=150,
        denom="usdc",
        interest=15,
        duration=10,
        collateral=40,
    )
    
    env.add_reply.assert_called_once()
    msg = env.add_reply.call_args[0][0]
    assert "Liquidity Request failed" in msg
    assert "You may not have enough staked NEAR" in msg


def test_request_liquidity_invalid_token_denom(monkeypatch, mock_setup):
    """Should raise a token resolution error and send a failure reply."""
    
    env, _ = mock_setup  # near is not needed here
    
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless")
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    liquidity_request.request_liquidity(
        vault_id="vault-0.factory.testnet",
        amount=100,
        denom="sol",  # ❌ not in TOKEN_REGISTRY
        interest=10,
        duration=7,
        collateral=20,
    )
    
    env.add_reply.assert_called_once()
    msg = env.add_reply.call_args[0][0]
    assert "Liquidity request failed" in msg
    assert "Unsupported token" in msg


def test_request_liquidity_indexing_failure(monkeypatch, mock_setup):
    """Should still succeed even if Firebase indexing fails."""
    
    env, mock_near = mock_setup
    
    mock_near.call = AsyncMock(return_value=MagicMock(
        transaction=MagicMock(hash="tx999"),
        logs=[],
        status={"SuccessValue": ""},
    ))
    
    monkeypatch.setattr(helpers, "_SIGNING_MODE", "headless")
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    def raise_index_error(vault_id):
        raise Exception("Firebase is down")
    
    monkeypatch.setattr(helpers, "index_vault_to_firebase", raise_index_error)
    
    liquidity_request.request_liquidity(
        vault_id="vault-9.factory.testnet",
        amount=250,
        denom="usdc",
        interest=10,
        duration=5,
        collateral=30,
    )
    
    env.add_reply.assert_called_once()
    msg = env.add_reply.call_args[0][0]
    assert "Liquidity Request Submitted" in msg
    assert "vault-9.factory.testnet" in msg
    assert "tx999" in msg

