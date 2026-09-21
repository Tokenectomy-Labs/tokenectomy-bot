#!/usr/bin/env python3
"""
Tokenectomy Bot - Sticky PR Comment Poster
Searches for an existing sticky comment in the PR and updates it in place (PATCH),
or creates a new comment (POST) if none exists.
"""

import argparse
import json
import sys
import urllib.request
import urllib.error

STICKY_MARKER = "<!-- tokenectomy-bot:sticky-summary -->"

def post_or_update_comment(repo: str, pr_number: int, token: str, summary_content: str):
    headers = {
        "Authorization": f"Bearer {token}",
        "User-Agent": "Tmy-Joy/1.0",
        "Content-Type": "application/json",
        "Accept": "application/vnd.github.v3+json"
    }

    # Detect if PR author is dependabot[bot] for M2M handover
    pr_author = None
    try:
        pr_url = f"https://api.github.com/repos/{repo}/pulls/{pr_number}"
        req_pr = urllib.request.Request(pr_url, headers=headers)
        with urllib.request.urlopen(req_pr) as resp:
            pr_data = json.loads(resp.read().decode("utf-8"))
            pr_author = pr_data.get("user", {}).get("login")
    except Exception as e:
        print(f"[tmy-joy] Notice: Could not inspect PR author: {e}", file=sys.stderr)

    # If Dependabot PR, append M2M Handover tag
    if pr_author == "dependabot[bot]":
        if "BLOCKED" in summary_content:
            summary_content += "\n\n---\n🤖 **M2M Handover**: Quality gate failed. @dependabot recreate\n"
        elif "PASSED" in summary_content:
            summary_content += "\n\n---\n🤖 **M2M Handover**: All deterministic quality checks passed. @dependabot squash and merge\n"

    # 1. Fetch existing comments to find sticky comment ID
    existing_comment_id = None
    try:
        url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments?per_page=100"
        req = urllib.request.Request(url, headers=headers)
        with urllib.request.urlopen(req) as resp:
            comments = json.loads(resp.read().decode("utf-8"))
            for c in comments:
                if STICKY_MARKER in c.get("body", ""):
                    existing_comment_id = c.get("id")
                    break
    except Exception as e:
        print(f"[tmy-joy] Warning: Failed to query existing PR comments: {e}", file=sys.stderr)

    payload = json.dumps({"body": summary_content}).encode("utf-8")

    # 2. Update existing or create new comment
    if existing_comment_id:
        update_url = f"https://api.github.com/repos/{repo}/issues/comments/{existing_comment_id}"
        req = urllib.request.Request(update_url, data=payload, headers=headers, method="PATCH")
        action_name = "updated sticky comment"
    else:
        create_url = f"https://api.github.com/repos/{repo}/issues/{pr_number}/comments"
        req = urllib.request.Request(create_url, data=payload, headers=headers, method="POST")
        action_name = "created sticky comment"

    try:
        with urllib.request.urlopen(req) as resp:
            res = json.loads(resp.read().decode("utf-8"))
            print(f"[tokenectomy-bot] Successfully {action_name}: {res.get('html_url')}")
    except urllib.error.HTTPError as e:
        print(f"[tokenectomy-bot] HTTP error {e.code} while posting PR comment: {e.read().decode('utf-8', errors='replace')}", file=sys.stderr)
    except Exception as e:
        print(f"[tokenectomy-bot] Unexpected error while posting PR comment: {e}", file=sys.stderr)

def main():
    parser = argparse.ArgumentParser(description="Post sticky PR comment for Tokenectomy Bot")
    parser.add_argument("--repo", required=True, help="Repository name (e.g. owner/repo)")
    parser.add_argument("--pr", required=True, type=int, help="Pull request number")
    parser.add_argument("--token", required=True, help="GitHub authentication token")
    parser.add_argument("--summary-file", required=True, help="Path to markdown summary file")

    args = parser.parse_args()

    try:
        with open(args.summary_file, "r", encoding="utf-8") as f:
            summary_content = f.read()
    except Exception as e:
        print(f"[tokenectomy-bot] Failed to read summary file: {e}", file=sys.stderr)
        sys.exit(0) # Fail open

    if not summary_content.strip():
        print("[tokenectomy-bot] Summary file is empty. Skipping comment.", file=sys.stderr)
        return

    post_or_update_comment(args.repo, args.pr, args.token, summary_content)

if __name__ == "__main__":
    main()
