# Security Policy

## Supported Versions

| Version | Supported |
| ------- | --------- |
| 0.1.x   | ✅ Yes |

## Reporting a Vulnerability

We take the security of VibeRemote seriously. If you discover a security vulnerability, please follow these steps:

### 1. Do Not Disclose Publicly

Please **do not** open a public GitHub issue for security vulnerabilities.

### 2. Contact Maintainers

Send a detailed report to the maintainers via GitHub's [private vulnerability reporting](https://github.com/pallab-js/vibe-remote/security/advisories/new).

Include:
- A description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

### 3. Response Timeline

- **Acknowledgment**: Within 48 hours
- **Assessment**: Within 1 week
- **Fix**: Within 2-4 weeks (depending on severity)
- **Public disclosure**: After fix is released

### 4. Severity Classification

| Level | Response Time | Examples |
|-------|---------------|-----------|
| **Critical** | 24-48 hours | Remote code execution, MITM attacks |
| **High** | 1 week | Auth bypass, data exfiltration |
| **Medium** | 2 weeks | Rate limiting bypass, info leakage |
| **Low** | 4 weeks | Minor configuration issues |

## Security Features

VibeRemote implements multiple layers of security:

- **Transport**: QUIC with TLS 1.3 encryption
- **Authentication**: Ed25519 identity system with Noise Protocol
- **Certificate Pinning**: SHA256 fingerprint verification
- **Access Control**: Consent-based remote control (secure by default)
- **Rate Limiting**: Input and clipboard command throttling
- **Key Storage**: Encrypted permissions (0o600) + memory zeroization
- **Audit Logging**: All security-sensitive operations logged

## Dependency Security

We use `cargo-audit` in our CI pipeline to scan for known vulnerabilities in Rust dependencies. If you identify a vulnerable dependency, please report it.

## Responsible Disclosure

We appreciate responsible disclosure and will credit reporters in our release notes (with permission).
