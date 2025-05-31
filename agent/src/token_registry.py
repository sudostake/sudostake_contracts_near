import os

# Canonical token registry per network
TOKEN_REGISTRY = {
     "testnet": {
         "usdc": {
            "symbol": "USDC",
            "contract": "usdc.tkn.primitives.testnet",
            "decimals": 6,
            "aliases": ["$", "usd", "usdc"],
        },
     },
     "mainnet": {
         "usdc": {
            "symbol": "USDC",
            "contract": "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1",
            "decimals": 6,
            "aliases": ["$", "usd", "usdc"],
        },
    },
}

def get_token_metadata(token_key: str) -> dict:
    """
    Resolve canonical metadata for a whitelisted token using aliases.
    """
    
    network = os.getenv("NEAR_NETWORK", "testnet").lower()
    registry = TOKEN_REGISTRY.get(network)
    
    if registry is None:
        raise ValueError(f"Unsupported network: {network}")
    
    normalized_key = token_key.strip().lower()
    
    for metadata in registry.values():
        aliases = [a.lower() for a in metadata.get("aliases", [])]
        if normalized_key in aliases:
            return metadata
    
    raise ValueError(f"Unsupported token '{token_key}' on network '{network}'")
