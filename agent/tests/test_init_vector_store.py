import sys
import os
import pytest

from unittest.mock import MagicMock


# Make jobs/ importable
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '../jobs')))

import init_vector_store_job # type: ignore

@pytest.fixture
def _openai_client_mock(monkeypatch):
    """
    Patch helpers.openai.OpenAI with a MagicMock exposing just the bits
    init_vector_store() touches.  Tests can tweak attributes as needed.
    """
    
    client = MagicMock(name="OpenAIClient")
    
    # • files.create → returns objs that each carry a unique .id
    client.files.create.side_effect = [
        MagicMock(id=f"file_{i}") for i in range(1, 10)
    ]
    
    # • vector_stores.create → returns a VS obj with .id
    vs_obj = MagicMock(id="vs_1")
    client.vector_stores.create.return_value = vs_obj
    
    # • vector_stores.retrieve → default happy-path: first in_progress, then completed
    in_progress = MagicMock(
        file_counts=MagicMock(completed=0),
        status="in_progress",
        last_error=None,
    )
    completed = MagicMock(
        file_counts=MagicMock(completed=1),
        status="completed",
        last_error=None,
    )
    client.vector_stores.retrieve.side_effect = [in_progress, completed]
    
    monkeypatch.setattr(init_vector_store_job.openai, "OpenAI", MagicMock(return_value=client))
    return client


@pytest.fixture(autouse=True)
def _fake_nearai_config(monkeypatch):
    monkeypatch.setattr(
        init_vector_store_job.nearai.config,
        "load_config_file",
        lambda: {"api_url": "https://api.near.ai/", "auth": {"token": "dummy"}},
        raising=False,
    )

# ───────────────────────── init_vector_store ────────────────
def test_init_vector_store_success(tmp_path, monkeypatch, _openai_client_mock):
    # create a markdown file so the helper finds something
    docs_dir = tmp_path / "agent" / "docs"
    docs_dir.mkdir(parents=True, exist_ok=True)
    (docs_dir / "doc.md").write_text("# Demo docs")
    monkeypatch.chdir(tmp_path)
    
    init_vector_store_job.init_vector_store()
    
    # upload & create calls happen
    _openai_client_mock.files.create.assert_called()
    _openai_client_mock.vector_stores.create.assert_called_once()


def test_init_vector_store_no_md_files(tmp_path, monkeypatch, _openai_client_mock):
    monkeypatch.chdir(tmp_path)
    with pytest.raises(FileNotFoundError):
        init_vector_store_job.init_vector_store()


def test_init_vector_store_timeout(tmp_path, monkeypatch, _openai_client_mock):
    # create a markdown file so the helper finds something
    docs_dir = tmp_path / "agent" / "docs"
    docs_dir.mkdir(parents=True, exist_ok=True)
    (docs_dir / "doc.md").write_text("# Demo docs")
    monkeypatch.chdir(tmp_path)
    
    # force retrieve to always return in_progress
    in_progress = MagicMock(
        file_counts=MagicMock(completed=0),
        status="in_progress",
        last_error=None,
    )
    _openai_client_mock.vector_stores.retrieve.side_effect = [in_progress] * 5
    
    # make timeout immediate
    monkeypatch.setattr(init_vector_store_job, "MAX_BUILD_MINUTES", 0)
    
    with pytest.raises(TimeoutError):
        init_vector_store_job.init_vector_store()


def test_init_vector_store_expired(tmp_path, monkeypatch, _openai_client_mock):
    # create a markdown file so the helper finds something
    docs_dir = tmp_path / "agent" / "docs"
    docs_dir.mkdir(parents=True, exist_ok=True)
    (docs_dir / "doc.md").write_text("# Demo docs")
    monkeypatch.chdir(tmp_path)
    
    expired = MagicMock(
        file_counts=MagicMock(completed=0),
        status="expired",
        last_error="boom!",
    )
    
    _openai_client_mock.vector_stores.retrieve.side_effect = [expired]
    
    with pytest.raises(RuntimeError, match="failed to build"):
        init_vector_store_job.init_vector_store()
