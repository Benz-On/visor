# Security policy

## Supported versions

VISOR is currently beta software. Security fixes are applied to the latest
published prerelease only.

## Reporting a vulnerability

Do not create a public issue for a suspected vulnerability.

After the repository becomes public, use GitHub's **Report a vulnerability**
button to open a private security advisory. Until private reporting is enabled,
contact `benz.on.contact@gmail.com` with:

- affected version and platform;
- reproducible steps or proof of concept;
- expected impact;
- any suggested mitigation.

Do not include real credentials, private prompts, or unrelated personal process
data. You will receive an acknowledgement as soon as practical. Please allow a
reasonable remediation window before public disclosure.

## Security boundaries

- Native telemetry uses local Tauri IPC.
- The browser development agent binds only to IPv4 loopback.
- Local runtime probes never target non-loopback addresses.
- Process actions validate PID confirmation and protected-process rules.
- Release signing keys and CI credentials must be stored only in GitHub secrets.
