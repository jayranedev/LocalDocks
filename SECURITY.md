# Security Policy

## Security Philosophy

Security is an important part of LocalDocks.

Because LocalDocks interacts with processes, ports, and other operating-system resources, security-sensitive functionality will be designed with the principle of least privilege in mind.

## Supported Versions

LocalDocks is currently under active development.

During the pre-release development period, security fixes will generally target the latest development version.

Once stable releases are available, supported versions will be documented here.

## Reporting a Vulnerability

Please do not publicly disclose security vulnerabilities before they have been reviewed by the maintainers.

When GitHub private vulnerability reporting is enabled for the repository, please use it to report security issues.

Security reports should include:

- A clear description of the vulnerability
- Steps required to reproduce it
- The potential impact
- Relevant logs or screenshots
- Any suggested mitigation, if available

Please avoid including passwords, tokens, personal information, or other sensitive data in reports.

## Security Considerations

LocalDocks may eventually interact with:

- Running processes
- Network ports
- Local files
- Development services
- Terminal processes
- Docker
- WSL
- Other system resources

The application will avoid requesting elevated privileges unless they are genuinely required for a specific operation.
