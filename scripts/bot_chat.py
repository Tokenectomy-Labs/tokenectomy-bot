#!/usr/bin/env python3
"""
Tmy-Joy / Tokenectomy Bot - Autonomous PR Discussion & M2M Conversational Agent
Listens to PR comments and bot activities (Dependabot, Renovate, CodeRabbit, Humans),
computes deterministic AST response using tb-cli, and posts reply directly to GitHub PR discussion.
"""

import argparse
import json
import os
import subprocess
import sys
import urllib.error
import urllib.request

KNOWN_BOTS = ["dependabot[bot]", "renovate[bot]", "coderabbitai[bot]", "copilot[bot]"]

def should_process(author: str, comment_body: str) -> bool:
    author_lower = author.lower()
    # Avoid infinite recursion: never respond to our own bot
    if "tokenectomy" in author_lower or "tmy-joy" in author_lower:
        return False

    body_lower = comment_body.lower()
    # Direct mentions
    if "@tmy-joy" in body_lower or "@tokenectomy-bot" in body_lower or "@tokenectomy" in body_lower:
        return True

    # Slash commands
    for line in comment_body.splitlines():
        trimmed = line.strip()
        if trimmed.startswith(("/help", "/status", "/explain", "/rules", "/ping", "/review", "/verify", "/tb")):
            return True

    # M2M Handshake for known automated bots
    if any(bot in author_lower for bot in KNOWN_BOTS):
        return True

    return False

def generate_reply_via_cli(repo: str, pr: int, author: str, comment_body: str, summary_file: str = None) -> str:
    # Look for built binary first
    bin_path = os.environ.get("TOKENECTOMY_BIN", "target/release/tokenectomy-bot")
    if not os.path.exists(bin_path):
        bin_path = "target/debug/tokenectomy-bot"

    cmd = []
    if os.path.exists(bin_path):
        cmd = [bin_path, "chat", "--repo", repo, "--pr", str(pr), "--author", author, "--comment", comment_body]
    else:
        # Fallback to cargo run
        cmd = ["cargo", "run", "--quiet", "-p", "tb-cli", "--", "chat", "--repo", repo, "--pr", str(pr), "--author", author, "--comment", comment_body]

    if summary_file and os.path.exists(summary_file):
        cmd.extend(["--summary-file", summary_file])

    try:
        res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, check=True)
        return res.stdout.strip()
    except subprocess.CalledProcessError as e:
        print(f"[tmy-joy] Error running chat CLI: {e.stderr}", file=sys.stderr)
        return ""

def post_github_comment(repo: str, pr: int, token: str, reply_text: str):
    url = f"https://api.github.com/repos/{repo}/issues/{pr}/comments"
    headers = {
        "Authorization": f"Bearer {token}",
        "User-Agent": "Tmy-Joy/1.0",
        "Content-Type": "application/json",
        "Accept": "application/vnd.github.v3+json"
    }

    payload = json.dumps({"body": reply_text}).encode("utf-8")
    req = urllib.request.Request(url, data=payload, headers=headers, method="POST")

    try:
        with urllib.request.urlopen(req) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            print(f"[tmy-joy] Successfully replied to PR #{pr}: {data.get('html_url')}")
    except urllib.error.HTTPError as e:
        print(f"[tmy-joy] HTTP error {e.code} while posting comment: {e.read().decode('utf-8', errors='replace')}", file=sys.stderr)
    except Exception as e:
        print(f"[tmy-joy] Failed to post comment: {e}", file=sys.stderr)

def main():
    parser = argparse.ArgumentParser(description="Tmy-Joy PR Conversation & M2M Bot Responder")
    parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY"), help="GitHub repository (owner/repo)")
    parser.add_argument("--pr", type=int, default=int(os.environ.get("PR_NUMBER", "0")) if os.environ.get("PR_NUMBER") else None, help="Pull Request number")
    parser.add_argument("--author", default=os.environ.get("PR_COMMENT_AUTHOR"), help="Comment author login")
    parser.add_argument("--comment", default=os.environ.get("PR_COMMENT_BODY"), help="Comment text")
    parser.add_argument("--token", default=os.environ.get("GITHUB_TOKEN"), help="GitHub authentication token")
    parser.add_argument("--summary-file", default="/tmp/tokenectomy-sticky-summary.md", help="Path to current sticky summary")

    args = parser.parse_args()

    if not args.repo or not args.pr or not args.author or not args.comment:
        print("[tmy-joy] Missing required parameters (--repo, --pr, --author, --comment). Skipping.", file=sys.stderr)
        return

    if not should_process(args.author, args.comment):
        print("[tmy-joy] Comment does not match trigger criteria. Skipping.")
        return

    reply = generate_reply_via_cli(args.repo, args.pr, args.author, args.comment, args.summary_file)
    if not reply:
        print("[tmy-joy] No response generated.")
        return

    print(f"[tmy-joy] Generated reply:\n{reply}\n")

    if args.token:
        post_github_comment(args.repo, args.pr, args.token, reply)
    else:
        print("[tmy-joy] Notice: No GITHUB_TOKEN provided; dry-run mode completed.")

if __name__ == "__main__":
    main()
