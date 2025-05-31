import sys
import os
import pytest
from unittest.mock import AsyncMock, MagicMock
from test_utils import make_dummy_resp

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
        lambda url, params, timeout, headers: make_dummy_resp(
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
        lambda url, params, timeout, headers: make_dummy_resp([]),
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


# ───────────────── accept_liquidity_request tests ─────────────────

def test_accept_liquidity_request_success(monkeypatch, mock_setup):
    """Should accept the liquidity request and show confirmation message."""
    
    env, mock_near = mock_setup
    
    # Mock vault state with valid liquidity request and no accepted offer
    vault_state = {
        "owner": "alice.testnet",
        "index": 1,
        "version": 3,
        "is_listed_for_takeover": False,
        "liquidity_request": {
            "token": "usdc.testnet",
            "amount": "500000000",
            "interest": "50000000",
            "collateral": "10000000000000000000000000",
            "duration": 2592000,
            "created_at": "1710000000000000000"
        },
        "accepted_offer": None,
    }
    
    # Patch near.view to return mocked vault state
    mock_near.view = AsyncMock(return_value=MagicMock(result=vault_state))
    
    # Patch near.call to simulate successful ft_transfer_call
    mock_near.call = AsyncMock(return_value=MagicMock(
        transaction=MagicMock(hash="tx123"),
        status={"SuccessValue": ""},
    ))
    
    # Patch token metadata lookup
    monkeypatch.setattr(
        liquidity_request, "get_token_metadata_by_contract",
        lambda contract_id: {
            "decimals": 6,
            "symbol": "USDC",
        }
    )
    
    # Patch Firebase indexer
    monkeypatch.setattr(
        liquidity_request, "index_vault_to_firebase",
        lambda vault_id: None
    )
    
    # Call the tool
    liquidity_request.accept_liquidity_request(
        vault_id="vault-0.factory.testnet"
    )
    
    # Assert final message includes success content
    env.add_reply.assert_called()
    msg = env.add_reply.call_args[0][0]
    assert "Accepted Liquidity Request" in msg
    assert "vault-0.factory.testnet" in msg
    assert "500" in msg  # formatted USDC amount
    assert "USDC" in msg
    assert "tx123" in msg


def test_accept_liquidity_request_no_request(monkeypatch, mock_setup):
    """Should show error if no liquidity request exists."""
    
    env, mock_near = mock_setup
    
    # Mock vault state with no liquidity request
    mock_near.view = AsyncMock(return_value=MagicMock(result={
        "owner": "alice.testnet",
        "index": 0,
        "version": 1,
        "is_listed_for_takeover": False,
        "liquidity_request": None,
        "accepted_offer": None,
    }))
    
    liquidity_request.accept_liquidity_request("vault-1.factory.testnet")
    
    env.add_reply.assert_called_once()
    msg = env.add_reply.call_args[0][0]
    assert "vault-1.factory.testnet" in msg
    assert "has no active liquidity request" in msg


def test_accept_liquidity_request_already_accepted(monkeypatch, mock_setup):
    """Should show error if a request has already been accepted."""
    
    env, mock_near = mock_setup
    
    # Mock vault state with accepted offer present
    mock_near.view = AsyncMock(return_value=MagicMock(result={
        "owner": "alice.testnet",
        "index": 0,
        "version": 1,
        "is_listed_for_takeover": False,
        "liquidity_request": {
            "token": "usdc.testnet",
            "amount": "100000000",
            "interest": "10000000",
            "collateral": "5000000000000000000000000",
            "duration": 2592000,
            "created_at": "1710000000000000000"
        },
        "accepted_offer": {
            "lender": "bob.testnet",
            "accepted_at": "1710001000000000000"
        }
    }))
    
    liquidity_request.accept_liquidity_request("vault-2.factory.testnet")
    
    env.add_reply.assert_called_once()
    msg = env.add_reply.call_args[0][0]
    assert "has no active liquidity request" in msg
    assert "already been accepted" in msg
    assert "vault-2.factory.testnet" in msg


def test_accept_liquidity_request_transfer_failure(monkeypatch, mock_setup):
    """Should show error if the ft_transfer_call fails with contract panic."""
    
    env, mock_near = mock_setup
    
    # Mock vault state with valid liquidity request
    mock_near.view = AsyncMock(return_value=MagicMock(result={
        "owner": "alice.testnet",
        "index": 0,
        "version": 1,
        "is_listed_for_takeover": False,
        "liquidity_request": {
            "token": "usdc.testnet",
            "amount": "250000000",
            "interest": "25000000",
            "collateral": "10000000000000000000000000",
            "duration": 2592000,
            "created_at": "1710000000000000000"
        },
        "accepted_offer": None
    }))
    
    # Mock ft_transfer_call failure (simulate contract panic)
    mock_near.call = AsyncMock(return_value=MagicMock(
        transaction=MagicMock(hash="tx_fail"),
        status={
            "Failure": {
                "ActionError": {
                    "kind": {
                        "FunctionCallError": {
                            "ExecutionError": "Smart contract panicked: Invalid offer"
                        }
                    }
                }
            }
        }
    ))
    
    # Patch token metadata to avoid lookup failure
    monkeypatch.setattr(
        liquidity_request, "get_token_metadata_by_contract",
        lambda contract_id: {
            "decimals": 6,
            "symbol": "USDC",
        }
    )
    
    liquidity_request.accept_liquidity_request("vault-3.factory.testnet")
    
    env.add_reply.assert_called_once()
    msg = env.add_reply.call_args[0][0]
    assert "Failed to accept liquidity request" in msg
    assert "Invalid offer" in msg
    assert "tx_fail" not in msg # failure path skips success tx formatting


def test_accept_liquidity_request_contract_not_deployed(monkeypatch, mock_setup):
    """Should show error if the vault contract is not deployed or returns no state."""
    
    env, mock_near = mock_setup
    
    # Simulate view call returning nothing
    mock_near.view = AsyncMock(return_value=MagicMock(result=None))
    
    liquidity_request.accept_liquidity_request("vault-4.factory.testnet")
    
    env.add_reply.assert_called_once()
    msg = env.add_reply.call_args[0][0]
    assert "No data returned for" in msg
    assert "vault-4.factory.testnet" in msg
    assert "Is the contract deployed?" in msg
    
