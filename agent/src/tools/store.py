import json
from .context import get_env
from helpers import (
    vector_store_id
)

def query_sudostake_docs() -> None:
    """Answer the user with the top vector-store chunks."""
    
    env = get_env()
    vs_id = vector_store_id()
    
    if not vs_id:
        env.add_reply("Vector store not initialised. Run /build_docs first.")
        return
    
    msgs = env.list_messages()
    if not msgs:
        env.add_reply("No query provided.")
        return
    
    user_query = msgs[-1]["content"]
    
    # Query the Vector Store
    vector_results = env.query_vector_store(vs_id, user_query)
    top_hits       = vector_results[:6]     # keep prompt tidy
    
    # Output the result
    env.add_reply(json.dumps(top_hits, indent=2))
