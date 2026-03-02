#!/usr/bin/env python3
import subprocess
import sys
from pathlib import Path


def main() -> int:
    repo_root = Path(__file__).resolve().parent.parent
    extracted_root = (repo_root.parent / "history-auditor").resolve()
    script = extracted_root / "scripts" / "verify_history_auditor_contract.py"
    if not script.is_file():
        print(
            f"history-auditor-contract: FAIL: missing extracted verifier at {script}",
            file=sys.stderr,
        )
        return 1
    return subprocess.call([sys.executable, str(script)], cwd=extracted_root)


if __name__ == "__main__":
    raise SystemExit(main())
