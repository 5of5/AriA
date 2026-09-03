# Operator binaries

560 workspace crates, one per catalog identity. Each links `aria-json-telemetry`
and owns a frozen `spec.json`; the closed operator JSON is unique per crate,
the transform is shared. All 560 are also reachable from the single `work`
binary (`work --commands`), which is the deployed surface.

`publish = false`: these are build targets for hosts that want one executable
per identity, not published crates.
