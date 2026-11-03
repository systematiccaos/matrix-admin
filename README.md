# Matrix Admin

A small desktop admin interface for a Synapse (Matrix) homeserver, built with Rust and [egui](https://github.com/emilk/egui). It supports multiple homeservers and lets you:

- Register new users using your homeserver's `registration_shared_secret`.
- Bootstrap or log in to get an admin access token, needed for the management features.
- List/search users, reset passwords, deactivate/reactivate, grant/revoke admin, and lock/unlock accounts.

## Screenshots

_Screenshots coming soon._

<!--
  Once available, drop PNGs into screenshots/ and reference them like:
  ![Settings tab](screenshots/settings.png)
  ![Register user tab](screenshots/register.png)
  ![Manage users tab](screenshots/manage.png)
-->

## Requirements

- Rust (stable), installable via [rustup](https://rustup.rs).
- A Synapse homeserver with a configured `registration_shared_secret` (see `registration_shared_secret` in `homeserver.yaml`).

## Running

```sh
cargo run
```

This opens the app with three tabs: **Settings**, **Register user**, and **Manage users**.

## First-time setup

1. Open the **Settings** tab.
2. Enter your homeserver's URL (e.g. `https://matrix.example.org`) and its `registration_shared_secret`, then click **Save settings**. You can optionally click **Test connection** to verify the URL is reachable.
3. Get an admin access token, needed for the **Manage users** tab, either by:
   - Entering an admin username/password and clicking **Bootstrap / fetch admin token** — this registers that account as an admin via the shared secret if it doesn't exist yet, or
   - Entering an existing admin account's username/password and clicking **Log in (existing account)**.

The resulting token is saved alongside your homeserver URL and shared secret.

## Multiple homeservers

Use the **Homeserver** dropdown at the top of any tab to switch between homeservers, or **+ Add homeserver** to create a new profile. Each profile has its own URL, shared secret, and admin token; switching profiles clears the user list and any in-progress form state so nothing leaks between servers.

## Registering users

The **Register user** tab creates a new account directly via the shared-secret registration flow — it does not require an admin token. Optionally check **Grant server admin** to register the account as an admin.

## Managing users

The **Manage users** tab requires an admin access token (see above). Use **Refresh** to list/search users, select one from the table, then:

- **Reset password** (optionally logging out their other devices)
- **Deactivate** (optionally erasing their personal data) or **Reactivate** (requires setting a new password)
- **Grant admin** / **Revoke admin**
- **Lock** / **Unlock**

## Where your data is stored

All homeserver profiles (URL, shared secret, admin token) are stored locally in `profiles.json` in the project directory. This file is git-ignored and never leaves your machine — the app only talks to the homeserver URL you configure.

If you used an earlier version of this app that stored a single homeserver's settings in `.env`, those values are automatically migrated into a `Default` profile the first time you run the new version.
