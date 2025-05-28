import json

from nearai.agents.environment import Environment
from helpers import ensure_loop, init_near, init_vector_store, vector_store_id, top_doc_chunks
from tools import register_tools


def run(env: Environment):
    """
    Entry-point invoked by NEAR AI Agents Hub.

    * Sets up a stable asyncio loop.
    * Initialises the NEAR connection in either:
        - headless-creds mode      (private key in secrets)
        - wallet-signer mode       (browser wallet attached)
        - read-only / onboarding   (neither present)
    * Registers all SudoStake tools.
    * Boots the LM with a system prompt + optional onboarding hint.
    """

    # Ensure asynchronous primitives have an event loop to bind to.
    ensure_loop()

    # Configure NEAR (returns py-near Account + headless flag)
    near = init_near(env)
    
    # Register tools and get their definitions
    tool_defs = register_tools(env, near)
    
    # Query the Vector Store
    messages = env.list_messages()
    user_query = messages[-1]["content"]
    docs = top_doc_chunks(env, vector_store_id(), user_query)

    # Init prompt list with system message
    prompt_list = [
        {
            "role": "system",
            "content": "You are SudoStake's AI Agent. "
                    "Help users inspect or manage their vaults on NEAR."
        },
        {
            "role": "documentation",
            "content": json.dumps(docs),
        },
    ]
    
    # Append any prior conversation history supplied by the Hub
    prompt_list.extend(messages)

    # Begin tool-driven interaction
    env.completions_and_run_tools(
        prompt_list,
        tools=tool_defs,
    )


# Only invoke run(env) if NearAI has injected `env` at import time.
if "env" in globals():
    run(env)  # type: ignore[name-defined]
