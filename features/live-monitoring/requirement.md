# Monitoring starts without hanging the app

Contract: Start scanning when the app opens and keep Git work off the interface thread.

Acceptance criteria:

- Startup begins monitoring and refreshing rather than hanging.
- Git work runs in a separate thread or process from the interface.
