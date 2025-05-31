import sys
import os
import pytest
from unittest.mock import AsyncMock, MagicMock

# Make src/ importable
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "../src")))

import helpers # type: ignore
from tools import ( # type: ignore[import]
    context,
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


def _make_dummy_resp(json_body):
    """Minimal stub mimicking requests.Response for our needs."""
    class DummyResp:
        def raise_for_status(self):          # no-op ⇢ 200 OK
            pass
        def json(self):
            return json_body
    return DummyResp()


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


# ───────────────── view_pending_liquidity_requests tests ─────────────────

def test_view_pending_requests_success(monkeypatch, mock_setup):
    """
    Should format and display a list of pending liquidity requests
    when the Firebase API returns valid data.
    """
    
    (dummy_env, _) = mock_setup
    
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Patch requests.get inside liquidity_request module to return fake JSON list
    monkeypatch.setattr(
        liquidity_request.requests, "get",
        lambda url, params, timeout, headers: _make_dummy_resp(
            [
                {
                    "id": "vault-0.factory.testnet",
                    "owner": "owner.testnet",
                    "state": "pending",
                    "liquidity_request": {
                        "token": "usdc.tkn.primitives.testnet",
                        "amount": "300000000",
                        "interest": "30000000",
                        "collateral": "10000000000000000000000000",
                        "duration": 2592000
                    },
                }
            ]
        ),
    )
    
    # run the tool
    liquidity_request.view_pending_liquidity_requests()
    
    # Assertions
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "📋 Pending Liquidity Requests" in msg
    assert "vault-0.factory.testnet" in msg
    assert "300" in msg
    assert "30 days" in msg


def test_view_pending_requests_empty(monkeypatch, mock_setup):
    """
    Should display a confirmation message when no pending liquidity requests exist.
    """
    
    (dummy_env, _) = mock_setup
    
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Patch requests.get to return an empty list
    monkeypatch.setattr(
        liquidity_request.requests,
        "get",
        lambda url, params, timeout, headers: _make_dummy_resp([]),
    )
    
    # Run the tool
    liquidity_request.view_pending_liquidity_requests()
    
    # Assert success message for no requests
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "No pending liquidity requests found" in msg


def test_view_pending_requests_error(monkeypatch, mock_setup):
    """
    Should log a warning and send an error message when the Firebase API request fails.
    """
    
    (dummy_env, _) = mock_setup
    
    monkeypatch.setenv("NEAR_NETWORK", "testnet")
    
    # Patch requests.get to raise an exception
    def raise_error(*args, **kwargs):
        raise RuntimeError("firebase api unreachable")
    
    monkeypatch.setattr(
        liquidity_request.requests, "get", raise_error
    )
    
    # Run the tool
    liquidity_request.view_pending_liquidity_requests()
    
    # Assert that an error reply was sent
    dummy_env.add_reply.assert_called_once()
    msg = dummy_env.add_reply.call_args[0][0]
    assert "Failed to fetch pending liquidity requests" in msg
    assert "firebase api unreachable" in msg