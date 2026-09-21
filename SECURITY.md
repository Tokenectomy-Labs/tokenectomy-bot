# Security Policy

## Supported Versions

Tokenectomy Labs actively maintains security patches for the following versions of `tokenectomy-bot` (Tmy-Joy):

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1.0 | :x:                |

## Reporting a Vulnerability

We take the security of Tokenectomy Labs infrastructure and our users very seriously. If you discover a security vulnerability, please do **NOT** open a public issue.

### Disclosure Process

1. **Email Reports:** Please send a detailed vulnerability report to:
   - **Primary Security Contact:** `daffaanan11@gmail.com`
   - **Subject Line:** `[SECURITY] Vulnerability Report: tokenectomy-bot`

2. **Include Details:**
   - Detailed description of the vulnerability.
   - Steps or proof-of-concept (PoC) to reproduce the issue.
   - Potential impact on CI/CD pipelines or host runners.
   - Any proposed mitigations or patches.

### Response Timelines

- **Initial Acknowledgment:** Within **24–48 hours** of report receipt.
- **Triage & Severity Assessment:** Within **3 business days**.
- **Patch Release & Advisory:** A coordinated patch and public CVE/GHSA advisory will be published once the fix is verified.

## Supply Chain Integrity & Automated Auditing

- All releases are deterministically compiled using safe Rust and signed.
- Workflows adhere to the OpenSSF Scorecard supply-chain security guidelines, including pinned action commit hashes and minimal token permissions.
