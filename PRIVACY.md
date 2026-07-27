# Privacy

VISOR is designed to operate locally.

## Data processed on the device

Depending on platform capabilities, VISOR reads hardware identifiers, utilization,
temperatures, running process names and paths, local AI runtime metadata, and local
model filenames. This data is used only to render the monitor and perform actions
requested by the user.

## Data not collected

VISOR contains no analytics SDK, advertising SDK, account system, or remote
telemetry endpoint. It does not upload process lists, hardware data, model names,
prompts, or usage history to Benz Engineering.

VISOR does not read API keys or infer paid-account status. Cloud token and cost
data is unavailable unless a future user explicitly enables an authenticated,
read-only provider connector. Such connectors are outside the current beta.

Loopback runtime probes communicate only with services on the same device. Cloud
AI clients may independently communicate with their providers under those
providers' own privacy policies; VISOR does not intercept prompts or responses.

## Local persistence

Interface preferences such as the selected theme are stored locally. Generated
build artifacts and application data can be removed using the operating system's
normal uninstall and application-data controls.
