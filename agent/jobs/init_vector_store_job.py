import openai
import nearai
import time
from pathlib import Path
import json
from typing import List
from typing import Final

# Tweak these knobs if you want different behaviour
POLL_INTERVAL_S:    Final[int] = 2          # seconds between status checks
MAX_BUILD_MINUTES:  Final[int] = 10         # hard cap (to avoid endless loop)


def init_vector_store() -> None:
    """
    Create a NEAR-AI vector-store containing **every** Markdown file
    under *root*.

    Raises
    ------
    TimeoutError
        If the vector-store build fails to complete within
        ``MAX_BUILD_MINUTES``.
    RuntimeError
        If the vector-store ends in a non-"completed" status.
    """
    
    # Bootstrap the client
    config = nearai.config.load_config_file()
    auth = config["auth"]
    hub_url = config.get("api_url", "https://api.near.ai/v1")
    signature = json.dumps(auth)
    
    client = openai.OpenAI(base_url=hub_url, api_key=signature)
    
    # Gather *.md docs
    root = './agent/docs'
    md_paths: List[Path] = list(Path(root).rglob("*.md"))
    
    if not md_paths:
        raise FileNotFoundError(f"No Markdown files found under {Path(root).resolve()}")
    
    # Upload each file (binary mode)
    file_ids: List[str] = []
    for p in md_paths:
        print(f"↳ uploading {p.relative_to(root)}")
        with p.open("rb") as fh:                                # binary!
            f = client.files.create(file=fh, purpose="assistants")
            file_ids.append(f.id)
    
    # Create the vector store
    vs = client.vector_stores.create(
        name="sudostake-vector-store",
        file_ids=file_ids,
        # chunking_strategy=dict(chunk_overlap_tokens=400,
        #                        max_chunk_size_tokens=800),
    )
    
    print(f"⏳ building vector-store {vs.id} ({len(file_ids)} files)…")
    
    # Poll until every file is processed or we time-out
    deadline = time.monotonic() + MAX_BUILD_MINUTES * 60
    
    while time.monotonic() < deadline:
        status = client.vector_stores.retrieve(vs.id)
        
        if (status.file_counts.completed == len(file_ids)
                and status.status == "completed"):
            print("✅ vector-store ready!")
            break
        
        if status.status == "expired":
            raise RuntimeError(f"Vector-store {vs.id} failed to build: "
                               f"{status.last_error}")
        
        time.sleep(POLL_INTERVAL_S)
        
    else:
        raise TimeoutError(f"Vector-store {vs.id} build timed out after "
                           f"{MAX_BUILD_MINUTES} minutes")


# Call this function to initialise the vector store.
if __name__ == "__main__":
    try:
        init_vector_store()
    except Exception as e:
        print(f"Error initializing vector store: {e}")
        raise