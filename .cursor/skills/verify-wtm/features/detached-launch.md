# Detached launch

Users open the console-free `wtm-ui` application directly. Only one UI instance runs in a Windows session.

## Sub-features

- `ui-direct` opens a visible native window from a Windows GUI executable without console-suppression launch flags.
- `ui-single-instance` makes a second launch exit successfully without opening another window or disturbing the first.
- `ui-startup-error` shows fatal startup failures in a native dialog and exits unsuccessfully after dismissal.
- `ui-close` closes the owned window and exits its host cleanly.

## How to get to it (user POV)

- Run `wtm-ui` from a terminal or open `wtm-ui.exe` directly.
- Launch it again while the first window is open.
- Close the dashboard through its window close action or Exit menu.

`wtm --live` is removed. The registration check verifies invalid-usage exit code 2.

## Driving it with PowerShell

- Complete Launch and Doctor from the skill. No other WTM UI may be running in the Windows session.
- Run `pwsh -NoProfile -File .cursor/skills/verify-wtm/scripts/drive.ps1 -Check launch`.
- Require PE subsystem 2, a visible WTM window owned by the tested UI process, no shared console, and clean exit after close. Read `second_instance_rejected` in the fixture's `proof.json` as well as the first window's ownership and exit fields.
- The same drive starts an owned UI without USERPROFILE after closing the normal instance. Require the native dialog to explain that variable and exit code 1 after dismissal.
- Require driver PASS and retain its result, logs, and fixture proof after cleanup.

## Gotchas

- The single-instance mutex is session-wide. HOME and USERPROFILE isolation does not allow concurrent fixture dashboards.
- The guard runs before user configuration is loaded. A pre-existing UI can suppress the startup-error path too.
- Direct GUI launch does not release a separate CLI launcher because that entry point no longer exists.
