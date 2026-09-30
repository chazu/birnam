# Historical measurement evidence

New raw measurements belong under `.cargo-target/measurements` or the validation
runner's output directory. Check in a compact decision and reproducible command;
CI retains logs as artifacts. Do not embed complete child matrices repeatedly in
new documentation receipts.

All 376 historical receipts are stored in `archive-20260914.zip`, with compact
JSON stubs at their original paths. Original payloads total 124,603,358 bytes;
the lossless archive is 14,619,943 bytes. Each stub binds the archive member, original
byte length and SHA-256, and retains a short verdict. `measurement_archive.load_json`
and `measurement_archive.read_bytes` resolve both ordinary files and archived
receipts; hashing the resolved bytes preserves historical proof references.

`python3 scripts/measurement-archive.py verify` checks all archived payloads.
`python3 scripts/measurement-archive.py extract --output .cargo-target/measurements/history`
restores exact original files for external tools that do not understand stubs.
Never edit a historical archive payload to make old qualification pass. This
migration affects the working tree; it does not rewrite Git history.

For current commands see [validation](../validation.md), and for current
optimization decisions see [74AK](../archive/design/74ak-bounded-qualification-and-defaults.md).

The [Design 81 closeout receipt](81-production-closeout.json) records the final
debug checks, current installation digests and disposition of local scratch
artifacts for the [closed production extraction phase](../archive/design/81-production-extraction-status.md).
It is a compact current receipt, not part of the historical measurement archive.
