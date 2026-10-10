# Automatic updates

Every installed copy of Termbook checks once a day (only when the school
allows the internet) for a newer signed release on this repository's GitHub
releases. When one exists, the School Admin sees it on the dashboard and in
Settings → Internet & updates, and presses **Install**: Termbook takes a
backup, downloads the installer, checks its signature, installs it and
restarts. Nothing is ever installed without a valid signature, and a school
is never restarted without someone pressing Install.

## One-time setup (the owner)

1. Make the updater key pair on your own computer:

       npx tauri signer generate -w %USERPROFILE%\.termbook\updater.key

   Choose a password. Keep `updater.key` and its password like the licence
   key: never in the repository, backed up somewhere safe. Lose it and
   installed schools can no longer be updated (they keep working).

2. Put the **public** key (the text it prints, also in `updater.key.pub`) into
   `src-tauri/updater.pub`, commit and push. Builds made before this have
   updates switched off.

3. On GitHub: repository → Settings → Secrets and variables → Actions → New
   repository secret:
   * `TAURI_SIGNING_PRIVATE_KEY`: the whole content of `updater.key`
   * `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password

## Releasing a version

1. Raise the version in three places, the same number:
   `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`.
2. Commit, then tag and push the tag:

       git tag v0.2.0
       git push origin v0.2.0

3. The Release workflow builds the installer, signs it and publishes the
   release with `latest.json`. Within a day every school with internet sees
   the update.

Schools already running a build without the public key need one manual
install of a build that has it; after that, updates are automatic.
