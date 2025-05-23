from py_near.account import Account
from nearai.agents.environment import Environment
from nearai.agents.models.tool_definition import MCPTool
from .context import set_context
from . import (
    balance,
    minting,
    transfer,
    delegation,
    vault,
    withdrawal,
    summary
)

# Register all tools here
def register_tools(env: Environment, near: Account) -> list[MCPTool]:
    """
    Register all SudoStake agent tools with the environment.
    Called from `tools/__init__.py`.
    """
    
    set_context(env, near)
    registry = env.get_tool_registry()
    registered_tools = []
    
    for tool in (
        vault.show_help_menu,
        balance.view_main_balance,
        balance.view_available_balance,
        minting.mint_vault,
        transfer.transfer_near_to_vault,
        vault.vault_state,
        delegation.delegate,
        delegation.undelegate,
        withdrawal.withdraw_balance,
        withdrawal.claim_unstaked_balance,
        summary.view_vault_status_with_validator,
        summary.vault_delegation_summary,
    ):
        registry.register_tool(tool)
        registered_tools.append(tool.__name__)
    
    return [
        registry.get_tool_definition(name)
        for name in registered_tools
    ]