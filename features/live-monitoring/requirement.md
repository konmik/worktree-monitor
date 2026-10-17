# Monitoring starts without hanging the app

Contract: Monitoring starts when the app opens, with Git work outside the interface thread.

Acceptance criteria:

- Monitoring and refreshing begin at startup without hanging the app.
- Git work runs in a separate thread or process from the interface.
