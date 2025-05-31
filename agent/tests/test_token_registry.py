import sys
import os
import pytest

# Make src/ importable
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "../src")))

import token_registry # type: ignore


@pytest.fixture(autouse=True)
def set_testnet_env(monkeypatch):
    monkeypatch.setenv("NEAR_NETWORK", "testnet")

def test_get_token_metadata_by_key():
    meta = token_registry.get_token_metadata("usdc")
    assert meta["symbol"] == "USDC"
    assert meta["contract"] == "usdc.tkn.primitives.testnet"
    assert meta["decimals"] == 6


def test_get_token_metadata_by_alias_dollar():
    meta = token_registry.get_token_metadata("$")
    assert meta["symbol"] == "USDC"


def test_get_token_metadata_by_alias_usd():
    meta = token_registry.get_token_metadata("usd")
    assert meta["symbol"] == "USDC"


def test_get_token_metadata_case_insensitive():
    meta = token_registry.get_token_metadata("USDC")
    assert meta["symbol"] == "USDC"


def test_get_token_metadata_invalid_token():
    with pytest.raises(ValueError) as excinfo:
        token_registry.get_token_metadata("solana")
    assert "Unsupported token" in str(excinfo.value)


def test_get_token_metadata_invalid_network(monkeypatch):
    monkeypatch.setenv("NEAR_NETWORK", "unknownnet")
    with pytest.raises(ValueError) as excinfo:
        token_registry.get_token_metadata("usdc")
    assert "Unsupported network" in str(excinfo.value)


# ───────────────── get_token_metadata_by_contract tests ─────────────────
def test_get_token_metadata_by_contract_valid():
    meta = token_registry.get_token_metadata_by_contract("usdc.tkn.primitives.testnet")
    assert meta["symbol"] == "USDC"
    assert meta["decimals"] == 6


def test_get_token_metadata_by_contract_invalid():
    with pytest.raises(ValueError) as excinfo:
        token_registry.get_token_metadata_by_contract("invalid.contract.testnet")
    assert "No token metadata found for contract" in str(excinfo.value)


def test_get_token_metadata_by_contract_invalid_network(monkeypatch):
    monkeypatch.setenv("NEAR_NETWORK", "invalidnet")
    
    with pytest.raises(ValueError) as excinfo:
        token_registry.get_token_metadata_by_contract("usdc.tkn.primitives.testnet")
    
    assert "Unsupported network" in str(excinfo.value)