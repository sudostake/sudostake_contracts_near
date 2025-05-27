import json

from .context import get_env
from helpers import (
    vector_store_id
)

def query_sudostake_docs() -> None:
    """
    vector store access to SudoStake docs
    """
    
    env = get_env()
    vs_id = vector_store_id()
    
    user_query = env.list_messages()[-1]["content"]
    
    # Query the Vector Store
    vector_results = env.query_vector_store(vs_id, user_query)
    
    # Output the result
    env.add_reply(json.dumps(vector_results, indent=2))
