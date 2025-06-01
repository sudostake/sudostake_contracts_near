import sys
import os
import pytest
from unittest.mock import MagicMock

# Make src/ importable
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "../src")))

from tools import ( # type: ignore[import]
    context,
)


def make_dummy_resp(json_body):
    """Minimal stub mimicking requests.Response for our needs."""
    class DummyResp:
        def raise_for_status(self):          # no-op ⇢ 200 OK
            pass
        def json(self):
            return json_body
    return DummyResp()

@pytest.fixture
def mock_setup():
    """Initialize mock environment, logger, and near — then set context."""
    
    env = MagicMock()
    near = MagicMock()

    # Set the context globally for tools
    context.set_context(env=env, near=near)

    return (env, near)