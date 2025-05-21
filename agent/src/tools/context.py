import logging
import sys

from nearai.agents.environment import Environment
from py_near.account import Account
from logging import Logger


# Globals
_env: Environment = None  # type: ignore
_near: Account = None     # type: ignore

# Logger for this module
_logger = logging.getLogger(__name__)

# ensure logs show up
logging.basicConfig(stream=sys.stdout, level=logging.INFO,
                    format="%(asctime)s %(levelname)s %(name)s: %(message)s")


def set_context(env: Environment, near: Account) -> None:
    global _env, _near
    _env = env
    _near = near


def get_env() -> Environment:
    return _env


def get_near() -> Account:
    return _near


def get_logger() -> Logger:
    return _logger