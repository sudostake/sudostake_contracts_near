import json
from .context import get_env
from helpers import (
    vector_store_id, top_doc_chunks
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
    
    chunks = top_doc_chunks(env, vs_id, msgs[-1]["content"])
    
    # Output the result
    env.add_reply(json.dumps(chunks, indent=2))
