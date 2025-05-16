from nearai.agents.environment import Environment
from helpers import ensure_loop, set_credentials
from tools import register_tools


def run(env: Environment):
    """
    Entrypoint called by NearAI at import time.

    Sets up the event loop, NEAR credentials, and registers all tools
    before handing control to NearAI's tool-runner.
    """

    # Ensure asynchronous primitives have an event loop to bind to.
    ensure_loop()

    # Set the NEAR connection using environment variables.
    near = set_credentials(env)
    
    # Register tools and get their definitions
    tool_defs = register_tools(env, near)
    
    # System prompt shown to the language model
    system_msg = {
        "role": "system",
        "content": "You help users interact with their SudoStake Vaults"
    }

    # Begin tool-driven interaction
    env.completions_and_run_tools(
        [system_msg] + env.list_messages(),
        tools=tool_defs,
    )


# Only invoke run(env) if NearAI has injected `env` at import time.
if "env" in globals():
    run(env)  # type: ignore[name-defined]
